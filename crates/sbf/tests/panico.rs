//! O gancho de pânico (A16): a mensagem mostra só o local, nunca o payload.

#[test]
fn o_gancho_de_panico_nao_mostra_o_payload() {
    let saida = std::process::Command::new(env!("CARGO_BIN_EXE_sbf"))
        .env("SBF_TESTE_DO_GANCHO_DE_PANICO", "1")
        .output()
        .expect("rodar o sbf");
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(!saida.status.success());
    assert!(erro.contains("sbf: falha interna em"), "{erro}");
    assert!(erro.contains("main.rs:"), "{erro}");
    assert!(!erro.contains("payload-que-nao-pode-aparecer"), "{erro}");
}

#[test]
fn reprojetar_sem_banco_sai_com_erro_claro() {
    let pasta = std::env::temp_dir().join("sbf-teste-reprojetar-sem-banco");
    let _ = std::fs::create_dir_all(&pasta);
    let saida = std::process::Command::new(env!("CARGO_BIN_EXE_sbf"))
        .args(["reprojetar", "--inventario"])
        .arg(&pasta)
        .output()
        .expect("rodar o sbf");
    assert_eq!(saida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&saida.stderr).starts_with("sbf: "));
    let sem_pasta = std::process::Command::new(env!("CARGO_BIN_EXE_sbf"))
        .args(["reprojetar"])
        .output()
        .expect("rodar o sbf");
    assert_eq!(sem_pasta.status.code(), Some(2));
}
