#![allow(dead_code)]
use sbf_regras::script::{Limites, PedidoDeScript};
use serde_json::{Value, json};

pub fn pedido_com(
    fonte: &str,
    entrada: Value,
    parametros: Value,
    limites: Limites,
) -> PedidoDeScript {
    PedidoDeScript {
        fonte: fonte.into(),
        checksum: "teste".into(),
        entrada,
        parametros,
        limites,
    }
}

pub fn pedido(fonte: &str) -> PedidoDeScript {
    pedido_com(
        fonte,
        json!({"tipo_documental": "x", "campos": {"a": 1}}),
        json!({}),
        Limites::default(),
    )
}

/// Embrulha um corpo de teste numa `transformar` que devolve `{corresponde=true}`.
pub fn corpo(codigo: &str) -> String {
    format!(
        "function transformar(entrada, parametros, ajuda)\ndo\n{codigo}\nend\nreturn {{corresponde = true}}\nend"
    )
}
