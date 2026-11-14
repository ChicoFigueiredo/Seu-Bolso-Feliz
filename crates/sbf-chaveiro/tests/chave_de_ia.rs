//! A chave de um provedor de IA no chaveiro (spec 09): `ia:<provedor>`, fora da lista de
//! senhas de PDF.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_segredos::testes::ChaveiroEmMemoria;
use bancada_segredos::{ParametrosDeDerivacao, Segredo};
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro};
use std::sync::Arc;

#[test]
fn a_chave_de_ia_fica_sob_ia_provedor_e_nao_aparece_como_senha() {
    let pasta = tempfile::tempdir().unwrap();
    let chaveiro = Chaveiro::abrir_com_chaveiro(
        pasta.path(),
        Arc::new(ChaveiroEmMemoria::novo()),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .unwrap();
    assert!(!chaveiro.tem_chave_de_ia("openrouter"));
    chaveiro
        .guardar_chave_de_ia("openrouter", Segredo::novo("chave-ficticia-ia"))
        .unwrap();
    assert!(chaveiro.tem_chave_de_ia("openrouter"));
    assert_eq!(
        chaveiro.chave_de_ia("openrouter").unwrap().expor(),
        "chave-ficticia-ia"
    );
    assert!(chaveiro.rotulos_de_senha().is_empty());
    assert!(matches!(
        chaveiro.guardar_chave_de_ia("", Segredo::novo("x")),
        Err(ErroDoChaveiro::RotuloInvalido)
    ));
    assert!(!chaveiro.tem_chave_de_ia(""));
}
