//! Migração 5 e a impressão do texto: a migração a partir da v4 (com cópia antes e sem refazer
//! nada ao rodar de novo), os gatilhos, quem grava o texto e a busca do original mais antigo.
//! Dados fictícios.
#![allow(clippy::unwrap_used)]

use bancada_armazem::Armazem;
use rusqlite::{Connection, params};
use sbf_inventario::banco::{EstadoDoBanco, Inventario, pragmas};
use sbf_inventario::impressao_do_texto::{
    self as impressao, MINIMO_DE_CARACTERES, completar_pendentes, original_do_mesmo_texto,
    preencher_tudo,
};
use sbf_inventario::{migracoes, texto};

fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn recibo() -> String {
    "Recibo de pagamento Exemplo S.A. referente a assinatura mensal ficticia. ".repeat(4)
}

/// O mesmo texto com outras quebras e espaços (bytes diferentes, impressão igual).
fn recibo_regerado() -> String {
    recibo().replace(' ', " \r\n  ")
}

fn achado(c: &Connection, n: u8, visto_em: &str) {
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, primeiro_visto_em)
         VALUES (?1, 10, 'A', ?2)",
        params![hash(n), visto_em],
    )
    .unwrap();
}

fn texto_de(c: &Connection, n: u8, conteudo: &str) {
    c.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', ?2)",
        params![hash(n), conteudo],
    )
    .unwrap();
}

fn impressao_de(c: &Connection, n: u8) -> Option<Option<String>> {
    use rusqlite::OptionalExtension;
    c.query_row(
        "SELECT impressao FROM impressao_do_texto WHERE content_hash = ?1",
        [hash(n)],
        |l| l.get(0),
    )
    .optional()
    .unwrap()
}

fn pendente(c: &Connection, n: u8) -> bool {
    c.query_row(
        "SELECT pendente FROM impressao_do_texto WHERE content_hash = ?1",
        [hash(n)],
        |l| l.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

/// Um banco na versão 4, com a escada parada ali (como o cofre real estava antes).
fn banco_na_v4(pasta: &std::path::Path) -> std::path::PathBuf {
    let caminho = pasta.join("inventario.sqlite");
    let mut armazem = Armazem::abrir(&caminho, &pragmas()).unwrap();
    armazem.migrar_ate(&migracoes::escada(), 4).unwrap();
    assert_eq!(armazem.versao().unwrap(), 4);
    caminho
}

fn migrado(pasta: &std::path::Path) -> Inventario {
    let mut inv = Inventario::abrir(pasta.join("inventario.sqlite")).unwrap();
    inv.migrar(pasta.join("copia.sqlite")).unwrap();
    inv
}

#[test]
fn migrar_da_v4_para_a_v5_copia_antes_e_preenche_as_impressoes_que_ja_existem() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_na_v4(pasta.path());
    let antiga = Connection::open(&caminho).unwrap();
    antiga.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    for n in 1..=4 {
        achado(&antiga, n, "2026-10-01T10:00:00.000Z");
    }
    texto_de(&antiga, 1, &recibo());
    texto_de(&antiga, 2, &recibo_regerado());
    texto_de(&antiga, 3, "foto sem texto");
    texto_de(&antiga, 4, "");
    // Antes da migração 5 a tabela não existe.
    assert!(!impressao::existe_a_tabela(&antiga));
    drop(antiga);

    let mut inv = Inventario::abrir(&caminho).unwrap();
    assert_eq!(inv.estado(), EstadoDoBanco::Rust(4));
    let plano = inv.plano().unwrap();
    assert_eq!(plano.versao_final, 5);
    assert_eq!(plano.passos.len(), 1);
    assert!(plano.faz_copia);
    let copia = inv.caminho_sugerido_da_copia();
    assert!(copia.to_string_lossy().contains("antes-da-v5-"));
    let feita = inv.migrar(&copia).unwrap();
    assert_eq!((feita.versao_inicial, feita.versao_final), (4, 5));
    assert_eq!(feita.copia.as_deref(), Some(copia.as_path()));

    // A cópia é o banco de antes: v4, sem a tabela, com os textos.
    let c = Connection::open(&copia).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |l| l.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert!(!impressao::existe_a_tabela(&c));
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM texto", [], |l| l.get::<_, i64>(0))
            .unwrap(),
        4
    );

    let c = inv.conexao();
    assert_eq!(inv.versao().unwrap(), 5);
    let a = impressao_de(c, 1).expect("linha 1").expect("impressao 1");
    assert_eq!(a.len(), 64);
    // Bytes diferentes, mesma impressão.
    assert_eq!(impressao_de(c, 2), Some(Some(a)));
    // Curto e vazio têm linha, sem impressão, e nada pendente.
    assert_eq!(impressao_de(c, 3), Some(None));
    assert_eq!(impressao_de(c, 4), Some(None));
    assert!(!pendente(c, 1) && !pendente(c, 3));
}

