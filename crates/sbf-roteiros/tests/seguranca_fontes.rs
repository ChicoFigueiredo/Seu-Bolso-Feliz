//! Guarda de código para RO-01: o valor de um `Segredo` só é exposto onde precisa.
//!
//! `Segredo::expor` pode aparecer em: `executor.rs` (troca por `***` do que sai) e
//! `gravacao.rs` (limpa a página antes de ir à IA). Nenhum outro arquivo de produção do crate
//! chama. A entrega ao CDP (`chrome.rs`) e o navegador falso moram na bancada, que tem a sua
//! própria guarda (`bancada-navegador/tests/seguranca_fontes.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

const PERMITIDOS: &[&str] = &["executor.rs", "gravacao.rs"];

#[test]
fn so_quem_precisa_expoe_o_segredo() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for e in std::fs::read_dir(src).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let nome = p.file_name().unwrap().to_str().unwrap();
        let texto = std::fs::read_to_string(&p).unwrap();
        if texto.contains(".expor()") {
            assert!(PERMITIDOS.contains(&nome), "{nome} chama Segredo::expor");
        }
    }
}

#[test]
fn nenhum_log_nem_print_no_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for e in std::fs::read_dir(src).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let texto = std::fs::read_to_string(&p).unwrap();
        for proibido in ["println!", "eprintln!", "dbg!(", "log::", "tracing::"] {
            // os testes unitários do próprio arquivo podem imprimir; a produção não
            let producao = texto.split("#[cfg(test)]").next().unwrap();
            assert!(!producao.contains(proibido), "{}: {proibido}", p.display());
        }
    }
}

#[test]
fn o_passo_de_senha_so_tem_o_rotulo() {
    // O JSON do passo de senha não tem campo de valor: nem a IA nem o usuário a põem no roteiro.
    let j = serde_json::to_value(sbf_roteiros::Passo::PreencherSenha {
        seletor: "#s".into(),
        rotulo: "r".into(),
        host: None,
    })
    .unwrap();
    let chaves: Vec<&str> = j.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        chaves,
        vec!["rotulo", "seletor", "tipo"],
        "host só aparece quando aprovado"
    );
    // e um JSON com `senha`/`valor` sobrando é descartado, não guardado
    let p: sbf_roteiros::Passo = serde_json::from_str(
        r##"{"tipo":"preencher_senha","seletor":"#s","rotulo":"r","senha":"vazou","valor":"vazou"}"##,
    )
    .unwrap();
    assert!(!serde_json::to_string(&p).unwrap().contains("vazou"));
}
