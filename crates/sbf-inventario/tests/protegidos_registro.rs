//! `protegidos`: alvos, ordem das regras, tentativas sem senha, limite de falhas e a pendência
//! "PDF protegido sem regra", sobre um banco fictício montado pelo teste (INV-17).

mod nucleo_comum;

use nucleo_comum::{hash, inventario_migrado};
use rusqlite::{Connection, params};
use sbf_inventario::comando::{Acao, Comandos, PedidoDeDecisao};
use sbf_inventario::protegidos::{
    self, AlvoProtegido, Conclusao, ResultadoDaTentativa, TIPO_SEM_REGRA, TentativaFeita,
};
use sbf_inventario::{Ref, Tipo};

/// Achado protegido 0x50 + n, da "Fatura Alfa Fictícia".
fn protegido(conexao: &Connection, n: u8, fornecedor: Option<&str>, tipo: Option<&str>) -> String {
    let h = hash(0x50 + n);
    conexao
        .execute(
            "INSERT INTO achado (content_hash, file_size_bytes, camada, estado, document_type,
                                 supplier_name_raw, protegido_senha, caminho_acervo)
             VALUES (?1, 10, 'A', 'documento', ?2, ?3, 1, ?4)",
            params![h, tipo, fornecedor, format!("acervo/{n}.pdf")],
        )
        .expect("achado");
    h
}

fn regra(
    conexao: &Connection,
    fornecedor: &str,
    tipo: &str,
    estrategia: &str,
    prioridade: i64,
) -> i64 {
    conexao
        .execute(
            "INSERT INTO regra_senha (fornecedor, document_type, estrategia, prioridade)
             VALUES (?1, ?2, ?3, ?4)",
            params![fornecedor, tipo, estrategia, prioridade],
        )
        .expect("regra");
    conexao.last_insert_rowid()
}

fn alvo(conexao: &Connection, h: &str) -> AlvoProtegido {
    protegidos::alvos(conexao, 6)
        .expect("alvos")
        .into_iter()
        .find(|a| a.content_hash == h)
        .expect("é alvo")
}

fn errada(estrategia: &str, regra: Option<i64>) -> TentativaFeita {
    TentativaFeita {
        estrategia: estrategia.to_owned(),
        regra,
        resultado: ResultadoDaTentativa::SenhaErrada,
    }
}

#[test]
fn alvos_sao_os_protegidos_sem_texto_e_fora_descartados() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inv = inventario_migrado(pasta.path());
    let c = inv.conexao();
    let a = protegido(c, 1, Some("Banco Alfa"), Some("fatura"));
    let b = protegido(c, 2, None, None);
    let descartado = protegido(c, 3, None, None);
    c.execute(
        "UPDATE achado SET estado = 'descartado' WHERE content_hash = ?1",
        [&descartado],
    )
    .expect("descartar");
    let com_texto = protegido(c, 4, None, None);
    c.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?1, 'pdftotext', 'x')",
        [&com_texto],
    )
    .expect("texto");
    let mut hashes: Vec<String> = protegidos::alvos(c, 6)
        .expect("alvos")
        .into_iter()
        .map(|a| a.content_hash)
        .collect();
    hashes.sort();
    let mut esperado = vec![a.clone(), b];
    esperado.sort();
    assert_eq!(hashes, esperado);
    let alvo_a = alvo(c, &a);
    assert_eq!(alvo_a.fornecedor.as_deref(), Some("Banco Alfa"));
    assert_eq!(alvo_a.caminho_acervo.as_deref(), Some("acervo/1.pdf"));
}

#[test]
fn regras_do_par_por_prioridade_depois_as_do_fornecedor_com_asterisco() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inv = inventario_migrado(pasta.path());
    let c = inv.conexao();
    let estrela = regra(c, "Banco Alfa", "*", "cpf_completo", 1);
    let segunda = regra(c, "Banco Alfa", "fatura", "cpf_ultimos:5", 60);
    let primeira = regra(c, "banco alfa ", "fatura", "cpf_primeiros:3", 50);
    let _outro_tipo = regra(c, "Banco Alfa", "extrato", "cpf_pontuado", 1);
    let _outro_fornecedor = regra(c, "Banco Beta", "fatura", "cpf_pontuado", 1);
    let inativa = regra(c, "Banco Alfa", "fatura", "cpf_primeiros:4", 1);
    c.execute("UPDATE regra_senha SET ativa = 0 WHERE id = ?1", [inativa])
        .expect("desativar");

    let ids: Vec<i64> = protegidos::regras_para(c, Some("Banco Alfa"), Some("fatura"))
        .expect("regras")
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, [primeira, segunda, estrela]);
    // Sem tipo, só as do fornecedor com `*`; sem fornecedor, nenhuma.
    let so_estrela: Vec<i64> = protegidos::regras_para(c, Some("Banco Alfa"), None)
        .expect("regras")
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(so_estrela, [estrela]);
    assert!(
        protegidos::regras_para(c, None, Some("fatura"))
            .expect("regras")
            .is_empty()
    );
}

