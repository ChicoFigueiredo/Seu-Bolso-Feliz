//! O contrato de draft (porte do que `packages/contracts/src/draft/` faz para movimentações):
//! a migração v0 → v1 em memória e a conferência "pode virar movimentação?" que monta o
//! `p_insert_payload` de `fn_materialize_draft_record`.
//!
//! Só `transaction` é portado nesta fatia. Os outros tipos de rascunho (`recurring_template`,
//! `liability`, `consumption_metric`) chegam ao desktop e podem ser corrigidos, recusados e
//! lidos, mas **não lançados**: o servidor recusa com uma mensagem que manda usar a web. O
//! porte do resto vem com o porte da função SQL (teste de ouro; spec 08, fora do escopo).

use serde_json::{Map, Value, json};

use crate::dinheiro::numeric_de_centavos;

/// O `draft_data` no formato v1. Para `schema_version = 0` de movimentações, converte em
/// memória como `migrateV0ToV1` do TypeScript; os outros tipos em v0 vão como estão.
pub fn dados_v1(tipo: &str, dados: &Value, versao_do_esquema: i16) -> Value {
    if versao_do_esquema >= 1 || tipo != "transaction" {
        return dados.clone();
    }
    migrar_transacao_v0(dados)
}

fn texto(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

/// O primeiro valor presente entre as chaves (nem nulo nem vazio).
fn escolher<'a>(o: &'a Map<String, Value>, chaves: &[&str]) -> Option<&'a Value> {
    chaves
        .iter()
        .find_map(|k| o.get(*k).filter(|v| !v.is_null() && v.as_str() != Some("")))
}

/// `YYYY-MM-DD` do começo de um texto de data, ou nada.
fn data_iso(v: Option<&Value>) -> Option<String> {
    let t = v?.as_str()?;
    let cabeca = t.get(..10)?;
    let b = cabeca.as_bytes();
    let ok = b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        });
    ok.then(|| cabeca.to_owned())
}

/// Dinheiro cru da v0 (número ou texto em reais) para centavos. Lê o texto do número em vez
/// de multiplicar o `f64` quando dá, para `0.1` virar `10` sem arredondar errado.
fn para_centavos(v: Option<&Value>) -> Option<i64> {
    let v = v?;
    let texto = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_owned(),
        _ => return None,
    };
    if let Ok(c) = crate::dinheiro::centavos_do_numeric(&texto) {
        return Some(c);
    }
    let reais: f64 = texto.parse().ok()?;
    reais.is_finite().then(|| (reais * 100.0).round() as i64)
}

fn tipo_de_transacao_v0(v: Option<&Value>) -> Option<&'static str> {
    let t = v?.as_str()?.to_lowercase();
    Some(match t.as_str() {
        "despesa" | "expense" => "expense",
        "receita" | "income" => "income",
        "refund" => "refund",
        "adjustment" => "adjustment",
        "interest_charge" => "interest_charge",
        "fee" => "fee",
        "statement_payment" => "statement_payment",
        "liability_payment" => "liability_payment",
        _ => return None,
    })
}

fn migrar_transacao_v0(bruto: &Value) -> Value {
    let vazio = Map::new();
    let o = bruto.as_object().unwrap_or(&vazio);
    let direcao = match o.get("type").and_then(Value::as_str).map(str::to_lowercase) {
        Some(t) if matches!(t.as_str(), "receita" | "income" | "inflow") => "inflow",
        _ => "outflow",
    };
    json!({
        "schema_version": 1,
        "draft_type": "transaction",
        "provenance": {
            "source_document_id": null, "extraction_result_id": null,
            "parsed_version_id": null, "parser_type": null, "field_sources": {}
        },
        "obligation_id": null,
        "direction": direcao,
        "transaction_type": tipo_de_transacao_v0(o.get("type")),
        "amount_cents": para_centavos(escolher(o, &["amount", "total_amount", "totalAmount"])),
        "currency": o.get("currency").and_then(texto).unwrap_or_else(|| "BRL".into()),
        "event_date": data_iso(escolher(o, &["event_date", "eventDate"])),
        "due_date": data_iso(escolher(o, &["due_date", "dueDate"])),
        "competence_date": data_iso(escolher(o, &["competence_date", "competenceDate"])),
        "description": o.get("description").and_then(texto),
        "supplier_name_raw": escolher(o, &["supplier_name", "supplier_name_raw", "supplierNameRaw"]).and_then(texto),
        "supplier_id": escolher(o, &["supplier_id", "supplierId"]).and_then(texto),
        "category_suggestion": escolher(o, &["category", "category_suggestion"]).and_then(texto),
        "category_id": o.get("category_id").and_then(texto),
        "tags": o.get("tags").filter(|t| t.is_array()).cloned().unwrap_or_else(|| json!([])),
        "priority": null,
        "document_number": escolher(o, &["document_number", "documentNumber"]).and_then(texto),
        "contract_identifier": escolher(o, &["contract_identifier", "contractIdentifier"]).and_then(texto),
        "barcode_digitable_line": escolher(o, &["barcode_digitable_line", "digitable_line"]).and_then(texto),
        "financial_product_id": o.get("financial_product_id").and_then(texto),
        "notes": o.get("notes").and_then(texto),
    })
}

