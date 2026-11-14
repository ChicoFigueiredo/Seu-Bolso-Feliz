//! A fatia 3 contra o Postgres de teste (Docker, dados fictícios): rascunhos, edição,
//! delta por cursor e envio de documentos. CX-01 a CX-07 e CX-09 da spec 08.
//! Só roda com `SBF_PG_TESTE=1`: `scripts/desktop/postgres-de-teste.sh testar`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use axum::http::StatusCode;
use comum::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn hex(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// Uma proposta v2: `bases` são `(coleção, id, versão)`.
fn proposta(acao: Value, item: (&str, Uuid), bases: &[(&str, Uuid, &str)]) -> Value {
    json!({
        "id": Uuid::new_v4(),
        "acao": acao,
        "item": { "colecao": item.0, "id": item.1 },
        "versoes_de_base": bases.iter().map(|(c, i, v)| json!({"colecao": c, "id": i, "versao": v})).collect::<Vec<_>>(),
        "feita_em": "2026-11-14T10:00:00Z"
    })
}

async fn versao_de(amb: &Ambiente, cred: &str, colecao: &str, id: Uuid) -> String {
    let r = amb.get(&format!("/api/v2/livro/{colecao}"), cred).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    r.corpo["linhas"]["linhas"]
        .as_array()
        .expect("linhas")
        .iter()
        .find(|l| l["id"] == json!(id))
        .unwrap_or_else(|| panic!("{colecao} {id} não veio"))["versao"]
        .as_str()
        .expect("versão")
        .to_owned()
}

async fn contar(amb: &Ambiente, sql: &str, dono: Uuid) -> i64 {
    let c = amb.banco.get().await.unwrap();
    c.query_one(sql, &[&dono]).await.unwrap().get(0)
}

struct Cena {
    amb: Ambiente,
    dono: Uuid,
    cred: String,
    s: Semeado,
}

async fn cena() -> Option<Cena> {
    let amb = ambiente().await?;
    let (dono, email, sub) = novo_dono(&amb).await;
    let s = semear(&amb, dono).await;
    let (_e, cred) = amb.parear(&email, &sub, "Notebook").await;
    Some(Cena { amb, dono, cred, s })
}

// ───────────────────────────── CX-01: aprovar ─────────────────────────────

#[tokio::test]
async fn cx01_aprovar_rascunho_cria_a_movimentacao_pela_funcao_existente() {
    let Some(c) = cena().await else { return };
    let (lote, ids) = semear_lote(&c.amb, c.dono, c.s.conta, 2, 0).await;
    let base = versao_de(&c.amb, &c.cred, "rascunhos", ids[0]).await;
    let antes = contar(
        &c.amb,
        "SELECT count(*) FROM public.transactions WHERE user_id = $1",
        c.dono,
    )
    .await;

    let p = proposta(
        json!({"tipo": "aprovar_rascunho", "rascunho": ids[0]}),
        ("rascunhos", ids[0]),
        &[("rascunhos", ids[0], &base)],
    );
    let r = c.amb.post("/api/v2/alteracoes", &c.cred, p).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);

    // O rascunho ficou lançado e apontando para a movimentação nova.
    let linhas = r.corpo["linhas"].as_array().unwrap();
    let rascunho = linhas.iter().find(|l| l["colecao"] == "rascunhos").unwrap();
    assert_eq!(rascunho["linha"]["estado"], "lancado");
    let mov = linhas
        .iter()
        .find(|l| l["colecao"] == "movimentacoes")
        .unwrap();
    assert_eq!(rascunho["linha"]["lancado_como"], mov["linha"]["id"]);
    assert_eq!(mov["linha"]["valor"], 1250);
    assert_eq!(mov["linha"]["descricao"], "Rascunho 0");
    assert_eq!(mov["linha"]["origem"], "importacao");
    let lote_agora = linhas
        .iter()
        .find(|l| l["colecao"] == "lotes-de-rascunhos")
        .unwrap();
    assert_eq!(lote_agora["linha"]["estado"], "em_revisao");
    assert_eq!(lote_agora["linha"]["aprovados"], 1);

    let depois = contar(
        &c.amb,
        "SELECT count(*) FROM public.transactions WHERE user_id = $1",
        c.dono,
    )
    .await;
    assert_eq!(depois, antes + 1);
    // A função existente deixou a auditoria dela.
    let auditorias = contar(
        &c.amb,
        "SELECT count(*) FROM public.audit_logs WHERE user_id = $1 AND action = 'draft_materialized'",
        c.dono,
    )
    .await;
    assert_eq!(auditorias, 1);
    let _ = lote;
}

