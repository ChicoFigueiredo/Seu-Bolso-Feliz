//! A reconciliação determinística de um draft recém-criado (`findReconciliationCandidates`
//! de `workers/ingestion`), só com as regras que valem para o sync da Pluggy (o draft não tem
//! `source_document_id`, então a regra 1, de hash duplicado, nunca dispara):
//!
//! - regra 2: mesmo fornecedor + valor ±5% numa janela de ±60 dias; data a ±7 dias é
//!   `match_exact` (0,95), senão `match_fuzzy` (0,70);
//! - regra 3: template recorrente mensal ativo do mesmo fornecedor (valor ±15% se houver);
//!   com instância no mês, 0,85, senão 0,60 (`match_recurrence`).
//!
//! Sem fornecedor resolvido nada casa: `no_match`, sem consulta ao banco.
//!
//! Diferença do Bun: a regra 3 consultava as instâncias até `AAAA-MM-31`, que o Postgres
//! recusa em fevereiro, abril, junho, setembro e novembro (a reconciliação do draft inteiro
//! falhava "sem ser crítico"). Aqui o limite é o último dia real do mês.

use chrono::{Datelike, Days, NaiveDate};
use serde_json::{Value, json};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::erro::ErroDeColeta;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidato {
    pub tipo: &'static str,
    pub pontuacao: f64,
    pub json: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resultado {
    pub situacao: &'static str,
    pub candidatos: Vec<Value>,
}

/// O que a reconciliação sabe do draft.
#[derive(Debug, Clone)]
pub struct DadosDoDraft<'a> {
    pub valor: f64,
    pub data: NaiveDate,
    pub fornecedor: Option<Uuid>,
    pub nome_do_fornecedor: Option<&'a str>,
}

/// `amountWithinPct`.
fn valor_proximo(a: f64, b: f64, pct: f64) -> bool {
    if a == 0.0 && b == 0.0 {
        return true;
    }
    let base = a.abs().max(b.abs());
    (a - b).abs() / base <= pct
}

fn ultimo_dia_do_mes(d: NaiveDate) -> NaiveDate {
    let primeiro_do_seguinte = if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
    };
    primeiro_do_seguinte
        .and_then(|p| p.checked_sub_days(Days::new(1)))
        .unwrap_or(d)
}