const TIPOS_DE_MOVIMENTACAO: [&str; 8] = [
    "income",
    "expense",
    "refund",
    "adjustment",
    "interest_charge",
    "fee",
    "statement_payment",
    "liability_payment",
];

const PRIORIDADES: [&str; 5] = ["essential", "high", "medium", "low", "optional"];

fn e_uuid(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
}

/// O que o `draft_data` v1 precisa ter para ser **corrigido** (a camada honesta): um objeto
/// com `schema_version: 1` e o `draft_type` do rascunho.
pub fn conferir_correcao(tipo: &str, dados: &Value) -> Result<(), String> {
    let Some(o) = dados.as_object() else {
        return Err("os dados do rascunho precisam ser um objeto".into());
    };
    if o.get("schema_version") != Some(&json!(1)) {
        return Err("os dados do rascunho precisam estar no formato v1 (schema_version 1)".into());
    }
    if o.get("draft_type").and_then(Value::as_str) != Some(tipo) {
        return Err("o tipo dos dados não é o do rascunho".into());
    }
    if tipo == "transaction" {
        if let Some(v) = o.get("amount_cents")
            && !v.is_null()
            && !v.is_i64()
        {
            return Err("amount_cents precisa ser um número inteiro de centavos".into());
        }
        for campo in ["financial_product_id", "category_id", "supplier_id"] {
            if let Some(v) = o.get(campo)
                && !v.is_null()
                && !e_uuid(Some(v))
            {
                return Err(format!("{campo} não é um id válido"));
            }
        }
        for campo in ["event_date", "due_date", "competence_date"] {
            if let Some(v) = o.get(campo)
                && !v.is_null()
                && data_iso(Some(v)).as_deref() != v.as_str()
            {
                return Err(format!("{campo} precisa estar em AAAA-MM-DD"));
            }
        }
    }
    Ok(())
}