#[tokio::test]
async fn corrigir_depois_recusar_e_conciliar() {
    let Some(c) = cena().await else { return };
    let (_lote, ids) = semear_lote(&c.amb, c.dono, c.s.conta, 3, 0).await;

    // Corrigir grava v1, `corrected` e `manual_edit`.
    let base = versao_de(&c.amb, &c.cred, "rascunhos", ids[0]).await;
    let dados = json!({
        "schema_version": 1, "draft_type": "transaction", "transaction_type": "expense",
        "amount_cents": 9999, "event_date": "2026-11-11", "financial_product_id": c.s.conta,
        "description": "Corrigido"
    });
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "corrigir_rascunho", "rascunho": ids[0], "dados": dados}),
                ("rascunhos", ids[0]),
                &[("rascunhos", ids[0], &base)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    let rasc = &r.corpo["linhas"][0]["linha"];
    assert_eq!(rasc["estado"], "corrigido");
    assert_eq!(rasc["correcoes"]["manual_edit"], true);
    assert_eq!(rasc["dados"]["amount_cents"], 9999);
    assert_ne!(rasc["versao"], json!(base));

    // Dados fora do formato: pedido inválido, nada muda.
    let nova = rasc["versao"].as_str().unwrap().to_owned();
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "corrigir_rascunho", "rascunho": ids[0], "dados": {"schema_version": 0}}),
                ("rascunhos", ids[0]),
                &[("rascunhos", ids[0], &nova)],
            ),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.corpo["codigo"], "pedido_invalido");

    // Recusar com motivo.
    let base1 = versao_de(&c.amb, &c.cred, "rascunhos", ids[1]).await;
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "recusar_rascunho", "rascunho": ids[1], "motivo": "duplicado"}),
                ("rascunhos", ids[1]),
                &[("rascunhos", ids[1], &base1)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    assert_eq!(r.corpo["linhas"][0]["linha"]["estado"], "recusado");
    assert_eq!(
        r.corpo["linhas"][0]["linha"]["correcoes"]["rejection_reason"],
        "duplicado"
    );

    // Conciliar com uma movimentação existente.
    let base2 = versao_de(&c.amb, &c.cred, "rascunhos", ids[2]).await;
    let base_mov = versao_de(&c.amb, &c.cred, "movimentacoes", c.s.movimentacao).await;
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "conciliar_rascunho", "rascunho": ids[2], "movimentacao": c.s.movimentacao}),
                ("rascunhos", ids[2]),
                &[("rascunhos", ids[2], &base2), ("movimentacoes", c.s.movimentacao, &base_mov)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    assert_eq!(
        r.corpo["linhas"][0]["linha"]["conciliacao"],
        "confirmado_duplicata"
    );
    assert_eq!(
        r.corpo["linhas"][0]["linha"]["movimentacao_conciliada"],
        json!(c.s.movimentacao)
    );

    // O lote refletiu: 0 pendente, 1 recusado... ainda há o corrigido pendente.
    let lotes = c.amb.get("/api/v2/livro/lotes-de-rascunhos", &c.cred).await;
    let lote = &lotes.corpo["linhas"]["linhas"][0];
    assert_eq!(lote["recusados"], 1);
}

// ───────────────────────────── CX-02: tudo ou nada ─────────────────────────────