pub async fn encontrar_candidatos<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    d: &DadosDoDraft<'_>,
) -> Result<Resultado, ErroDeColeta> {
    let mut candidatos: Vec<Candidato> = Vec::new();
    let nome = d.nome_do_fornecedor;
    if let Some(fornecedor) = d.fornecedor {
        // Regra 2.
        let de = d.data.checked_sub_days(Days::new(60)).unwrap_or(d.data);
        let ate = d.data.checked_add_days(Days::new(60)).unwrap_or(d.data);
        let linhas = cliente
            .query(
                "SELECT id, amount::float8 AS amount, event_date, category_id FROM public.transactions \
                 WHERE user_id = $1 AND supplier_id = $2 AND event_date >= $3 AND event_date <= $4 \
                 ORDER BY event_date, id LIMIT 10",
                &[&dono, &fornecedor, &de, &ate],
            )
            .await?;
        for l in &linhas {
            let valor: Option<f64> = l.try_get("amount")?;
            let Some(valor) = valor else { continue };
            if !valor_proximo(d.valor, valor, 0.05) {
                continue;
            }
            let data: NaiveDate = l.try_get("event_date")?;
            let id: Uuid = l.try_get("id")?;
            let categoria: Option<Uuid> = l.try_get("category_id")?;
            let exato = (d.data - data).num_days().abs() <= 7;
            candidatos.push(Candidato {
                tipo: if exato { "match_exact" } else { "match_fuzzy" },
                pontuacao: if exato { 0.95 } else { 0.7 },
                json: json!({
                    "transactionId": id,
                    "recurringTemplateId": null,
                    "matchType": if exato { "match_exact" } else { "match_fuzzy" },
                    "score": if exato { 0.95 } else { 0.7 },
                    "description": if exato {
                        "Transação com mesmo fornecedor, valor e data encontrada."
                    } else {
                        "Transação com mesmo fornecedor e valor semelhante (data difere)."
                    },
                    "candidateData": {
                        "amount": valor,
                        "date": data.format("%Y-%m-%d").to_string(),
                        "supplierName": nome,
                        "category": categoria,
                    }
                }),
            });
        }

        // Regra 3.
        let modelos = cliente
            .query(
                "SELECT id, name, amount::float8 AS amount FROM public.recurring_templates \
                 WHERE user_id = $1 AND supplier_id = $2 AND is_active = true AND frequency = 'monthly' \
                 ORDER BY created_at, id LIMIT 5",
                &[&dono, &fornecedor],
            )
            .await?;
        let inicio = d.data.with_day(1).unwrap_or(d.data);
        let fim = ultimo_dia_do_mes(d.data);
        for m in &modelos {
            let id: Uuid = m.try_get("id")?;
            let nome_do_modelo: String = m.try_get("name")?;
            let valor: Option<f64> = m.try_get("amount")?;
            if let Some(v) = valor
                && !valor_proximo(d.valor, v, 0.15)
            {
                continue;
            }
            let instancia = cliente
                .query_opt(
                    "SELECT expected_date FROM public.recurring_instances \
                     WHERE user_id = $1 AND recurring_template_id = $2 \
                     AND expected_date >= $3 AND expected_date <= $4 LIMIT 1",
                    &[&dono, &id, &inicio, &fim],
                )
                .await?;
            let data_da_instancia: Option<NaiveDate> = instancia
                .as_ref()
                .map(|i| i.try_get("expected_date"))
                .transpose()?;
            let tem = data_da_instancia.is_some();
            candidatos.push(Candidato {
                tipo: "match_recurrence",
                pontuacao: if tem { 0.85 } else { 0.6 },
                json: json!({
                    "transactionId": null,
                    "recurringTemplateId": id,
                    "matchType": "match_recurrence",
                    "score": if tem { 0.85 } else { 0.6 },
                    "description": if tem {
                        format!("Template recorrente \"{nome_do_modelo}\" tem instância prevista neste período.")
                    } else {
                        format!("Template recorrente \"{nome_do_modelo}\" corresponde ao fornecedor (sem instância confirmada).")
                    },
                    "candidateData": {
                        "amount": valor,
                        "date": data_da_instancia.map(|x| x.format("%Y-%m-%d").to_string()),
                        "supplierName": nome,
                        "category": null,
                    }
                }),
            });
        }
    }
    // Do maior para o menor score; a ordenação é estável, como a do JavaScript.
    candidatos.sort_by(|a, b| b.pontuacao.total_cmp(&a.pontuacao));
    let situacao = candidatos.first().map_or("no_match", |c| c.tipo);
    Ok(Resultado {
        situacao,
        candidatos: candidatos.into_iter().map(|c| c.json).collect(),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn valor_proximo_por_percentual() {
        assert!(valor_proximo(100.0, 104.9, 0.05));
        assert!(!valor_proximo(100.0, 106.0, 0.05));
        assert!(valor_proximo(0.0, 0.0, 0.05));
        assert!(!valor_proximo(0.0, 1.0, 0.05));
        assert!(valor_proximo(100.0, 114.0, 0.15));
    }

    #[test]
    fn ultimo_dia_do_mes_real() {
        let d = |a, m, dia| NaiveDate::from_ymd_opt(a, m, dia).expect("data");
        assert_eq!(ultimo_dia_do_mes(d(2026, 2, 10)), d(2026, 2, 28));
        assert_eq!(ultimo_dia_do_mes(d(2024, 2, 10)), d(2024, 2, 29));
        assert_eq!(ultimo_dia_do_mes(d(2026, 11, 30)), d(2026, 11, 30));
        assert_eq!(ultimo_dia_do_mes(d(2026, 12, 1)), d(2026, 12, 31));
    }
}