/// A camada de lançamento: confere o que `transactions` exige como `NOT NULL` e monta o
/// `p_insert_payload` de `fn_materialize_draft_record`. `Err` traz as mensagens em
/// português, uma por problema.
pub fn payload_de_lancamento(tipo: &str, dados: &Value) -> Result<Value, Vec<String>> {
    if tipo != "transaction" {
        return Err(vec![format!(
            "o desktop ainda não lança rascunhos do tipo {tipo}: lance pela web"
        )]);
    }
    let vazio = Map::new();
    let o = dados.as_object().unwrap_or(&vazio);
    let mut erros = Vec::new();
    if o.get("schema_version") != Some(&json!(1)) {
        erros.push("os dados do rascunho não estão no formato v1".to_owned());
    }
    if !e_uuid(o.get("financial_product_id")) {
        erros.push("Selecione a conta ou cartão em que este lançamento entra".into());
    }
    let tipo_do_lancamento = o
        .get("transaction_type")
        .and_then(Value::as_str)
        .filter(|t| TIPOS_DE_MOVIMENTACAO.contains(t));
    if tipo_do_lancamento.is_none() {
        erros.push("Tipo do lançamento é obrigatório".into());
    }
    let centavos = o.get("amount_cents").and_then(Value::as_i64);
    if !centavos.is_some_and(|c| c > 0) {
        erros.push("Valor deve ser maior que zero".into());
    }
    let evento = data_iso(o.get("event_date"));
    if evento.is_none() {
        erros.push("Data do evento é obrigatória para lançar".into());
    }
    let categoria = o.get("category_id").filter(|v| !v.is_null());
    if categoria.is_some() && !e_uuid(categoria) {
        erros.push("category_id não é um id válido".into());
    }
    let prioridade = o
        .get("priority")
        .and_then(Value::as_str)
        .filter(|p| PRIORIDADES.contains(p));
    if !erros.is_empty() {
        return Err(erros);
    }
    let descricao = o
        .get("description")
        .and_then(texto)
        .or_else(|| o.get("supplier_name_raw").and_then(texto));
    let nulo = Value::Null;
    let pegar = |k: &str| o.get(k).unwrap_or(&nulo).clone();
    Ok(json!({
        "financial_product_id": pegar("financial_product_id"),
        "type": tipo_do_lancamento,
        "amount": numeric_de_centavos(centavos.unwrap_or_default()),
        "description": descricao,
        "event_date": evento,
        "competence_date": data_iso(o.get("competence_date")),
        "category_id": categoria.cloned(),
        "priority": prioridade,
        "notes": o.get("notes").and_then(texto),
        "origin_type": "import",
        "is_confirmed": true,
        "metadata": {
            "schema_version": 1,
            "supplier_id": pegar("supplier_id"),
            "supplier_name_raw": pegar("supplier_name_raw"),
            "document_number": pegar("document_number"),
            "contract_identifier": pegar("contract_identifier"),
            "due_date": pegar("due_date"),
            "tags": o.get("tags").cloned().unwrap_or_else(|| json!([])),
            "obligation_id": pegar("obligation_id"),
            "provenance": pegar("provenance"),
        }
    }))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v1_completo() -> Value {
        json!({
            "schema_version": 1, "draft_type": "transaction",
            "transaction_type": "expense", "amount_cents": 1250,
            "event_date": "2026-11-10",
            "financial_product_id": "00000000-0000-4000-8000-0000000000c1",
            "description": "Padaria Exemplo", "supplier_name_raw": null
        })
    }

    #[test]
    fn v0_migra_dinheiro_sem_arredondar_errado() {
        let v0 = json!({ "type": "despesa", "amount": 0.1, "due_date": "2026-11-20T00:00:00Z",
                         "supplier_name": "Luz Exemplo", "category": "Casa" });
        let v1 = dados_v1("transaction", &v0, 0);
        assert_eq!(v1["schema_version"], 1);
        assert_eq!(v1["amount_cents"], 10);
        assert_eq!(v1["transaction_type"], "expense");
        assert_eq!(v1["direction"], "outflow");
        assert_eq!(v1["due_date"], "2026-11-20");
        assert_eq!(v1["event_date"], Value::Null);
        assert_eq!(v1["category_suggestion"], "Casa");
        let texto = json!({ "type": "receita", "amount": "1234.56" });
        assert_eq!(dados_v1("transaction", &texto, 0)["amount_cents"], 123_456);
        assert_eq!(dados_v1("transaction", &texto, 0)["direction"], "inflow");
    }

    #[test]
    fn v1_e_outros_tipos_passam_como_estao() {
        let v = v1_completo();
        assert_eq!(dados_v1("transaction", &v, 1), v);
        let cru = json!({"qualquer": "coisa"});
        assert_eq!(dados_v1("liability", &cru, 0), cru);
    }

    #[test]
    fn lancamento_monta_o_payload_em_texto_exato() {
        let p = payload_de_lancamento("transaction", &v1_completo()).unwrap();
        assert_eq!(p["amount"], "12.50");
        assert_eq!(p["type"], "expense");
        assert_eq!(p["description"], "Padaria Exemplo");
        assert_eq!(p["origin_type"], "import");
    }

    #[test]
    fn lancamento_diz_tudo_que_falta() {
        let erros = payload_de_lancamento(
            "transaction",
            &json!({"schema_version": 1, "draft_type": "transaction"}),
        )
        .unwrap_err();
        assert_eq!(erros.len(), 4, "{erros:?}");
        let outro = payload_de_lancamento("liability", &json!({})).unwrap_err();
        assert!(outro[0].contains("web"));
    }

    #[test]
    fn correcao_exige_v1_e_o_tipo_certo() {
        assert!(conferir_correcao("transaction", &v1_completo()).is_ok());
        assert!(conferir_correcao("transaction", &json!({"schema_version": 0})).is_err());
        assert!(conferir_correcao("liability", &v1_completo()).is_err());
        let mut ruim = v1_completo();
        ruim["event_date"] = json!("10/11/2026");
        assert!(conferir_correcao("transaction", &ruim).is_err());
    }
}