#[tokio::test]
async fn cx02_lote_com_rascunho_invalido_nao_aprova_nenhum() {
    let Some(c) = cena().await else { return };
    let (lote, ids) = semear_lote(&c.amb, c.dono, c.s.conta, 2, 1).await;
    let antes = contar(
        &c.amb,
        "SELECT count(*) FROM public.transactions WHERE user_id = $1",
        c.dono,
    )
    .await;
    let mut bases = vec![(
        "lotes-de-rascunhos",
        lote,
        versao_de(&c.amb, &c.cred, "lotes-de-rascunhos", lote).await,
    )];
    for id in &ids {
        bases.push((
            "rascunhos",
            *id,
            versao_de(&c.amb, &c.cred, "rascunhos", *id).await,
        ));
    }
    let bases_ref: Vec<(&str, Uuid, &str)> =
        bases.iter().map(|(a, b, v)| (*a, *b, v.as_str())).collect();
    let p = proposta(
        json!({"tipo": "aprovar_lote_de_rascunhos", "lote": lote}),
        ("lotes-de-rascunhos", lote),
        &bases_ref,
    );
    let r = c.amb.post("/api/v2/alteracoes", &c.cred, p.clone()).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    assert_eq!(r.corpo["resultado"], "recusada", "{:?}", r.corpo);
    assert_eq!(r.corpo["motivo"], "regra_violada");
    assert!(
        r.corpo["mensagem"]
            .as_str()
            .unwrap()
            .contains(&ids[2].to_string())
    );

    // Nada foi lançado nem aprovado: nem os dois bons.
    assert_eq!(
        contar(
            &c.amb,
            "SELECT count(*) FROM public.transactions WHERE user_id = $1",
            c.dono
        )
        .await,
        antes
    );
    assert_eq!(
        contar(
            &c.amb,
            "SELECT count(*) FROM public.draft_records WHERE user_id = $1 AND status = 'pending_review'",
            c.dono
        )
        .await,
        3
    );
    // E o lado a lado traz o lote e os rascunhos como estão.
    assert!(r.corpo["no_servidor_agora"].as_array().unwrap().len() >= 4);

    // Sem o inválido, o lote inteiro passa.
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            {
                // Recusa o inválido (ação própria) e refaz o lote sobre as versões novas.
                let base_i = versao_de(&c.amb, &c.cred, "rascunhos", ids[2]).await;
                let r = c
                    .amb
                    .post(
                        "/api/v2/alteracoes",
                        &c.cred,
                        proposta(
                            json!({"tipo": "recusar_rascunho", "rascunho": ids[2], "motivo": "sem conta"}),
                            ("rascunhos", ids[2]),
                            &[("rascunhos", ids[2], &base_i)],
                        ),
                    )
                    .await;
                assert_eq!(r.corpo["resultado"], "aceita");
                let mut bases = vec![("lotes-de-rascunhos", lote, versao_de(&c.amb, &c.cred, "lotes-de-rascunhos", lote).await)];
                for id in &ids[..2] {
                    bases.push(("rascunhos", *id, versao_de(&c.amb, &c.cred, "rascunhos", *id).await));
                }
                let b: Vec<(&str, Uuid, &str)> = bases.iter().map(|(a, b, v)| (*a, *b, v.as_str())).collect();
                proposta(json!({"tipo": "aprovar_lote_de_rascunhos", "lote": lote}), ("lotes-de-rascunhos", lote), &b)
            },
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    assert_eq!(
        contar(
            &c.amb,
            "SELECT count(*) FROM public.transactions WHERE user_id = $1",
            c.dono
        )
        .await,
        antes + 2
    );
    let lote_agora = r.corpo["linhas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["colecao"] == "lotes-de-rascunhos")
        .unwrap();
    assert_eq!(lote_agora["linha"]["estado"], "parcial");
}

#[tokio::test]
async fn lote_que_ganhou_um_rascunho_e_recusado_pela_base() {
    let Some(c) = cena().await else { return };
    let (lote, ids) = semear_lote(&c.amb, c.dono, c.s.conta, 1, 0).await;
    let bases = [
        (
            "lotes-de-rascunhos",
            lote,
            versao_de(&c.amb, &c.cred, "lotes-de-rascunhos", lote).await,
        ),
        (
            "rascunhos",
            ids[0],
            versao_de(&c.amb, &c.cred, "rascunhos", ids[0]).await,
        ),
    ];
    // O pipeline acrescenta um rascunho ao lote depois da cópia da estação.
    c.amb
        .sql(&format!(
            "INSERT INTO public.draft_records (batch_id, user_id, draft_type, status, draft_data, draft_schema_version) \
             VALUES ('{lote}', '{}', 'transaction', 'pending_review', '{{}}'::jsonb, 1)",
            c.dono
        ))
        .await;
    let b: Vec<(&str, Uuid, &str)> = bases.iter().map(|(a, b, v)| (*a, *b, v.as_str())).collect();
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "aprovar_lote_de_rascunhos", "lote": lote}),
                ("lotes-de-rascunhos", lote),
                &b,
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "recusada");
    assert_eq!(r.corpo["motivo"], "versao_de_base_divergente");
}