#[test]
fn a_migracao_e_idempotente_e_rodar_de_novo_nao_muda_nada() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_na_v4(pasta.path());
    {
        let c = Connection::open(&caminho).unwrap();
        achado(&c, 1, "2026-10-01T10:00:00.000Z");
        texto_de(&c, 1, &recibo());
    }
    let mut inv = Inventario::abrir(&caminho).unwrap();
    inv.migrar(pasta.path().join("copia.sqlite")).unwrap();
    let antes = impressao_de(inv.conexao(), 1);

    // Reabrir e migrar de novo não tem o que fazer nem copia.
    let mut de_novo = Inventario::abrir(&caminho).unwrap();
    assert!(!de_novo.plano().unwrap().precisa_migrar());
    let feita = de_novo.migrar(pasta.path().join("outra.sqlite")).unwrap();
    assert_eq!(feita.copia, None);
    assert!(!pasta.path().join("outra.sqlite").exists());

    // O preenchimento em si também é idempotente (e o DDL tem IF NOT EXISTS).
    let c = inv.conexao();
    assert_eq!(preencher_tudo(c).unwrap(), 1);
    assert_eq!(preencher_tudo(c).unwrap(), 1);
    assert_eq!(impressao_de(c, 1), antes);
    c.execute_batch(sbf_inventario::esquema::MIGRACAO_5_IMPRESSAO_DO_TEXTO)
        .unwrap();
    assert_eq!(impressao_de(c, 1), antes);
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM impressao_do_texto", [], |l| l
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn banco_vazio_e_banco_do_bun_chegam_a_v5_com_a_tabela() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    assert_eq!(inv.versao().unwrap(), 5);
    assert!(impressao::existe_a_tabela(inv.conexao()));
    let indices: i64 = inv
        .conexao()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'
               AND name IN ('impressao_do_texto_igual', 'impressao_do_texto_pendente')",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert_eq!(indices, 2);
}

#[test]
fn quem_grava_texto_sem_passar_pelo_rust_deixa_pendente_e_o_rust_completa() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    // Como os scripts Bun gravam: um INSERT direto.
    texto_de(c, 1, &recibo());
    assert_eq!(impressao_de(c, 1), Some(None));
    assert!(pendente(c, 1));

    assert_eq!(completar_pendentes(c).unwrap(), 1);
    assert!(!pendente(c, 1));
    assert!(impressao_de(c, 1).unwrap().is_some());
    assert_eq!(completar_pendentes(c).unwrap(), 0);
}

#[test]
fn texto_que_muda_ou_some_desfaz_a_impressao() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    texto_de(c, 1, "texto curto de uma imagem");
    completar_pendentes(c).unwrap();
    assert_eq!(impressao_de(c, 1), Some(None));

    // O OCR do e3-ocr.ts troca o conteúdo (ON CONFLICT DO UPDATE).
    c.execute(
        "UPDATE texto SET metodo = 'ocr-por', conteudo = ?2 WHERE content_hash = ?1",
        params![hash(1), recibo()],
    )
    .unwrap();
    assert!(pendente(c, 1));
    completar_pendentes(c).unwrap();
    assert!(impressao_de(c, 1).unwrap().is_some());

    c.execute("DELETE FROM texto WHERE content_hash = ?1", [hash(1)])
        .unwrap();
    assert_eq!(impressao_de(c, 1), None);
}

#[test]
fn gravar_texto_calcula_a_impressao_na_hora() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    achado(c, 2, "2026-10-01T10:00:00.000Z");
    texto::registrar_texto(c, &hash(1), "pdftotext", &recibo(), None).unwrap();
    texto::registrar_texto(c, &hash(2), "ocr-tesseract", "curto", None).unwrap();
    assert!(!pendente(c, 1));
    assert!(impressao_de(c, 1).unwrap().is_some());
    assert_eq!(impressao_de(c, 2), Some(None));
    // A segunda gravação não troca o texto e, com ele, a impressão.
    let antes = impressao_de(c, 1);
    texto::registrar_texto(c, &hash(1), "ocr-por", "outro texto", None).unwrap();
    assert_eq!(impressao_de(c, 1), antes);
}

#[test]
fn gravar_texto_num_banco_sem_a_migracao_5_nao_falha() {
    let pasta = tempfile::tempdir().unwrap();
    let caminho = banco_na_v4(pasta.path());
    let c = Connection::open(&caminho).unwrap();
    achado(&c, 1, "2026-10-01T10:00:00.000Z");
    texto::registrar_texto(&c, &hash(1), "pdftotext", &recibo(), None).unwrap();
    assert_eq!(completar_pendentes(&c).unwrap(), 0);
    assert_eq!(original_do_mesmo_texto(&c, &hash(1)).unwrap(), None);
}

