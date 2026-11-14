//! A senha de um site (roteiros, ADR-016) fica sob `site:<rotulo>`, fora da lista de senhas de PDF.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ParametrosDeDerivacao, Segredo};
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro};
use std::sync::Arc;

#[test]
fn a_senha_de_site_fica_separada_das_senhas_de_pdf() {
    let pasta = tempfile::tempdir().unwrap();
    let chaveiro = Chaveiro::abrir_com_chaveiro(
        pasta.path(),
        Arc::new(ChaveiroEmMemoria::novo()),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .unwrap();
    chaveiro
        .guardar_senha_de_site("banco-ficticio", Segredo::novo("senha-ficticia-site"))
        .unwrap();
    chaveiro
        .guardar_senha("pdf", Segredo::novo("outra-ficticia"))
        .unwrap();
    assert_eq!(
        chaveiro.rotulos_de_site(),
        vec!["banco-ficticio".to_owned()]
    );
    assert_eq!(chaveiro.rotulos_de_senha(), vec!["pdf".to_owned()]);
    assert_eq!(
        chaveiro.senha_de_site("banco-ficticio").unwrap().expor(),
        "senha-ficticia-site"
    );
    chaveiro.apagar_senha_de_site("banco-ficticio").unwrap();
    chaveiro.apagar_senha_de_site("banco-ficticio").unwrap();
    assert!(chaveiro.rotulos_de_site().is_empty());
    assert!(matches!(
        chaveiro.guardar_senha_de_site("", Segredo::novo("x")),
        Err(ErroDoChaveiro::RotuloInvalido)
    ));
}