// ─────────────────── CX-03, CX-04: versão de base e reenvio ───────────────────

#[tokio::test]
async fn cx03_e_cx04_base_velha_recusa_sem_mudar_nada_e_reenvio_devolve_o_mesmo() {
    let Some(c) = cena().await else { return };
    let mov = c.s.movimentacao;
    let base = versao_de(&c.amb, &c.cred, "movimentacoes", mov).await;
    let edicao = |descricao: &str, valor: i64| {
        json!({"tipo": "editar_movimentacao", "movimentacao": mov, "edicao": {
            "descricao": descricao, "data": "2026-11-05", "valor": valor, "conta": c.s.conta,
            "fornecedor": null, "centro": null, "notas": "editada"
        }})
    };
    // A primeira passa.
    let p1 = proposta(
        edicao("Luz editada", 15000),
        ("movimentacoes", mov),
        &[("movimentacoes", mov, &base)],
    );
    let r1 = c.amb.post("/api/v2/alteracoes", &c.cred, p1.clone()).await;
    assert_eq!(r1.corpo["resultado"], "aceita", "{:?}", r1.corpo);
    // CX-04: reenviar a mesma devolve a mesma resposta, byte a byte, e não grava duas vezes.
    let r1b = c.amb.post("/api/v2/alteracoes", &c.cred, p1).await;
    assert_eq!(r1b.corpo, r1.corpo);
    assert_eq!(
        contar(
            &c.amb,
            "SELECT count(*) FROM public.alteracoes WHERE user_id = $1",
            c.dono
        )
        .await,
        1
    );
    // CX-03: a segunda parte da mesma base velha e é recusada; nada muda.
    let p2 = proposta(
        edicao("Outra edição", 99999),
        ("movimentacoes", mov),
        &[("movimentacoes", mov, &base)],
    );
    let r2 = c.amb.post("/api/v2/alteracoes", &c.cred, p2).await;
    assert_eq!(r2.corpo["resultado"], "recusada");
    assert_eq!(r2.corpo["motivo"], "versao_de_base_divergente");
    assert_eq!(
        r2.corpo["no_servidor_agora"][0]["linha"]["descricao"],
        "Luz editada"
    );
    let c0 = c.amb.banco.get().await.unwrap();
    let linha = c0
        .query_one(
            "SELECT description, amount::text AS v FROM public.transactions WHERE id = $1",
            &[&mov],
        )
        .await
        .unwrap();
    assert_eq!(linha.get::<_, String>("description"), "Luz editada");
    assert_eq!(linha.get::<_, String>("v"), "150.00");
}

// ───────────────────────────── edição, rateio, exclusão ─────────────────────────────

#[tokio::test]
async fn cx05_ratear_valida_a_soma_e_troca_o_conjunto() {
    let Some(c) = cena().await else { return };
    let mov = c.s.movimentacao; // 150,00, com um rateio de 100,00
    let base = versao_de(&c.amb, &c.cred, "movimentacoes", mov).await;
    let outro_centro = Uuid::new_v4();
    c.amb
        .sql(&format!(
            "INSERT INTO public.financial_centers (id, user_id, name, type) VALUES ('{outro_centro}', '{}', 'Outro {outro_centro}', 'pessoa')",
            c.dono
        ))
        .await;
    let ratear = |a: i64, b: i64| {
        json!({"tipo": "ratear_movimentacao", "movimentacao": mov, "rateios": [
            {"id": Uuid::new_v4(), "centro": c.s.centro, "valor": a, "nota": null},
            {"id": Uuid::new_v4(), "centro": outro_centro, "valor": b, "nota": "metade"}
        ]})
    };
    // CX-05: soma diferente é pedido inválido, e nada muda.
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                ratear(10_000, 4_000),
                ("movimentacoes", mov),
                &[("movimentacoes", mov, &base)],
            ),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{:?}", r.corpo);
    assert_eq!(r.corpo["codigo"], "pedido_invalido");
    assert_eq!(
        contar(
            &c.amb,
            "SELECT count(*) FROM public.transaction_allocations WHERE user_id = $1",
            c.dono
        )
        .await,
        1
    );
    // Soma certa: troca o conjunto inteiro.
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                ratear(10_000, 5_000),
                ("movimentacoes", mov),
                &[("movimentacoes", mov, &base)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    let nova = r.corpo["linhas"][0]["linha"]["versao"].as_str().unwrap();
    assert_ne!(nova, base, "ratear muda a versão da movimentação");
    assert_eq!(r.corpo["excluidas"][0]["colecao"], "rateios");
    assert_eq!(r.corpo["excluidas"][0]["id"], json!(c.s.rateio));
    assert_eq!(
        contar(
            &c.amb,
            "SELECT count(*) FROM public.transaction_allocations WHERE user_id = $1",
            c.dono
        )
        .await,
        2
    );
}

