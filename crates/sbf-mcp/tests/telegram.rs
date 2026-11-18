//! As ferramentas do Telegram contra uma ponte falsa (nunca a conta real).
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
#[allow(unused_imports)]
mod ajuda;

use std::sync::{Arc, Mutex};

use ajuda::banco_limpo;
use sbf_mcp::Nucleo;
use sbf_mcp::servidor::Servidor;
use sbf_mcp::telegram::{
    FERRAMENTAS, PedidoDeColeta, PedidoDeConversas, PedidoDeSeguir, PonteDoTelegram, coletar,
    conversas, seguir,
};
use serde_json::{Value, json};

#[derive(Default)]
struct Falsa {
    chamadas: Mutex<Vec<String>>,
}

impl PonteDoTelegram for Falsa {
    fn conversas(&self, atualizar: bool) -> Result<Value, String> {
        self.chamadas
            .lock()
            .unwrap()
            .push(format!("conversas {atualizar}"));
        Ok(json!({"conversas": [{"id": "-1", "nome": "Casa", "seguida": true}]}))
    }
    fn seguir(&self, conversa: &str, seguir: bool) -> Result<Value, String> {
        self.chamadas
            .lock()
            .unwrap()
            .push(format!("seguir {conversa} {seguir}"));
        if conversa == "nenhuma" {
            return Err("nenhuma conversa chamada assim".into());
        }
        Ok(json!({"seguindo": seguir}))
    }
    fn coletar(&self, conversa: Option<&str>, desde: Option<&str>) -> Result<Value, String> {
        self.chamadas
            .lock()
            .unwrap()
            .push(format!("coletar {conversa:?} {desde:?}"));
        Ok(json!({"novos": 2}))
    }
}

#[test]
fn as_ferramentas_so_aparecem_com_a_ponte_ligada() {
    let banco = banco_limpo();
    let nucleo = Nucleo::novo(&banco.caminho, None).unwrap();
    let sem = Servidor::novo(nucleo.clone()).nomes_das_ferramentas();
    for f in FERRAMENTAS {
        assert!(!sem.iter().any(|n| n == f), "não devia anunciar {f}");
    }
    assert!(nucleo.telegram().unwrap_err().contains("não está ligado"));
    let com =
        Servidor::novo(nucleo.com_telegram(Arc::new(Falsa::default()))).nomes_das_ferramentas();
    for f in FERRAMENTAS {
        assert!(com.iter().any(|n| n == f), "falta {f}");
    }
}

#[test]
fn cada_ferramenta_delega_e_valida_a_entrada() {
    let banco = banco_limpo();
    let falsa = Arc::new(Falsa::default());
    let nucleo = Nucleo::novo(&banco.caminho, None)
        .unwrap()
        .com_telegram(falsa.clone());
    let ponte = nucleo.telegram().unwrap();

    let r = conversas(
        ponte,
        &PedidoDeConversas {
            atualizar: Some(true),
        },
    )
    .unwrap();
    assert_eq!(r["conversas"][0]["seguida"], true);

    seguir(
        ponte,
        &PedidoDeSeguir {
            conversa: " Casa ".into(),
            deixar: None,
        },
    )
    .unwrap();
    seguir(
        ponte,
        &PedidoDeSeguir {
            conversa: "Casa".into(),
            deixar: Some(true),
        },
    )
    .unwrap();
    assert!(
        seguir(
            ponte,
            &PedidoDeSeguir {
                conversa: "  ".into(),
                deixar: None
            }
        )
        .is_err()
    );
    assert!(
        seguir(
            ponte,
            &PedidoDeSeguir {
                conversa: "nenhuma".into(),
                deixar: None
            }
        )
        .is_err()
    );

    assert!(
        coletar(
            ponte,
            &PedidoDeColeta {
                conversa: None,
                desde: Some("ontem".into())
            }
        )
        .is_err()
    );
    let r = coletar(
        ponte,
        &PedidoDeColeta {
            conversa: Some("Casa".into()),
            desde: Some("2026-11-01".into()),
        },
    )
    .unwrap();
    assert_eq!(r["coleta"]["novos"], 2);

    let chamadas = falsa.chamadas.lock().unwrap().clone();
    assert_eq!(
        chamadas,
        [
            "conversas true",
            "seguir Casa true",
            "seguir Casa false",
            "seguir nenhuma true",
            "coletar Some(\"Casa\") Some(\"2026-11-01\")",
        ]
    );
}
