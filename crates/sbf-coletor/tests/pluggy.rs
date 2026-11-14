//! O sync da Pluggy contra um servidor falso local (respostas gravadas à mão) e, com
//! `SBF_PG_TESTE=1`, contra o Postgres de teste.

mod comum;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::NaiveDate;
use comum::{Falso, Pedido, arquivo, banco, novo_dono, subir};
use deadpool_postgres::Pool;
use sbf_coletor::config::{ConfigPluggy, Segredo};
use sbf_coletor::pluggy::cliente::ClientePluggy;
use sbf_coletor::pluggy::sincronizacao::{OpcoesDaSincronizacao, sincronizar_em};
use uuid::Uuid;

const SEGREDO: &str = "segredo-pluggy-ficticio";
const HOJE: &str = "2026-11-14";

fn hoje() -> NaiveDate {
    NaiveDate::parse_from_str(HOJE, "%Y-%m-%d").expect("data")
}

fn roteiro(pagina_2_falha: Arc<AtomicBool>) -> impl Fn(&Pedido) -> (u16, String) + Send + Sync {
    move |p: &Pedido| match (p.metodo.as_str(), p.caminho.as_str()) {
        ("POST", "/auth") => {
            if p.corpo.contains(SEGREDO) {
                (200, arquivo("pluggy/auth.json"))
            } else {
                (403, "{}".into())
            }
        }
        ("GET", _) if p.chave_da_api.as_deref() != Some("chave-de-api-ficticia") => {
            (401, "{}".into())
        }
        ("GET", "/accounts") => (200, arquivo("pluggy/contas.json")),
        ("GET", "/v2/transactions") => {
            if p.parametro("page").is_some() || p.parametro("pageSize").is_some() {
                return (400, "{}".into());
            }
            match (
                p.parametro("accountId").as_deref(),
                p.parametro("after").as_deref(),
            ) {
                (Some("c1"), None) => (200, arquivo("pluggy/transacoes_c1_p1.json")),
                (Some("c1"), Some("cursor-2%3D")) => {
                    if pagina_2_falha.load(Ordering::SeqCst) {
                        (500, "{\"message\":\"boom\"}".into())
                    } else {
                        (200, arquivo("pluggy/transacoes_c1_p2.json"))
                    }
                }
                (Some("c2"), None) => (200, arquivo("pluggy/transacoes_c2.json")),
                _ => (400, "{}".into()),
            }
        }
        _ => (404, "{}".into()),
    }
}

async fn pluggy_falso(falha: bool) -> (Falso, ClientePluggy, Arc<AtomicBool>) {
    let chave = Arc::new(AtomicBool::new(falha));
    let falso = subir(roteiro(Arc::clone(&chave))).await;
    let mut config = ConfigPluggy {
        client_id: "cliente-pluggy-ficticio".into(),
        client_secret: Segredo::novo(SEGREDO),
        base_api: falso.base.clone(),
    };
    config.base_api = falso.base.clone();
    (falso, ClientePluggy::novo(config), chave)
}

struct Cenario {
    dono: Uuid,
    produto: Uuid,
    conexao: Uuid,
}

