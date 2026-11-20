//! `texto`: gravar o texto e a impressão canônica de um achado, sem sobrescrever o que já há.

mod nucleo_comum;

use nucleo_comum::{hash, inventario_migrado};
use sbf_inventario::texto;

#[test]
fn grava_o_texto_e_a_impressao_e_a_segunda_gravacao_nao_sobrescreve() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inventario = inventario_migrado(pasta.path());
    let c = inventario.conexao();
    let h = hash(0x71);
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES (?1, 10, 'A')",
        [&h],
    )
    .expect("achado");
    assert!(!texto::tem_texto(c, &h).expect("tem"));
    assert_eq!(texto::ler_texto(c, &h).expect("ler"), None);

    texto::registrar_texto(c, &h, "pdftotext", "fatura ficticia", Some("ab12")).expect("gravar");
    texto::registrar_texto(c, &h, "ocr-por", "outro texto", Some("cd34")).expect("regravar");

    assert!(texto::tem_texto(c, &h).expect("tem"));
    let gravado = texto::ler_texto(c, &h).expect("ler").expect("linha");
    assert_eq!(gravado.metodo, "pdftotext");
    assert_eq!(gravado.conteudo, "fatura ficticia");
    let impressao: Option<String> = c
        .query_row(
            "SELECT canonical_fingerprint FROM achado WHERE content_hash = ?1",
            [&h],
            |l| l.get(0),
        )
        .expect("impressao");
    // A impressão é a do texto que ficou.
    assert_eq!(impressao.as_deref(), Some("ab12"));
}

#[test]
fn sem_impressao_nao_mexe_no_fingerprint() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inventario = inventario_migrado(pasta.path());
    let c = inventario.conexao();
    let h = hash(0x72);
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada, canonical_fingerprint)
         VALUES (?1, 10, 'A', 'antigo')",
        [&h],
    )
    .expect("achado");
    texto::registrar_texto(c, &h, "pdf-sem-texto", "", None).expect("gravar");
    let impressao: Option<String> = c
        .query_row(
            "SELECT canonical_fingerprint FROM achado WHERE content_hash = ?1",
            [&h],
            |l| l.get(0),
        )
        .expect("impressao");
    assert_eq!(impressao.as_deref(), Some("antigo"));
}

#[test]
fn momento_do_achado_e_a_menor_data_de_origem_depois_o_primeiro_visto() {
    let pasta = tempfile::tempdir().expect("pasta");
    let inventario = inventario_migrado(pasta.path());
    let c = inventario.conexao();
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:m', 'disco', 'Disco')",
        [],
    )
    .expect("fonte");
    c.execute(
        "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
         VALUES (900, 'disco:m', 'E1', 'raiz', 't')",
        [],
    )
    .expect("varredura");
    let (com, vazio, sem) = (hash(0x81), hash(0x82), hash(0x83));
    for h in [&com, &vazio, &sem] {
        c.execute(
            "INSERT INTO achado (content_hash, file_size_bytes, camada, primeiro_visto_em)
             VALUES (?1, 1, 'A', '2026-11-20T10:00:00.000Z')",
            [h],
        )
        .expect("achado");
    }
    let origem = |chave: &str, h: &str, mtime: Option<&str>| {
        c.execute(
            "INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
               local_mtime, filename) VALUES (?1, 'disco:m', 900, 'local_file', ?2, ?3, 'x')",
            rusqlite::params![h, chave, mtime],
        )
        .expect("origem");
    };
    origem("a", &com, Some("2026-11-18T12:00:00.000Z"));
    origem("b", &com, Some("2026-10-02T08:00:00.000Z"));
    origem("c", &com, None);
    origem("d", &vazio, Some(""));
    origem("e", &vazio, None);

    let m = |h: &str| texto::momento_do_achado(c, h).expect("momento");
    assert_eq!(m(&com).as_deref(), Some("2026-10-02T08:00:00.000Z"));
    // Origens sem data (ou com data vazia): vale quando o sbf indexou.
    assert_eq!(m(&vazio).as_deref(), Some("2026-11-20T10:00:00.000Z"));
    assert_eq!(m(&sem).as_deref(), Some("2026-11-20T10:00:00.000Z"));
    assert_eq!(m(&hash(0x84)), None);
}
