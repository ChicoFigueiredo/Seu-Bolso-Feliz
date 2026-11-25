//! O que um roteiro baixa só entra no acervo se a extensão está na allowlist.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use comum::inventario;
use sbf_roteiros::executor::{Destino, Recebido};
use sbf_roteiros::inventario::{EXTENSOES_ACEITAS, NoInventario};

#[test]
fn so_as_extensoes_aceitas_entram_no_acervo() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let mut dest = NoInventario::novo(&conn, &cofre, "r", 1);
    for (i, ext) in EXTENSOES_ACEITAS.iter().enumerate() {
        let arquivo = d.path().join(format!("aceito.{ext}"));
        std::fs::write(&arquivo, format!("conteudo {i} {ext}")).unwrap();
        let r = dest.receber(&arquivo, &format!("arquivo.{ext}"));
        assert_eq!(r.unwrap(), Recebido::Novo, "{ext}");
    }
    // a caixa da extensão não importa
    let maiuscula = d.path().join("EXTRATO.PDF");
    std::fs::write(&maiuscula, "outro pdf").unwrap();
    assert_eq!(
        dest.receber(&maiuscula, "EXTRATO.PDF").unwrap(),
        Recebido::Novo
    );
    let antes: i64 = conn
        .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
        .unwrap();
    assert_eq!(antes, EXTENSOES_ACEITAS.len() as i64 + 1);
}

#[test]
fn outra_extensao_e_recusada_descartada_e_nao_entra_no_acervo() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let mut dest = NoInventario::novo(&conn, &cofre, "r", 1);
    for nome in [
        "malware.exe",
        "pagina.html",
        "script.js",
        "atalho.lnk",
        "sem-extensao",
        "duplo.pdf.exe",
        "muito.longa-extensao-estranha",
    ] {
        let arquivo = d.path().join(nome);
        std::fs::write(&arquivo, format!("conteudo de {nome}")).unwrap();
        let e = dest.receber(&arquivo, nome).unwrap_err().to_string();
        assert!(e.contains("download recusado"), "{nome}: {e}");
        assert!(e.contains("não é aceita"), "{nome}: {e}");
        assert!(!arquivo.exists(), "{nome}: o arquivo é descartado");
    }
    for tabela in ["achado", "origem"] {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {tabela}"), [], |l| l.get(0))
            .unwrap();
        assert_eq!(n, 0, "{tabela}: nada entrou");
    }
    // nem no acervo do cofre
    let copias = walk(&cofre);
    assert!(
        copias
            .iter()
            .all(|c| !c.ends_with(".exe") && !c.ends_with(".html")),
        "{copias:?}"
    );
}

fn walk(p: &std::path::Path) -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let c = e.path();
            if c.is_dir() {
                v.extend(walk(&c));
            } else {
                v.push(c.to_string_lossy().into_owned());
            }
        }
    }
    v
}