/// Um dono com uma conexão `item-1`: c1 mapeada, c2 sem produto, c3 ignorada.
async fn montar(banco: &Pool) -> Cenario {
    let dono = novo_dono(banco).await;
    let c = banco.get().await.expect("conexão");
    let instituicao: Uuid = c
        .query_one(
            "INSERT INTO public.institutions (user_id, name) VALUES ($1, 'Banco Exemplo') RETURNING id",
            &[&dono],
        )
        .await
        .expect("instituição")
        .get(0);
    let produto: Uuid = c
        .query_one(
            "INSERT INTO public.financial_products (user_id, institution_id, name, type, current_balance) \
             VALUES ($1, $2, 'Conta Exemplo', 'checking_account', 0) RETURNING id",
            &[&dono, &instituicao],
        )
        .await
        .expect("produto")
        .get(0);
    let conexao: Uuid = c
        .query_one(
            "INSERT INTO public.provider_connections (user_id, provider, external_item_id, status) \
             VALUES ($1, 'pluggy', 'item-1', 'active') RETURNING id",
            &[&dono],
        )
        .await
        .expect("conexão pluggy")
        .get(0);
    for (conta, produto, situacao) in [
        ("c1", Some(produto), "mapped"),
        ("c2", None, "pending_mapping"),
        ("c3", None, "ignored"),
    ] {
        c.execute(
            "INSERT INTO public.external_account_mappings \
             (user_id, provider_connection_id, external_account_id, financial_product_id, status) \
             VALUES ($1, $2, $3, $4, $5)",
            &[&dono, &conexao, &conta, &produto, &situacao],
        )
        .await
        .expect("mapeamento");
    }
    Cenario {
        dono,
        produto,
        conexao,
    }
}

async fn contar(banco: &Pool, sql: &str, dono: Uuid) -> i64 {
    banco
        .get()
        .await
        .expect("conexão")
        .query_one(sql, &[&dono])
        .await
        .expect("contagem")
        .get(0)
}

