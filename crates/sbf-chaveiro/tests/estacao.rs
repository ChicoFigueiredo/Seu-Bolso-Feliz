//! A credencial de estação no chaveiro (spec 07): chave `estacao:<servidor>`, nunca em
//! arquivo em claro.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ParametrosDeDerivacao, Segredo};
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro, SERVICO};
use std::sync::Arc;

#[test]
fn a_credencial_fica_no_chaveiro_sob_estacao_servidor() {
    let pasta = tempfile::tempdir().unwrap();
    let sistema = Arc::new(ChaveiroEmMemoria::novo());
    let chaveiro = Chaveiro::abrir_com_chaveiro(
        pasta.path(),
        sistema.clone(),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .unwrap();
    assert!(!chaveiro.tem_credencial_de_estacao("sbf.exemplo.com.br"));
    chaveiro
        .guardar_credencial_de_estacao("sbf.exemplo.com.br", Segredo::novo("tok-ficticio-123"))
        .unwrap();
    assert!(chaveiro.tem_credencial_de_estacao("sbf.exemplo.com.br"));
    assert_eq!(
        chaveiro
            .credencial_de_estacao("sbf.exemplo.com.br")
            .unwrap()
            .expor(),
        "tok-ficticio-123"
    );
    assert!(
        sistema
            .entradas()
            .iter()
            .any(|(conta, _)| conta.contains("estacao:sbf.exemplo.com.br"))
            || sistema.entradas().len() == 1
    );
    let _ = SERVICO;
    // Nenhum arquivo da pasta tem o valor.
    for entrada in std::fs::read_dir(pasta.path()).unwrap() {
        let bytes = std::fs::read(entrada.unwrap().path()).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("tok-ficticio-123"));
    }
    chaveiro
        .apagar_credencial_de_estacao("sbf.exemplo.com.br")
        .unwrap();
    assert!(!chaveiro.tem_credencial_de_estacao("sbf.exemplo.com.br"));
    // Apagar de novo não é erro.
    chaveiro
        .apagar_credencial_de_estacao("sbf.exemplo.com.br")
        .unwrap();
    assert!(matches!(
        chaveiro.guardar_credencial_de_estacao("", Segredo::novo("x")),
        Err(ErroDoChaveiro::RotuloInvalido)
    ));
}
