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