#[tokio::test]
async fn incremental_cria_drafts_categorias_saldo_e_nao_duplica() {
    let Some(banco) = banco().await else { return };
    let cen = montar(&banco).await;
    let (falso, pluggy, _) = pluggy_falso(false).await;
    let c = banco.get().await.expect("conexão");

    // Um fornecedor com apelido e uma transação parecida: a reconciliação acha `match_exact`.
    let fornecedor: Uuid = c
        .query_one(
            "INSERT INTO public.suppliers (user_id, name) VALUES ($1, 'Padaria Exemplo') RETURNING id",
            &[&cen.dono],
        )
        .await
        .expect("fornecedor")
        .get(0);
    c.execute(
        "INSERT INTO public.supplier_aliases (user_id, supplier_id, alias_name) VALUES ($1, $2, 'PADARIA EXEMPLO ltda')",
        &[&cen.dono, &fornecedor],
    )
    .await
    .expect("apelido");
    c.execute(
        "INSERT INTO public.transactions (user_id, financial_product_id, type, amount, event_date, supplier_id) \
         VALUES ($1, $2, 'expense', 120.00, '2026-03-08', $3)",
        &[&cen.dono, &cen.produto, &fornecedor],
    )
    .await
    .expect("transação anterior");

    let r = sincronizar_em(
        &banco,
        &pluggy,
        cen.dono,
        &OpcoesDaSincronizacao::default(),
        hoje(),
    )
    .await
    .expect("sync");
    // c1: t-001, t-002 (página 1) e t-003 (página 2); c2: t-101; c3 é ignorada.
    assert_eq!(
        (r.conexoes, r.criados, r.ja_sincronizados, r.erros_de_item),
        (1, 4, 0, 0)
    );

    // O modo incremental, sem --de: 7 dias (a conexão nunca sincronizou), sem checkpoint.
    let primeiro = &falso.pedidos_em("/v2/transactions")[0];
    assert_eq!(primeiro.parametro("accountId").as_deref(), Some("c1"));
    assert_eq!(
        primeiro.parametro("dateFrom").as_deref(),
        Some("2026-11-07")
    );
    assert_eq!(primeiro.parametro("dateTo").as_deref(), Some(HOJE));
    assert_eq!(
        primeiro.chave_da_api.as_deref(),
        Some("chave-de-api-ficticia")
    );
    assert_eq!(
        falso.pedidos_em("/auth").len(),
        1,
        "a chave da API é reaproveitada"
    );
    assert_eq!(
        contar(
            &banco,
            "SELECT count(*) FROM public.ingestion_checkpoints WHERE user_id = $1",
            cen.dono
        )
        .await,
        0
    );
    // A segunda página usa só o token `after`, nunca a query string inteira do `next`.
    let segunda = &falso.pedidos_em("/v2/transactions")[1];
    assert_eq!(segunda.parametro("after").as_deref(), Some("cursor-2%3D"));
    assert_eq!(segunda.parametro("accountId").as_deref(), Some("c1"));

    let l = c
        .query_one(
            "SELECT status, draft_type, draft_schema_version, confidence_score::text AS conf, \
             source_document_id, draft_data, reconciliation_status, reconciliation_candidates, \
             materialization_key, batch_id FROM public.draft_records \
             WHERE user_id = $1 AND external_ref = 'pluggy:txn_t-001'",
            &[&cen.dono],
        )
        .await
        .expect("draft t-001");
    assert_eq!(l.get::<_, String>("status"), "pending_review");
    assert_eq!(l.get::<_, String>("draft_type"), "transaction");
    assert_eq!(l.get::<_, i16>("draft_schema_version"), 1);
    assert_eq!(l.get::<_, String>("conf"), "1.00");
    assert!(l.get::<_, Option<Uuid>>("source_document_id").is_none());
    assert!(
        l.get::<_, String>("materialization_key")
            .starts_with("draft:")
    );
    let dados: serde_json::Value = l.get("draft_data");
    assert_eq!(dados["amount_cents"], 12345);
    assert_eq!(dados["direction"], "outflow");
    assert_eq!(dados["event_date"], "2026-03-10");
    assert_eq!(dados["supplier_name_raw"], "Padaria Exemplo LTDA");
    assert_eq!(dados["supplier_id"], fornecedor.to_string());
    assert_eq!(dados["financial_product_id"], cen.produto.to_string());
    assert_eq!(dados["category_suggestion"], "Eating out");
    assert_eq!(l.get::<_, String>("reconciliation_status"), "match_exact");
    let candidatos: serde_json::Value = l.get("reconciliation_candidates");
    assert_eq!(candidatos[0]["matchType"], "match_exact");
    assert_eq!(candidatos[0]["score"], 0.95);
    assert_eq!(candidatos[0]["candidateData"]["amount"], 120.0);
    assert_eq!(candidatos[0]["candidateData"]["date"], "2026-03-08");

    // O cartão (c2) não tem produto vinculado, e o draft fica com `financial_product_id` nulo.
    let cartao: serde_json::Value = c
        .query_one(
            "SELECT draft_data FROM public.draft_records WHERE user_id = $1 AND external_ref = 'pluggy:txn_t-101'",
            &[&cen.dono],
        )
        .await
        .expect("t-101")
        .get(0);
    assert_eq!(cartao["financial_product_id"], serde_json::Value::Null);
    assert_eq!(
        cartao["amount_cents"], 9990,
        "o sinal invertido do cartão vira magnitude"
    );
    assert_eq!(cartao["direction"], "outflow");
    let salario: serde_json::Value = c
        .query_one(
            "SELECT draft_data FROM public.draft_records WHERE user_id = $1 AND external_ref = 'pluggy:txn_t-002'",
            &[&cen.dono],
        )
        .await
        .expect("t-002")
        .get(0);
    assert_eq!(
        (
            salario["direction"].clone(),
            salario["transaction_type"].clone()
        ),
        ("inflow".into(), "income".into())
    );
    let sem_candidato: String = c
        .query_one(
            "SELECT reconciliation_status FROM public.draft_records WHERE user_id = $1 AND external_ref = 'pluggy:txn_t-002'",
            &[&cen.dono],
        )
        .await
        .expect("t-002")
        .get(0);
    assert_eq!(sem_candidato, "no_match");

    // Um lote só, com o total; categorias criadas em português.
    let lote = c
        .query_one(
            "SELECT count(*) AS n, max(total_drafts) AS total, max(status) AS status \
             FROM public.draft_batches WHERE user_id = $1",
            &[&cen.dono],
        )
        .await
        .expect("lote");
    assert_eq!(
        (lote.get::<_, i64>("n"), lote.get::<_, Option<i32>>("total")),
        (1, Some(4))
    );
    assert_eq!(lote.get::<_, String>("status"), "open");
    let categorias: Vec<String> = c
        .query(
            "SELECT name FROM public.categories WHERE user_id = $1 ORDER BY name",
            &[&cen.dono],
        )
        .await
        .expect("categorias")
        .iter()
        .map(|l| l.get(0))
        .collect();
    assert_eq!(
        categorias,
        vec!["Compras online", "Mercado", "Restaurantes", "Salário"]
    );
    let com_categoria: i64 = contar(
        &banco,
        "SELECT count(*) FROM public.draft_records WHERE user_id = $1 AND (draft_data->>'category_id') IS NOT NULL",
        cen.dono,
    )
    .await;
    assert_eq!(com_categoria, 4);

    // Saldo: só a conta BANK mapeada (c1), exato em centavos; a conexão ganha `last_synced_at`.
    let saldo: String = c
        .query_one(
            "SELECT current_balance::text FROM public.financial_products WHERE id = $1",
            &[&cen.produto],
        )
        .await
        .expect("saldo")
        .get(0);
    assert_eq!(saldo, "1234.56");
    let sincronizada: bool = c
        .query_one(
            "SELECT last_synced_at IS NOT NULL FROM public.provider_connections WHERE id = $1",
            &[&cen.conexao],
        )
        .await
        .expect("conexão")
        .get(0);
    assert!(sincronizada);

    // Repetir não duplica nem cria lote vazio.
    let r2 = sincronizar_em(
        &banco,
        &pluggy,
        cen.dono,
        &OpcoesDaSincronizacao::default(),
        hoje(),
    )
    .await
    .expect("2o sync");
    assert_eq!((r2.criados, r2.ja_sincronizados), (0, 4));
    assert_eq!(
        contar(
            &banco,
            "SELECT count(*) FROM public.draft_records WHERE user_id = $1",
            cen.dono
        )
        .await,
        4
    );
    assert_eq!(
        contar(
            &banco,
            "SELECT count(*) FROM public.draft_batches WHERE user_id = $1",
            cen.dono
        )
        .await,
        1
    );
}

