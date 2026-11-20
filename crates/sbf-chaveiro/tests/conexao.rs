//! Os segredos das conexões de e-mail têm namespace próprio (`conexao:`), separado do dos
//! mensageiros, das senhas de PDF e das chaves de IA.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ParametrosDeDerivacao, Segredo};
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro};
use std::sync::Arc;

#[test]
fn o_segredo_de_conexao_nao_se_mistura_com_mensageiro_senha_nem_ia() {
    let pasta = tempfile::tempdir().unwrap();
    let c = Chaveiro::abrir_com_chaveiro(
        pasta.path(),
        Arc::new(ChaveiroEmMemoria::novo()),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .unwrap();
    assert!(!c.tem_segredo_de_conexao("email/gmail/refresh_token"));
    c.guardar_segredo_de_conexao("email/gmail/refresh_token", Segredo::novo("token-ficticio"))
        .unwrap();
    assert!(c.tem_segredo_de_conexao("email/gmail/refresh_token"));
    assert_eq!(
        c.segredo_de_conexao("email/gmail/refresh_token")
            .unwrap()
            .expor(),
        "token-ficticio"
    );
    // Outros namespaces não enxergam, e a lista de senhas de PDF continua vazia.
    assert!(!c.tem_segredo_de_mensageiro("email/gmail/refresh_token"));
    assert!(!c.tem_chave_de_ia("email/gmail/refresh_token"));
    assert!(c.rotulos_de_senha().is_empty());
    c.apagar_segredo_de_conexao("email/gmail/refresh_token")
        .unwrap();
    c.apagar_segredo_de_conexao("email/gmail/refresh_token")
        .unwrap();
    assert!(!c.tem_segredo_de_conexao("email/gmail/refresh_token"));
    assert!(matches!(
        c.guardar_segredo_de_conexao("", Segredo::novo("x")),
        Err(ErroDoChaveiro::RotuloInvalido)
    ));
}
