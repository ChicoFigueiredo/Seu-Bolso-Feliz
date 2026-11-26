//! O CPF digitado à mão na janela: confere os dígitos verificadores e grava só os 11 dígitos.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_segredos::ParametrosDeDerivacao;
use bancada_segredos::testes::ChaveiroEmMemoria;
use sbf_chaveiro::{Chaveiro, ErroDoChaveiro};
use std::sync::Arc;

fn chaveiro() -> (tempfile::TempDir, Chaveiro) {
    let pasta = tempfile::tempdir().unwrap();
    let c = Chaveiro::abrir_com_chaveiro(
        pasta.path(),
        Arc::new(ChaveiroEmMemoria::novo()),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .unwrap();
    (pasta, c)
}

#[test]
fn cpf_valido_com_mascara_e_guardado_so_com_os_digitos() {
    let (_pasta, c) = chaveiro();
    assert!(!c.tem_cpf());
    c.guardar_cpf_digitado("529.982.247-25").unwrap();
    assert!(c.tem_cpf());
    assert_eq!(c.cpf().unwrap().expor(), "52998224725");
}

#[test]
fn cpf_com_digito_errado_ou_repetido_e_recusado_e_nada_e_gravado() {
    let (_pasta, c) = chaveiro();
    for ruim in ["52998224724", "11111111111", "5299822472", "", "abc"] {
        assert!(
            matches!(
                c.guardar_cpf_digitado(ruim),
                Err(ErroDoChaveiro::CpfInvalido)
            ),
            "{ruim}"
        );
    }
    assert!(!c.tem_cpf());
}

#[test]
fn o_erro_do_cpf_nao_repete_o_numero_digitado() {
    let (_pasta, c) = chaveiro();
    let e = c.guardar_cpf_digitado("52998224724").unwrap_err();
    assert!(!e.to_string().contains("5299822"), "{e}");
}