#[tokio::test]
async fn editar_com_centro_e_excluir_apagam_os_rateios_e_a_exclusao_entra_no_delta() {
    let Some(c) = cena().await else { return };
    let mov = c.s.movimentacao;
    let base = versao_de(&c.amb, &c.cred, "movimentacoes", mov).await;
    // Editar com centro: o rateio vira um só, do valor inteiro.
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "editar_movimentacao", "movimentacao": mov, "edicao": {
                    "descricao": "Luz", "data": "2026-11-06", "valor": 20000, "conta": c.s.conta,
                    "fornecedor": c.s.fornecedor, "centro": c.s.centro, "notas": null}}),
                ("movimentacoes", mov),
                &[("movimentacoes", mov, &base)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    let rateios: Vec<&Value> = r.corpo["linhas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["colecao"] == "rateios")
        .collect();
    assert_eq!(rateios.len(), 1);
    assert_eq!(rateios[0]["linha"]["valor"], 20000);

    // Cursor depois da edição; então excluir.
    let d0 = c.amb.get("/api/v2/livro/movimentacoes", &c.cred).await;
    let cursor = d0.corpo["cursor"].as_str().unwrap().to_owned();
    let d0r = c.amb.get("/api/v2/livro/rateios", &c.cred).await;
    let cursor_r = d0r.corpo["cursor"].as_str().unwrap().to_owned();
    let base = versao_de(&c.amb, &c.cred, "movimentacoes", mov).await;
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "excluir_movimentacao", "movimentacao": mov}),
                ("movimentacoes", mov),
                &[("movimentacoes", mov, &base)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    assert_eq!(r.corpo["excluidas"].as_array().unwrap().len(), 2);

    // CX-06: o delta traz a exclusão da movimentação e a do rateio, e nenhuma linha.
    let d = c
        .amb
        .get(
            &format!("/api/v2/livro/movimentacoes?desde={cursor}"),
            &c.cred,
        )
        .await;
    assert_eq!(d.corpo["completo"], false);
    assert_eq!(d.corpo["excluidas"], json!([mov]));
    assert_eq!(d.corpo["linhas"]["linhas"], json!([]));
    let dr = c
        .amb
        .get(&format!("/api/v2/livro/rateios?desde={cursor_r}"), &c.cred)
        .await;
    assert_eq!(dr.corpo["excluidas"].as_array().unwrap().len(), 1);
}

// ───────────────────────────── CX-06: delta ─────────────────────────────