#[test]
fn tentativas_gravam_estrategia_e_resultado_e_os_contadores_da_regra() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inv = inventario_migrado(pasta.path());
    let c = inv.conexao();
    let h = protegido(c, 1, Some("Banco Alfa"), Some("fatura"));
    let r1 = regra(c, "Banco Alfa", "fatura", "cpf_primeiros:3", 10);
    let r2 = regra(c, "Banco Alfa", "fatura", "cpf_ultimos:5", 20);
    let alvo = alvo(c, &h);
    let registro = protegidos::registrar(
        c,
        &alvo,
        &[
            errada("cpf_primeiros:3", Some(r1)),
            TentativaFeita {
                estrategia: "cpf_ultimos:5".into(),
                regra: Some(r2),
                resultado: ResultadoDaTentativa::Abriu,
            },
        ],
        &Conclusao::Aberto {
            conteudo: "texto fictício".into(),
            impressao: "ab".repeat(32),
        },
        6,
    )
    .expect("registrar");
    assert_eq!(registro.falhas, 1);
    assert_eq!(registro.pendencia, None);

    let tentativas: Vec<(String, i64)> = c
        .prepare(
            "SELECT estrategia, sucesso FROM tentativa_senha WHERE content_hash = ?1 ORDER BY id",
        )
        .expect("sql")
        .query_map([&h], |l| Ok((l.get(0)?, l.get(1)?)))
        .expect("tentativas")
        .collect::<Result<_, _>>()
        .expect("linhas");
    assert_eq!(
        tentativas,
        [
            ("cpf_primeiros:3".to_owned(), 0),
            ("cpf_ultimos:5".to_owned(), 1)
        ]
    );
    let contadores = |id: i64| -> (i64, i64, bool, bool) {
        c.query_row(
            "SELECT sucessos, falhas, ultimo_sucesso_em IS NOT NULL, ultima_falha_em IS NOT NULL
               FROM regra_senha WHERE id = ?1",
            [id],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
        )
        .expect("regra")
    };
    assert_eq!(contadores(r1), (0, 1, false, true));
    assert_eq!(contadores(r2), (1, 0, true, false));
    let (metodo, conteudo): (String, String) = c
        .query_row(
            "SELECT metodo, conteudo FROM texto WHERE content_hash = ?1",
            [&h],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .expect("texto");
    assert_eq!(
        (metodo.as_str(), conteudo.as_str()),
        ("pdftotext", "texto fictício")
    );
    let impressao: String = c
        .query_row(
            "SELECT canonical_fingerprint FROM achado WHERE content_hash = ?1",
            [&h],
            |l| l.get(0),
        )
        .expect("impressao");
    assert_eq!(impressao, "ab".repeat(32));
    // Aberto: sai dos alvos.
    assert!(
        protegidos::alvos(c, 6)
            .expect("alvos")
            .iter()
            .all(|a| a.content_hash != h)
    );
}

#[test]
fn n_falhas_viram_pendencia_e_responder_libera_nova_rodada() {
    let pasta = tempfile::tempdir().expect("pasta");
    let mut inv = inventario_migrado(pasta.path());
    let h = protegido(inv.conexao(), 1, Some("Banco Alfa"), Some("fatura"));
    let alvo_1 = alvo(inv.conexao(), &h);

    let registro = protegidos::registrar(
        inv.conexao(),
        &alvo_1,
        &[errada("cpf_completo", None), errada("cpf_pontuado", None)],
        &Conclusao::SoTentativas,
        3,
    )
    .expect("registrar");
    assert_eq!((registro.falhas, registro.pendencia), (2, None));
    let registro = protegidos::registrar(
        inv.conexao(),
        &alvo_1,
        &[errada("cpf_primeiros:5", None)],
        &Conclusao::SoTentativas,
        3,
    )
    .expect("registrar");
    assert_eq!(registro.falhas, 3);
    let id = registro.pendencia.expect("pendência");
    let (etapa, tipo, referencia, pergunta): (String, String, String, String) = inv
        .conexao()
        .query_row(
            "SELECT etapa, tipo, referencia, pergunta FROM pendencia WHERE id = ?1",
            [id],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
        )
        .expect("pendência");
    assert_eq!(etapa, "desktop");
    assert_eq!(tipo, TIPO_SEM_REGRA);
    assert_eq!(referencia, format!("achado:{h}"));
    assert!(
        pergunta.starts_with("PDF protegido sem regra (fatura de Banco Alfa)"),
        "{pergunta}"
    );
    // As tentativas param: não é mais alvo, nem com limite maior (a pendência está aberta).
    assert!(
        protegidos::alvos(inv.conexao(), 100)
            .expect("alvos")
            .is_empty()
    );

    // Registrar de novo não duplica a pendência.
    let de_novo = protegidos::registrar(
        inv.conexao(),
        &alvo_1,
        &[],
        &Conclusao::SemRegra {
            pergunta: "outra".into(),
        },
        3,
    )
    .expect("registrar");
    assert_eq!(de_novo.pendencia, Some(id));

    // O usuário responde (uma decisão): as falhas zeram e o achado volta a ser alvo.
    std::thread::sleep(std::time::Duration::from_millis(5));
    inv.executar(&PedidoDeDecisao {
        acao: Acao::Responder {
            resposta: "Guardei a senha no chaveiro".into(),
        },
        alvos: vec![Ref::nova(Tipo::Pendencia, id.to_string())],
        motivo: None,
    })
    .expect("responder");
    let de_volta = alvo(inv.conexao(), &h);
    assert_eq!(de_volta.falhas, 0);
}

#[test]
fn sem_texto_grava_o_metodo_do_bun_e_sai_dos_alvos() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inv = inventario_migrado(pasta.path());
    let c = inv.conexao();
    let h = protegido(c, 1, None, None);
    let alvo = alvo(c, &h);
    protegidos::registrar(c, &alvo, &[], &Conclusao::SemTexto, 6).expect("registrar");
    let metodo: String = c
        .query_row(
            "SELECT metodo FROM texto WHERE content_hash = ?1",
            [&h],
            |l| l.get(0),
        )
        .expect("texto");
    assert_eq!(metodo, "pdf-sem-texto");
    assert!(protegidos::alvos(c, 6).expect("alvos").is_empty());
}
