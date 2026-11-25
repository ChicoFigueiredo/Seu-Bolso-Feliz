//! `sbf --versao` e `sbf --version`, rodando o binário de verdade.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

fn saida(flag: &str) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_sbf"))
        .arg(flag)
        .output()
        .expect("rodar o sbf");
    assert!(o.status.success());
    String::from_utf8(o.stdout).unwrap()
}

#[test]
fn versao_traz_commit_da_bancada_e_data() {
    for flag in ["--versao", "--version"] {
        let s = saida(flag);
        let linha = s.trim_end();
        assert_eq!(s.lines().count(), 1, "uma linha só: {s:?}");
        assert!(linha.starts_with("sbf 0.1.0 · rev "), "{linha}");
        assert!(linha.contains(" · bancada "), "{linha}");
        let data = linha.rsplit("compilado em ").next().unwrap();
        assert_eq!(data.len(), 10, "{linha}");
        assert_eq!(data.matches('-').count(), 2, "{linha}");
    }
}
