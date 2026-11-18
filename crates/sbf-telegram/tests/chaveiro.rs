//! A sessão e as credenciais no chaveiro (com o keyring de mentira): ida e volta, e nada em
//! arquivo solto.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use bancada_segredos::ParametrosDeDerivacao;
use bancada_segredos::testes::ChaveiroEmMemoria;
use sbf_chaveiro::Chaveiro;
use sbf_telegram::Erro;
use sbf_telegram::credenciais::Credenciais;
use sbf_telegram::sessao::{self, SessaoMtproto};

fn chaveiro(pasta: &std::path::Path) -> Chaveiro {
    Chaveiro::abrir_com_chaveiro(
        pasta,
        Arc::new(ChaveiroEmMemoria::novo()),
        ParametrosDeDerivacao::rapidos_para_testes(),
    )
    .unwrap()
}

fn sessao_com_chave() -> SessaoMtproto {
    use grammers_session::Session as _;
    let s = SessaoMtproto::default();
    let mut o = s.dc_option(2).unwrap().unwrap();
    o.auth_key = Some([0xAB; 256]);
    futures_lite_block(s.set_dc_option(&o)).unwrap();
    s
}

/// Roda o futuro da sessão (que não espera nada) sem trazer um runtime para o teste.
fn futures_lite_block<T>(f: impl std::future::Future<Output = T>) -> T {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(f)
}

#[test]
fn a_sessao_vai_e_volta_pelo_chaveiro_e_nao_vira_arquivo_solto() {
    let pasta = tempfile::tempdir().unwrap();
    let c = chaveiro(pasta.path());
    assert!(!sessao::existe_no_chaveiro(&c));
    assert!(sessao::ler(&c).unwrap().is_none());

    sessao::guardar(&c, &sessao_com_chave()).unwrap();
    assert!(sessao::existe_no_chaveiro(&c));
    let volta = sessao::ler(&c).unwrap().unwrap();
    assert!(volta.tem_chave());
    // Não aparece como senha de PDF nem como chave de IA.
    assert!(c.rotulos_de_senha().is_empty());

    // Nenhum arquivo da pasta do chaveiro guarda a chave em claro (hex nem binário).
    let hex = "ab".repeat(16);
    for e in std::fs::read_dir(pasta.path()).unwrap().flatten() {
        if e.path().is_file() {
            let bytes = std::fs::read(e.path()).unwrap();
            let texto = String::from_utf8_lossy(&bytes);
            assert!(!texto.contains(&hex), "{:?} guarda a chave", e.path());
            assert!(!bytes.windows(16).any(|w| w == [0xAB; 16]));
        }
    }

    sessao::apagar(&c).unwrap();
    assert!(!sessao::existe_no_chaveiro(&c));
    // Apagar de novo não é erro.
    sessao::apagar(&c).unwrap();
}

#[test]
fn valor_estranho_no_chaveiro_nao_vira_sessao() {
    let pasta = tempfile::tempdir().unwrap();
    let c = chaveiro(pasta.path());
    c.guardar_segredo_de_mensageiro(
        sessao::NOME_NO_CHAVEIRO,
        sbf_chaveiro::Segredo::novo("isto não é uma sessão"),
    )
    .unwrap();
    assert!(matches!(sessao::ler(&c), Err(Erro::Formato(_))));
}

#[test]
fn credenciais_ficam_no_chaveiro_para_o_worker_sem_env() {
    let pasta = tempfile::tempdir().unwrap();
    let c = chaveiro(pasta.path());
    assert!(matches!(
        Credenciais::resolver(&|_| None, &[], Some(&c)),
        Err(Erro::SemCredenciais)
    ));
    let origem = Credenciais::resolver(
        &|n| match n {
            "TELEGRAM_API_ID" => Some("4242".to_owned()),
            "TELEGRAM_API_HASH" => Some("hash-ficticio".to_owned()),
            _ => None,
        },
        &[],
        None,
    )
    .unwrap();
    origem.guardar(&c).unwrap();
    let do_chaveiro = Credenciais::resolver(&|_| None, &[], Some(&c)).unwrap();
    assert_eq!(do_chaveiro.api_id, 4242);
    assert_eq!(do_chaveiro.api_hash.as_str(), "hash-ficticio");
}