#[tokio::test]
async fn backfill_retoma_da_pagina_que_falhou() {
    let Some(banco) = banco().await else { return };
    let cen = montar(&banco).await;
    let (falso, pluggy, falha) = pluggy_falso(true).await;
    let backfill = OpcoesDaSincronizacao {
        backfill: true,
        ..Default::default()
    };

    // A página 2 de c1 falha: a conexão conta como falha, mas a página 1 ficou gravada.
    let r = sincronizar_em(&banco, &pluggy, cen.dono, &backfill, hoje())
        .await
        .expect("sync");
    assert_eq!((r.conexoes, r.conexoes_com_falha, r.criados), (1, 1, 0));
    let c = banco.get().await.expect("conexão");
    let cp = c
        .query_one(
            "SELECT status, cursor_kind, cursor_value, window_start::text AS de, window_end::text AS ate, \
             messages_seen, documents_created, duplicates_skipped, errors \
             FROM public.ingestion_checkpoints WHERE user_id = $1 AND source_type = 'pluggy' AND scope_key = 'item-1:c1'",
            &[&cen.dono],
        )
        .await
        .expect("checkpoint");
    assert_eq!(cp.get::<_, String>("status"), "running");
    assert_eq!(cp.get::<_, String>("cursor_kind"), "page_number");
    assert_eq!(
        cp.get::<_, String>("cursor_value"),
        "cursor-2=",
        "o cursor é só o token `after`, decodificado"
    );
    assert_eq!(cp.get::<_, String>("de"), "2025-11-14", "365 dias");
    assert_eq!(cp.get::<_, String>("ate"), HOJE);
    assert_eq!(
        (
            cp.get::<_, i32>("messages_seen"),
            cp.get::<_, i32>("documents_created"),
            cp.get::<_, i32>("errors")
        ),
        (2, 2, 0)
    );
    assert_eq!(
        contar(
            &banco,
            "SELECT count(*) FROM public.draft_records WHERE user_id = $1",
            cen.dono
        )
        .await,
        2
    );
    let conexao_sem_data: bool = c
        .query_one(
            "SELECT last_synced_at IS NULL FROM public.provider_connections WHERE id = $1",
            &[&cen.conexao],
        )
        .await
        .expect("conexão")
        .get(0);
    assert!(
        conexao_sem_data,
        "uma conexão que falhou não ganha last_synced_at"
    );

    // A página volta: o backfill retoma de `cursor-2`, sem refazer a página 1 de c1.
    falha.store(false, Ordering::SeqCst);
    let antes = falso.pedidos_em("/v2/transactions").len();
    let r2 = sincronizar_em(&banco, &pluggy, cen.dono, &backfill, hoje())
        .await
        .expect("retomada");
    assert_eq!(
        (r2.conexoes_com_falha, r2.criados, r2.ja_sincronizados),
        (0, 2, 0)
    );
    let novos: Vec<_> = falso
        .pedidos_em("/v2/transactions")
        .into_iter()
        .skip(antes)
        .collect();
    assert_eq!(novos[0].parametro("accountId").as_deref(), Some("c1"));
    assert_eq!(
        novos[0].parametro("after").as_deref(),
        Some("cursor-2%3D"),
        "retoma da página 2"
    );
    assert_eq!(
        novos[0].parametro("dateFrom").as_deref(),
        Some("2025-11-14")
    );
    assert_eq!(
        contar(
            &banco,
            "SELECT count(*) FROM public.draft_records WHERE user_id = $1",
            cen.dono
        )
        .await,
        4
    );
    let (status, vistos): (String, i32) = {
        let l = c
            .query_one(
                "SELECT status, messages_seen FROM public.ingestion_checkpoints WHERE user_id = $1 AND scope_key = 'item-1:c1'",
                &[&cen.dono],
            )
            .await
            .expect("checkpoint");
        (l.get(0), l.get(1))
    };
    assert_eq!((status.as_str(), vistos), ("completed", 3));

    // Um novo backfill, com o checkpoint concluído, recomeça do início da janela.
    let r3 = sincronizar_em(&banco, &pluggy, cen.dono, &backfill, hoje())
        .await
        .expect("de novo");
    assert_eq!((r3.criados, r3.ja_sincronizados), (0, 4));
}