#[test]
fn o_mais_novo_aponta_para_o_mais_antigo_e_o_antigo_fica_como_esta() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    achado(c, 2, "2026-10-03T10:00:00.000Z");
    achado(c, 3, "2026-10-02T10:00:00.000Z");
    texto::registrar_texto(c, &hash(1), "pdftotext", &recibo(), None).unwrap();
    texto::registrar_texto(c, &hash(2), "pdftotext", &recibo_regerado(), None).unwrap();
    texto::registrar_texto(c, &hash(3), "pdftotext", &recibo(), None).unwrap();
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:x', 'disco', 'fictício')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
         VALUES (1, 'disco:x', 'E1', '/x', 't@1')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
                             local_filepath, filename)
         VALUES (?1, 'disco:x', 1, 'local_file', 'k1', '/x/recibo.pdf', 'recibo-original.pdf')",
        [hash(1)],
    )
    .unwrap();

    assert_eq!(original_do_mesmo_texto(c, &hash(1)).unwrap(), None);
    let do_3 = original_do_mesmo_texto(c, &hash(3)).unwrap().unwrap();
    assert_eq!(do_3.content_hash, hash(1));
    assert_eq!(do_3.nome, "recibo-original.pdf");
    // Com dois mais antigos, aponta para o mais antigo de todos.
    let do_2 = original_do_mesmo_texto(c, &hash(2)).unwrap().unwrap();
    assert_eq!(do_2.content_hash, hash(1));
}

#[test]
fn a_chegada_na_caixa_vale_mais_que_o_primeiro_visto() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    // Visto primeiro, mas chegou à Caixa depois: o outro é o mais antigo.
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    achado(c, 2, "2026-10-02T10:00:00.000Z");
    texto::registrar_texto(c, &hash(1), "pdftotext", &recibo(), None).unwrap();
    texto::registrar_texto(c, &hash(2), "pdftotext", &recibo(), None).unwrap();
    for (n, chegou) in [
        (1u8, "2026-11-10T10:00:00.000Z"),
        (2, "2026-11-09T10:00:00.000Z"),
    ] {
        c.execute(
            "INSERT INTO caixa_entrada (content_hash, nome, chegou_em, canal, estado)
             VALUES (?1, ?2, ?3, 'telegram', 'pronto')",
            params![hash(n), format!("doc-{n}.pdf"), chegou],
        )
        .unwrap();
    }
    assert_eq!(original_do_mesmo_texto(c, &hash(2)).unwrap(), None);
    let do_1 = original_do_mesmo_texto(c, &hash(1)).unwrap().unwrap();
    assert_eq!(
        (do_1.content_hash, do_1.nome),
        (hash(2), "doc-2.pdf".to_owned())
    );
}

#[test]
fn chegada_igual_desempata_pelo_hash_e_so_um_lado_e_marcado() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    achado(c, 2, "2026-10-01T10:00:00.000Z");
    texto::registrar_texto(c, &hash(1), "pdftotext", &recibo(), None).unwrap();
    texto::registrar_texto(c, &hash(2), "pdftotext", &recibo(), None).unwrap();
    let marcados = [1u8, 2]
        .into_iter()
        .filter(|n| original_do_mesmo_texto(c, &hash(*n)).unwrap().is_some())
        .count();
    assert_eq!(marcados, 1);
}

#[test]
fn texto_curto_ou_diferente_nunca_e_duplicado() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    for n in 1..=5 {
        achado(c, n, &format!("2026-10-0{n}T10:00:00.000Z"));
    }
    let curto = "x".repeat(MINIMO_DE_CARACTERES - 1);
    texto::registrar_texto(c, &hash(1), "ocr-tesseract", &curto, None).unwrap();
    texto::registrar_texto(c, &hash(2), "ocr-tesseract", &curto, None).unwrap();
    texto::registrar_texto(c, &hash(3), "pdftotext", &recibo(), None).unwrap();
    texto::registrar_texto(c, &hash(4), "pdftotext", &recibo().to_uppercase(), None).unwrap();
    texto::registrar_texto(
        c,
        &hash(5),
        "pdftotext",
        &recibo().replace("mensal", "anual"),
        None,
    )
    .unwrap();
    for n in 1..=5 {
        assert_eq!(original_do_mesmo_texto(c, &hash(n)).unwrap(), None, "{n}");
    }
}

/// A cláusula de conflito do comando de fora vence a do gatilho: o `INSERT OR IGNORE` e o
/// upsert dos scripts Bun precisam seguir valendo com a linha da impressão já existente.
#[test]
fn o_insert_or_ignore_e_o_upsert_do_bun_nao_quebram_com_a_linha_ja_existente() {
    let pasta = tempfile::tempdir().unwrap();
    let inv = migrado(pasta.path());
    let c = inv.conexao();
    achado(c, 1, "2026-10-01T10:00:00.000Z");
    c.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'sem-extrator:jpg', '')",
        [hash(1)],
    )
    .unwrap();
    completar_pendentes(c).unwrap();
    c.execute(
        "INSERT OR IGNORE INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', 'x')",
        [hash(1)],
    )
    .unwrap();
    assert!(!pendente(c, 1), "o INSERT ignorado não mexe na impressão");
    c.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'ocr-por', ?2)
         ON CONFLICT (content_hash) DO UPDATE SET
           metodo = excluded.metodo, conteudo = excluded.conteudo
         WHERE texto.metodo LIKE 'sem-extrator:%'",
        params![hash(1), recibo()],
    )
    .unwrap();
    assert!(pendente(c, 1));
    completar_pendentes(c).unwrap();
    assert!(impressao_de(c, 1).unwrap().is_some());
}
