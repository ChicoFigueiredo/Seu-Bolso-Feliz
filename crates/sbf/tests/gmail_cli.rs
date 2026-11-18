//! `sbf gmail coletar` sem rede e sem credenciais: só o que dá para conferir sem o Gmail.
//! As credenciais nunca vão em argumento; o teste as remove do ambiente.

use std::process::Command;

fn sbf() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_sbf"));
    for v in [
        "GOOGLE_MAIL_CLIENT_ID",
        "GOOGLE_MAIL_CLIENT_SECRET",
        "GMAIL_REFRESH_TOKEN",
    ] {
        c.env_remove(v);
    }
    c
}

#[test]
fn sem_credenciais_diz_quais_variaveis_faltam_e_sai_com_2() {
    let cofre = tempfile::tempdir().expect("cofre");
    let saida = sbf()
        .args(["gmail", "coletar", "--inventario"])
        .arg(cofre.path())
        .output()
        .expect("rodar o sbf");
    assert_eq!(saida.status.code(), Some(2));
    let erro = String::from_utf8_lossy(&saida.stderr);
    for nome in [
        "GOOGLE_MAIL_CLIENT_ID",
        "GOOGLE_MAIL_CLIENT_SECRET",
        "GMAIL_REFRESH_TOKEN",
    ] {
        assert!(erro.contains(nome), "{erro}");
    }
    // Nada foi criado no cofre.
    assert_eq!(std::fs::read_dir(cofre.path()).expect("ler").count(), 0);
}

#[test]
fn argumentos_errados_saem_com_2_e_mostram_o_uso() {
    for args in [
        vec!["gmail"],
        vec!["gmail", "coletar"],
        vec![
            "gmail",
            "coletar",
            "--inventario",
            "/x",
            "--limite",
            "muitos",
        ],
        vec!["gmail", "coletar", "--senha", "x"],
    ] {
        let saida = sbf().args(&args).output().expect("rodar o sbf");
        assert_eq!(saida.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&saida.stderr).contains("sbf gmail coletar"),
            "{args:?}"
        );
    }
}