#[tokio::test]
async fn cx06_delta_traz_so_o_que_mudou_e_duas_leituras_seguidas_vem_vazias() {
    let Some(c) = cena().await else { return };
    // Carga completa.
    let d0 = c.amb.get("/api/v2/livro/movimentacoes", &c.cred).await;
    assert_eq!(d0.status, StatusCode::OK, "{:?}", d0.corpo);
    assert_eq!(d0.corpo["completo"], true);
    assert_eq!(d0.corpo["linhas"]["linhas"].as_array().unwrap().len(), 3);
    let cursor0 = d0.corpo["cursor"].as_str().unwrap().to_owned();

    // Sem mudança: vazio, e o cursor não anda.
    let d1 = c
        .amb
        .get(
            &format!("/api/v2/livro/movimentacoes?desde={cursor0}"),
            &c.cred,
        )
        .await;
    assert_eq!(d1.corpo["completo"], false);
    assert_eq!(d1.corpo["linhas"]["linhas"], json!([]));
    assert_eq!(d1.corpo["excluidas"], json!([]));
    assert_eq!(d1.corpo["cursor"], json!(cursor0));
    let d1b = c
        .amb
        .get(
            &format!("/api/v2/livro/movimentacoes?desde={cursor0}"),
            &c.cred,
        )
        .await;
    assert_eq!(d1b.corpo, d1.corpo);

    // Muda uma só (pelo SQL, como o pipeline faria): aparece só ela.
    c.amb
        .sql(&format!(
            "UPDATE public.transactions SET description = 'Mudou fora' WHERE id = '{}'",
            c.s.movimentacao_dez
        ))
        .await;
    let d2 = c
        .amb
        .get(
            &format!("/api/v2/livro/movimentacoes?desde={cursor0}"),
            &c.cred,
        )
        .await;
    let linhas = d2.corpo["linhas"]["linhas"].as_array().unwrap();
    assert_eq!(linhas.len(), 1);
    assert_eq!(linhas[0]["id"], json!(c.s.movimentacao_dez));
    assert_eq!(linhas[0]["descricao"], "Mudou fora");
    let cursor2 = d2.corpo["cursor"].as_str().unwrap().to_owned();
    assert_ne!(cursor2, cursor0);
    let d3 = c
        .amb
        .get(
            &format!("/api/v2/livro/movimentacoes?desde={cursor2}"),
            &c.cred,
        )
        .await;
    assert_eq!(d3.corpo["linhas"]["linhas"], json!([]));

    // Mudar o rateio por fora muda o delta de rateios (a versão é a da movimentação).
    let dr0 = c.amb.get("/api/v2/livro/rateios", &c.cred).await;
    let cr = dr0.corpo["cursor"].as_str().unwrap().to_owned();
    c.amb
        .sql(&format!(
            "UPDATE public.transaction_allocations SET note = 'nota nova' WHERE id = '{}'",
            c.s.rateio
        ))
        .await;
    let dr1 = c
        .amb
        .get(&format!("/api/v2/livro/rateios?desde={cr}"), &c.cred)
        .await;
    assert_eq!(dr1.corpo["linhas"]["linhas"].as_array().unwrap().len(), 1);
    assert_eq!(dr1.corpo["linhas"]["linhas"][0]["nota"], "nota nova");

    // Cursor lixo é pedido inválido; outra coleção, delta próprio.
    let r = c
        .amb
        .get("/api/v2/livro/movimentacoes?desde=lixo!", &c.cred)
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let outras = c.amb.get("/api/v2/livro/categorias", &c.cred).await;
    assert_eq!(
        outras.corpo["linhas"]["linhas"].as_array().unwrap().len(),
        2
    );
}

// ───────────────────────────── CX-07: documentos ─────────────────────────────

async fn enviar_doc(c: &Cena, sha: &str, nome: &str, bytes: &[u8]) -> Resposta {
    let (tipo, corpo) = multipart(
        &[
            ("sha256", sha),
            ("nome_original", nome),
            ("id_no_inventario", "doc-7"),
        ],
        Some((nome, "application/pdf", bytes)),
    );
    c.amb
        .pedir_bruto("/api/v2/documentos", &c.cred, &tipo, corpo)
        .await
}

