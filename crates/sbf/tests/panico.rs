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