#[tokio::test]
async fn periodo_pedido_e_data_invalida() {
    let Some(banco) = banco().await else { return };
    let cen = montar(&banco).await;
    let (falso, pluggy, _) = pluggy_falso(false).await;
    let opcoes = OpcoesDaSincronizacao {
        de: Some("2026-01-01".into()),
        ate: Some("2026-02-01".into()),
        ..Default::default()
    };
    sincronizar_em(&banco, &pluggy, cen.dono, &opcoes, hoje())
        .await
        .expect("sync");
    let p = &falso.pedidos_em("/v2/transactions")[0];
    assert_eq!(p.parametro("dateFrom").as_deref(), Some("2026-01-01"));
    assert_eq!(p.parametro("dateTo").as_deref(), Some("2026-02-01"));

    let torta = OpcoesDaSincronizacao {
        de: Some("ontem".into()),
        ..Default::default()
    };
    assert!(
        sincronizar_em(&banco, &pluggy, cen.dono, &torta, hoje())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn auth_recusada_nao_vaza_o_segredo() {
    let falso = subir(roteiro(Arc::new(AtomicBool::new(false)))).await;
    let config = ConfigPluggy {
        client_id: "x".into(),
        client_secret: Segredo::novo("outro-segredo-ficticio"),
        base_api: falso.base.clone(),
    };
    let erro = ClientePluggy::novo(config)
        .listar_contas("item-1")
        .await
        .expect_err("403")
        .to_string();
    assert!(erro.contains("HTTP 403"), "{erro}");
    assert!(!erro.contains("outro-segredo-ficticio"), "{erro}");
}