#[tokio::test]
async fn cx07_envio_de_documento_e_idempotente_pelo_sha256() {
    let Some(c) = cena().await else { return };
    let bytes = b"%PDF-1.4 conteudo ficticio do boleto exemplo".to_vec();
    let sha = hex(&bytes);
    let enviar = enviar_doc;
    let r1 = enviar(&c, &sha, "boleto-exemplo.pdf", &bytes).await;
    assert_eq!(r1.status, StatusCode::OK, "{:?}", r1.corpo);
    assert_eq!(r1.corpo["ja_existia"], false);
    assert_eq!(r1.corpo["sha256"], json!(sha));
    assert_eq!(r1.corpo["id_no_inventario"], "doc-7");
    let doc: Uuid = r1.corpo["source_document_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let job: Uuid = r1.corpo["job_id"].as_str().unwrap().parse().unwrap();

    // O que o worker Bun espera.
    let c0 = c.amb.banco.get().await.unwrap();
    let d = c0
        .query_one(
            "SELECT origin_type, filename, mime_type, file_size_bytes, status, content_hash, storage_path \
             FROM public.source_documents WHERE id = $1 AND user_id = $2",
            &[&doc, &c.dono],
        )
        .await
        .unwrap();
    assert_eq!(d.get::<_, String>("origin_type"), "manual_upload");
    assert_eq!(d.get::<_, String>("filename"), "boleto-exemplo.pdf");
    assert_eq!(d.get::<_, String>("mime_type"), "application/pdf");
    assert_eq!(d.get::<_, i64>("file_size_bytes"), bytes.len() as i64);
    assert_eq!(d.get::<_, String>("content_hash"), sha);
    let caminho: String = d.get("storage_path");
    assert!(
        caminho.starts_with(&format!("{}/", c.dono)) && caminho.ends_with("/boleto-exemplo.pdf")
    );
    let j = c0
        .query_one(
            "SELECT status, requires, source_document_id, user_id FROM public.ingestion_jobs WHERE id = $1",
            &[&job],
        )
        .await
        .unwrap();
    assert_eq!(j.get::<_, String>("status"), "discovered");
    assert_eq!(j.get::<_, Vec<String>>("requires"), vec!["ocr".to_owned()]);
    assert_eq!(j.get::<_, Uuid>("source_document_id"), doc);
    assert_eq!(j.get::<_, Uuid>("user_id"), c.dono);
    // Os bytes foram para o armazenamento, na chave do banco.
    let (guardado, tipo) = c.amb.armazenamento.arquivo(&caminho).expect("gravado");
    assert_eq!(guardado, bytes);
    assert_eq!(tipo, "application/pdf");

    // Reenviar: mesmos ids, nada novo, nenhuma gravação nova.
    let r2 = enviar(&c, &sha, "boleto-exemplo.pdf", &bytes).await;
    assert_eq!(r2.corpo["ja_existia"], true);
    assert_eq!(
        r2.corpo["source_document_id"],
        r1.corpo["source_document_id"]
    );
    assert_eq!(r2.corpo["job_id"], r1.corpo["job_id"]);
    assert_eq!(c.amb.armazenamento.gravacoes(), 1);
    assert_eq!(
        contar(&c.amb, "SELECT count(*) FROM public.source_documents WHERE user_id = $1 AND content_hash IS NOT NULL AND origin_type = 'manual_upload'", c.dono).await,
        1
    );
    // Duas chamadas ao mesmo tempo com conteúdo novo também criam um só.
    let novo = b"%PDF-1.4 outro conteudo ficticio".to_vec();
    let sha_novo = hex(&novo);
    let (a, b) = tokio::join!(
        enviar(&c, &sha_novo, "outro.pdf", &novo),
        enviar(&c, &sha_novo, "outro.pdf", &novo)
    );
    assert_eq!(a.corpo["source_document_id"], b.corpo["source_document_id"]);
    assert_eq!(c.amb.armazenamento.gravacoes(), 2);
}

#[tokio::test]
async fn documento_recusa_sha_errado_formato_estranho_e_armazenamento_fora_do_ar() {
    let Some(c) = cena().await else { return };
    let bytes = b"conteudo".to_vec();
    let mandar = |sha: String, nome: &'static str, bytes: Vec<u8>| {
        let (tipo, corpo) = multipart(
            &[("sha256", sha.as_str()), ("nome_original", nome)],
            Some((nome, "application/octet-stream", &bytes)),
        );
        let amb = &c.amb;
        let cred = c.cred.clone();
        async move {
            amb.pedir_bruto("/api/v2/documentos", &cred, &tipo, corpo)
                .await
        }
    };
    // SHA que não é o dos bytes.
    let r = mandar(hex(b"outra coisa"), "a.pdf", bytes.clone()).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{:?}", r.corpo);
    // Formato que o pipeline não abre.
    let r = mandar(hex(&bytes), "programa.exe", bytes.clone()).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Sem o arquivo.
    let (tipo, corpo) = multipart(&[("sha256", "a"), ("nome_original", "a.pdf")], None);
    let r = c
        .amb
        .pedir_bruto("/api/v2/documentos", &c.cred, &tipo, corpo)
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Na v1 o recurso não existe.
    let (tipo, corpo) = multipart(
        &[("sha256", &hex(&bytes)), ("nome_original", "a.csv")],
        Some(("a.csv", "text/csv", &bytes)),
    );
    let r = c
        .amb
        .pedir_bruto("/api/v1/documentos", &c.cred, &tipo, corpo)
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Armazenamento fora do ar: 502, nada fica no banco, e depois funciona.
    c.amb.armazenamento.falhar(true);
    let r = mandar(hex(&bytes), "a.csv", bytes.clone()).await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY, "{:?}", r.corpo);
    assert_eq!(
        contar(&c.amb, "SELECT count(*) FROM public.source_documents WHERE user_id = $1 AND content_hash IS NOT NULL", c.dono).await,
        0
    );
    c.amb.armazenamento.falhar(false);
    let r = mandar(hex(&bytes), "a.csv", bytes.clone()).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    assert_eq!(r.corpo["ja_existia"], false);
}

