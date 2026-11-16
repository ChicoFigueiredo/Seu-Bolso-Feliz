//! A transação normalizada da Pluggy em `draft_records` (`build-draft.ts` e `map-to-draft.ts`).
//!
//! O mesmo destino do pipeline de documentos, sem OCR: `source_document_id` nulo,
//! `parser_type: "pluggy_provider"` e confiança fixa em 1.00 (a Pluggy é fonte estruturada e
//! conclusiva). A idempotência vem de `external_ref = pluggy:txn_<id>`, único por usuário.

use serde_json::{Value, json};
use uuid::Uuid;

use super::cliente::TransacaoNormalizada;
use crate::erro::ErroDeColeta;

/// `DRAFT_SCHEMA_VERSION` de `@sbf/contracts`.
pub const VERSAO_DO_ESQUEMA_DO_RASCUNHO: i16 = 1;

/// A `external_ref` que torna o sync idempotente.
pub fn referencia_externa(id_externo: &str) -> String {
    format!("pluggy:txn_{id_externo}")
}

/// Resolvidos pelo chamador: conta, fornecedor e categoria.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vinculos {
    pub produto: Option<Uuid>,
    pub fornecedor: Option<Uuid>,
    pub categoria: Option<Uuid>,
}

fn e_data_iso(texto: &str) -> bool {
    let b = texto.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

/// O `draft_data` (um `TransactionDraftV1`). Falha se a data não for `AAAA-MM-DD`: o Zod do
/// Bun também reprova, e um draft com data torta não pode entrar.
pub fn montar_draft_data(t: &TransacaoNormalizada, v: Vinculos) -> Result<Value, ErroDeColeta> {
    if !e_data_iso(&t.data) {
        return Err(ErroDeColeta::Entrada(format!(
            "a transação {} veio com data fora de AAAA-MM-DD",
            t.id_externo
        )));
    }
    let descricao = if t.descricao.is_empty() {
        t.descricao_bruta.clone().filter(|d| !d.is_empty())
    } else {
        Some(t.descricao.clone())
    };
    let tipo = match t.direcao {
        super::cliente::Direcao::Entrada => "income",
        super::cliente::Direcao::Saida => "expense",
    };
    Ok(json!({
        "schema_version": VERSAO_DO_ESQUEMA_DO_RASCUNHO,
        "provenance": {
            "source_document_id": null,
            "extraction_result_id": null,
            "parsed_version_id": null,
            "parser_type": "pluggy_provider",
            "field_sources": {
                "amount_cents": "pluggy_provider",
                "direction": "pluggy_provider",
                "transaction_type": "pluggy_provider",
                "event_date": "pluggy_provider",
                "supplier_name_raw": "pluggy_provider"
            }
        },
        "obligation_id": null,
        "draft_type": "transaction",
        "direction": t.direcao.como_texto(),
        "transaction_type": tipo,
        "amount_cents": t.centavos,
        "currency": t.moeda,
        "event_date": t.data,
        "due_date": null,
        "competence_date": null,
        "description": descricao,
        "supplier_name_raw": t.nome_do_comerciante,
        "supplier_id": v.fornecedor,
        "category_suggestion": t.categoria,
        "category_id": v.categoria,
        "tags": [],
        "priority": null,
        "document_number": null,
        "contract_identifier": null,
        "barcode_digitable_line": null,
        "financial_product_id": v.produto,
        "notes": null
    }))
}

#[cfg(test)]
mod testes {
    use super::super::cliente::Direcao;
    use super::*;

    fn transacao() -> TransacaoNormalizada {
        TransacaoNormalizada {
            id_externo: "t-1".into(),
            direcao: Direcao::Saida,
            centavos: 12345,
            moeda: "BRL".into(),
            data: "2026-03-10".into(),
            descricao: "Padaria Exemplo".into(),
            descricao_bruta: Some("PADARIA EXEMPLO 001".into()),
            categoria: Some("Eating out".into()),
            nome_do_comerciante: Some("Padaria Exemplo LTDA".into()),
        }
    }

    #[test]
    fn rascunho_de_ouro() {
        let produto = Uuid::from_u128(7);
        let d = montar_draft_data(
            &transacao(),
            Vinculos {
                produto: Some(produto),
                fornecedor: None,
                categoria: Some(Uuid::from_u128(9)),
            },
        )
        .expect("draft");
        let esperado: Value = serde_json::from_str(include_str!(
            "../../tests/dados/pluggy/rascunho_de_ouro.json"
        ))
        .expect("ouro");
        assert_eq!(d, esperado);
    }

    #[test]
    fn entrada_e_descricao_bruta_de_reserva() {
        let mut t = transacao();
        t.direcao = Direcao::Entrada;
        t.descricao = String::new();
        let d = montar_draft_data(&t, Vinculos::default()).expect("draft");
        assert_eq!(d["direction"], "inflow");
        assert_eq!(d["transaction_type"], "income");
        assert_eq!(d["description"], "PADARIA EXEMPLO 001");
        t.descricao_bruta = None;
        let d = montar_draft_data(&t, Vinculos::default()).expect("draft");
        assert_eq!(d["description"], Value::Null);
        assert_eq!(d["financial_product_id"], Value::Null);
    }

    #[test]
    fn data_torta_e_recusada() {
        let mut t = transacao();
        t.data = "10/03/2026".into();
        assert!(montar_draft_data(&t, Vinculos::default()).is_err());
        assert_eq!(referencia_externa("abc"), "pluggy:txn_abc");
    }
}