// ───────────────────────────── CX-09: contrato v1 ─────────────────────────────

#[tokio::test]
async fn cx09_contrato_v1_continua_aceito_sem_as_novidades_da_v2() {
    let Some(c) = cena().await else { return };
    // O estado fala as duas versões.
    let r = c.amb.get("/api/v1/estado", &c.cred).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.corpo["contratos_aceitos"], json!([1, 2]));
    // As coleções velhas seguem inteiras, com ETag, na v1.
    let r = c.amb.get("/api/v1/livro/movimentacoes", &c.cred).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.etag.is_some());
    assert_eq!(r.corpo["colecao"], "movimentacoes");
    assert_eq!(r.corpo["linhas"].as_array().unwrap().len(), 3);
    // `?desde=` na v1 é ignorado: a coleção continua inteira.
    let r = c
        .amb
        .get("/api/v1/livro/movimentacoes?desde=x", &c.cred)
        .await;
    assert_eq!(r.corpo["linhas"].as_array().unwrap().len(), 3);
    // As coleções novas não existem na v1.
    let r = c.amb.get("/api/v1/livro/rascunhos", &c.cred).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Ação nova na v1: ainda não aceita.
    let r = c
        .amb
        .post(
            "/api/v1/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "aprovar_rascunho", "rascunho": Uuid::new_v4()}),
                ("rascunhos", Uuid::new_v4()),
                &[],
            ),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.corpo["codigo"], "acao_ainda_nao_aceita");
    // A única Ação da v1 segue de pé, também pela v2.
    let base = versao_de(&c.amb, &c.cred, "movimentacoes", c.s.movimentacao).await;
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "mudar_categoria_da_movimentacao", "movimentacao": c.s.movimentacao, "categoria": c.s.outra_categoria}),
                ("movimentacoes", c.s.movimentacao),
                &[("movimentacoes", c.s.movimentacao, &base)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
}

#[tokio::test]
async fn rascunho_de_outro_tipo_nao_e_lancado_pelo_desktop() {
    let Some(c) = cena().await else { return };
    let id = Uuid::new_v4();
    c.amb
        .sql(&format!(
            "INSERT INTO public.draft_records (id, user_id, draft_type, status, draft_data, draft_schema_version) \
             VALUES ('{id}', '{}', 'liability', 'pending_review', '{{\"schema_version\":1,\"draft_type\":\"liability\"}}'::jsonb, 1)",
            c.dono
        ))
        .await;
    let base = versao_de(&c.amb, &c.cred, "rascunhos", id).await;
    let r = c
        .amb
        .post(
            "/api/v2/alteracoes",
            &c.cred,
            proposta(
                json!({"tipo": "aprovar_rascunho", "rascunho": id}),
                ("rascunhos", id),
                &[("rascunhos", id, &base)],
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "recusada");
    assert_eq!(r.corpo["motivo"], "regra_violada");
    assert!(r.corpo["mensagem"].as_str().unwrap().contains("web"));
}
