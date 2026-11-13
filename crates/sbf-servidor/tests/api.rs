//! A API de ponta a ponta contra o Postgres de teste (Docker, dados fictícios).
//! Só roda com `SBF_PG_TESTE=1`: `scripts/desktop/postgres-de-teste.sh testar`.

mod comum;

use axum::http::StatusCode;
use comum::*;
use sbf_contrato::{Colecao, LinhasDaColecao, RespostaDaAlteracao};
use serde_json::{Value, json};

fn proposta(id: uuid::Uuid, mov: uuid::Uuid, categoria: Option<uuid::Uuid>, versao: &str) -> Value {
    json!({
        "id": id,
        "acao": { "tipo": "mudar_categoria_da_movimentacao", "movimentacao": mov, "categoria": categoria },
        "item": { "colecao": "movimentacoes", "id": mov },
        "versoes_de_base": [ { "colecao": "movimentacoes", "id": mov, "versao": versao } ],
        "feita_em": "2026-11-13T21:00:00Z"
    })
}

fn movimentacao(corpo: &Value, id: uuid::Uuid) -> &Value {
    corpo["linhas"]
        .as_array()
        .expect("linhas")
        .iter()
        .find(|l| l["id"] == json!(id))
        .unwrap_or_else(|| panic!("movimentação {id} não veio"))
}

#[tokio::test]
async fn pareamento_emite_credencial_que_so_existe_como_hash() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, sub) = novo_dono(&amb).await;
    let (estacao, credencial) = amb.parear(&email, &sub, "Notebook de teste").await;
    assert_eq!(estacao["nome"], "Notebook de teste");
    assert_eq!(estacao["revogada"], false);
    assert!(credencial.starts_with("sbfe_"));

    // O banco não tem o texto da credencial nem o segredo dela em lugar nenhum.
    let segredo = credencial.rsplit('_').next().unwrap();
    let c = amb.banco.get().await.unwrap();
    let linha = c
        .query_one(
            "SELECT row_to_json(e)::text AS t, user_id, octet_length(credencial_sha256) AS n \
             FROM public.estacoes e WHERE id = $1::text::uuid",
            &[&estacao["id"].as_str().unwrap()],
        )
        .await
        .unwrap();
    let texto: String = linha.get("t");
    assert!(!texto.contains(segredo) && !texto.contains(&credencial));
    assert_eq!(linha.get::<_, uuid::Uuid>("user_id"), dono);
    assert_eq!(linha.get::<_, i32>("n"), 32);

    // O google_sub ficou vinculado, como no web.
    let sub_gravado: Option<String> = c
        .query_one(
            "SELECT google_sub FROM public.users WHERE id = $1",
            &[&dono],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(sub_gravado.as_deref(), Some(sub.as_str()));

    // E a credencial funciona.
    let r = amb.get("/api/v1/estado", &credencial).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    assert_eq!(r.corpo["estacao"]["id"], estacao["id"]);
    assert!(r.corpo["estacao"]["ultimo_contato"].is_string());
}

#[tokio::test]
async fn pareamento_recusa_email_desconhecido_e_conta_google_trocada() {
    let Some(amb) = ambiente().await else { return };
    let corpo = |email: &str, sub: &str| {
        json!({
            "id_token": id_token(email, sub, "n"),
            "nonce": "n", "nome": "Estação", "sistema": "linux"
        })
    };
    let r = amb
        .pedir(
            "POST",
            "/api/v1/estacoes/pareamento",
            None,
            &[],
            Some(corpo("ninguem-cadastrado@example.com", "sub-x")),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.corpo["codigo"], "pareamento_recusado");

    // Vinculado a um sub, outro sub com o mesmo e-mail é recusado.
    let (_dono, email, sub) = novo_dono(&amb).await;
    amb.parear(&email, &sub, "Primeira").await;
    let r = amb
        .pedir(
            "POST",
            "/api/v1/estacoes/pareamento",
            None,
            &[],
            Some(corpo(&email, "sub-de-outra-conta")),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.corpo["codigo"], "pareamento_recusado");

    // Nonce que não confere.
    let r = amb
        .pedir(
            "POST",
            "/api/v1/estacoes/pareamento",
            None,
            &[],
            Some(json!({
                "id_token": id_token(&email, &sub, "n1"),
                "nonce": "n2", "nome": "Estação", "sistema": "linux"
            })),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_pareia_para_os_dados_do_dono() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email_dono, sub_dono) = novo_dono(&amb).await;
    let semeado = semear(&amb, dono).await;
    let admin = uuid::Uuid::new_v4();
    let email_admin = format!("admin-{}@example.com", admin.simple());
    amb.sql(&format!(
        "INSERT INTO public.users (id, email, role, owner_id, password_hash) \
         VALUES ('{admin}', '{email_admin}', 'admin', '{dono}', 'x')"
    ))
    .await;
    let (_e, credencial) = amb
        .parear(&email_admin, "sub-admin", "Estação do admin")
        .await;
    let r = amb.get("/api/v1/livro/movimentacoes", &credencial).await;
    assert_eq!(r.status, StatusCode::OK);
    movimentacao(&r.corpo, semeado.movimentacao);
    // E a estação aparece na lista do dono.
    let (_e2, cred_dono) = amb.parear(&email_dono, &sub_dono, "Estação do dono").await;
    let r = amb.get("/api/v1/estacoes", &cred_dono).await;
    assert_eq!(r.corpo["estacoes"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn dono_isolado_em_todas_as_colecoes() {
    let Some(amb) = ambiente().await else { return };
    let (dono_a, email_a, sub_a) = novo_dono(&amb).await;
    let (dono_b, _email_b, _sub_b) = novo_dono(&amb).await;
    let a = semear(&amb, dono_a).await;
    let b = semear(&amb, dono_b).await;
    let (_e, credencial) = amb.parear(&email_a, &sub_a, "Estação A").await;

    for colecao in Colecao::TODAS {
        let r = amb
            .get(&format!("/api/v1/livro/{}", colecao.caminho()), &credencial)
            .await;
        assert_eq!(r.status, StatusCode::OK, "{colecao:?}: {:?}", r.corpo);
        // O corpo segue o contrato.
        let linhas: LinhasDaColecao = serde_json::from_slice(&r.bruto)
            .unwrap_or_else(|e| panic!("{colecao:?} fora do contrato: {e}"));
        assert_eq!(linhas.colecao(), colecao);
        assert!(linhas.quantas() >= 1, "{colecao:?} veio vazia");
        let texto = String::from_utf8(r.bruto.clone()).unwrap();
        for id in &b.ids {
            assert!(
                !texto.contains(&id.to_string()),
                "{colecao:?} vazou linha do outro dono"
            );
        }
        assert!(!texto.contains(&dono_b.to_string()));
        // Pelo menos um id do dono A em cada coleção.
        assert!(
            a.ids.iter().any(|id| texto.contains(&id.to_string())),
            "{colecao:?}"
        );
    }
}

#[tokio::test]
async fn dinheiro_chega_em_centavos_exatos() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, sub) = novo_dono(&amb).await;
    let s = semear(&amb, dono).await;
    let (_e, credencial) = amb.parear(&email, &sub, "Estação").await;

    let r = amb.get("/api/v1/livro/movimentacoes", &credencial).await;
    let dez = movimentacao(&r.corpo, s.movimentacao_dez);
    let vinte = movimentacao(&r.corpo, s.movimentacao_vinte);
    assert_eq!(dez["valor"], json!(10));
    assert_eq!(vinte["valor"], json!(20));
    assert_eq!(
        dez["valor"].as_i64().unwrap() + vinte["valor"].as_i64().unwrap(),
        30
    );
    assert_eq!(vinte["tipo"], "estorno");
    assert_eq!(
        movimentacao(&r.corpo, s.movimentacao)["valor"],
        json!(15000)
    );
    assert_eq!(
        movimentacao(&r.corpo, s.movimentacao)["tags"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let r = amb.get("/api/v1/livro/contas", &credencial).await;
    let saldos: Vec<Option<i64>> = r.corpo["linhas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["saldo_atual"].as_i64())
        .collect();
    assert!(saldos.contains(&Some(123_456)));
    assert!(saldos.contains(&Some(-999_999_999_999_999)));

    let r = amb.get("/api/v1/livro/dividas", &credencial).await;
    assert_eq!(r.corpo["linhas"][0]["saldo_devedor"], json!(750_050));
    assert_eq!(r.corpo["linhas"][0]["taxa_de_juros"], "0.012500");
    assert_eq!(r.corpo["linhas"][0]["sistema_de_amortizacao"], "price");

    let r = amb.get("/api/v1/livro/parcelas", &credencial).await;
    assert_eq!(r.corpo["linhas"][0]["juros"], json!(8033));
    let r = amb.get("/api/v1/livro/rateios", &credencial).await;
    assert_eq!(r.corpo["linhas"][0]["valor"], json!(10_000));
}

#[tokio::test]
async fn etag_da_colecao_inteira_e_304() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, sub) = novo_dono(&amb).await;
    let s = semear(&amb, dono).await;
    let (_e, credencial) = amb.parear(&email, &sub, "Estação").await;

    let r1 = amb.get("/api/v1/livro/movimentacoes", &credencial).await;
    let etag = r1.etag.clone().expect("etag");
    let r2 = amb
        .pedir(
            "GET",
            "/api/v1/livro/movimentacoes",
            Some(&credencial),
            &[("if-none-match", &etag)],
            None,
        )
        .await;
    assert_eq!(r2.status, StatusCode::NOT_MODIFIED);
    assert!(r2.bruto.is_empty());
    assert_eq!(r2.etag.as_deref(), Some(etag.as_str()));

    // Uma tag nova numa movimentação (tabela sem updated_at) também muda o ETag.
    amb.sql(&format!(
        "INSERT INTO public.tags (id, user_id, name) VALUES ('{t}', '{dono}', 'nova');
         INSERT INTO public.transaction_tags (transaction_id, tag_id) VALUES ('{m}', '{t}');",
        t = uuid::Uuid::new_v4(),
        m = s.movimentacao_dez
    ))
    .await;
    let r3 = amb
        .pedir(
            "GET",
            "/api/v1/livro/movimentacoes",
            Some(&credencial),
            &[("if-none-match", &etag)],
            None,
        )
        .await;
    assert_eq!(r3.status, StatusCode::OK);
    assert_ne!(r3.etag.as_deref(), Some(etag.as_str()));

    // O ETag de outra coleção não serve para esta.
    let contas = amb.get("/api/v1/livro/contas", &credencial).await;
    assert_ne!(contas.etag, r3.etag);
    let r = amb.get("/api/v1/livro/nao-existe", &credencial).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.corpo["codigo"], "nao_encontrado");
}

#[tokio::test]
async fn revogada_recebe_estacao_revogada_e_as_outras_continuam() {
    let Some(amb) = ambiente().await else { return };
    let (_dono, email, sub) = novo_dono(&amb).await;
    let (e1, cred1) = amb.parear(&email, &sub, "Notebook").await;
    let (e2, cred2) = amb.parear(&email, &sub, "Desktop").await;

    let r = amb.get("/api/v1/estacoes", &cred1).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.corpo["estacoes"].as_array().unwrap().len(), 2);
    assert_eq!(r.corpo["esta"], e1["id"]);

    let r = amb
        .post(
            &format!("/api/v1/estacoes/{}/revogar", e2["id"].as_str().unwrap()),
            &cred1,
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    assert_eq!(r.corpo["revogada"], true);
    assert!(r.corpo["revogada_em"].is_string());

    // A revogada: 401 com o código, em qualquer recurso.
    for caminho in ["/api/v1/estado", "/api/v1/livro/contas", "/api/v1/estacoes"] {
        let r = amb.get(caminho, &cred2).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.corpo["codigo"], "estacao_revogada");
    }
    // O hash foi apagado.
    let c = amb.banco.get().await.unwrap();
    let hash: Option<Vec<u8>> = c
        .query_one(
            "SELECT credencial_sha256 FROM public.estacoes WHERE id = $1::text::uuid",
            &[&e2["id"].as_str().unwrap()],
        )
        .await
        .unwrap()
        .get(0);
    assert!(hash.is_none());

    // A outra continua, e vê a revogada na lista.
    let r = amb.get("/api/v1/estacoes", &cred1).await;
    assert_eq!(r.status, StatusCode::OK);
    let revogadas: Vec<&Value> = r.corpo["estacoes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["revogada"] == true)
        .collect();
    assert_eq!(revogadas.len(), 1);

    // Revogar de novo devolve a mesma estação; revogar estação de outro dono é 404.
    let r = amb
        .post(
            &format!("/api/v1/estacoes/{}/revogar", e2["id"].as_str().unwrap()),
            &cred1,
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let (_d, email_b, sub_b) = novo_dono(&amb).await;
    let (eb, _cb) = amb.parear(&email_b, &sub_b, "Do outro").await;
    let r = amb
        .post(
            &format!("/api/v1/estacoes/{}/revogar", eb["id"].as_str().unwrap()),
            &cred1,
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn credencial_desconhecida_ou_adulterada_e_401_sem_detalhe() {
    let Some(amb) = ambiente().await else { return };
    let (_dono, email, sub) = novo_dono(&amb).await;
    let (_e, cred) = amb.parear(&email, &sub, "Estação").await;
    let mut adulterada = cred.clone();
    let ultimo = adulterada.pop().unwrap();
    adulterada.push(if ultimo == 'A' { 'B' } else { 'A' });
    for c in [
        Some("sbfe_naoexiste"),
        Some(adulterada.as_str()),
        Some(&*format!(
            "sbfe_{}_{}",
            uuid::Uuid::new_v4().simple(),
            "A".repeat(43)
        )),
        None,
    ] {
        let r = amb.pedir("GET", "/api/v1/estado", c, &[], None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.corpo["codigo"], "nao_autenticado");
    }
}

#[tokio::test]
async fn contrato_antigo_e_mais_novo_sao_recusados_antes_da_credencial() {
    let Some(amb) = ambiente().await else { return };
    let (_dono, email, sub) = novo_dono(&amb).await;
    let (_e, cred) = amb.parear(&email, &sub, "Estação").await;
    let r = amb.get("/api/v0/estado", &cred).await;
    assert_eq!(r.status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(r.corpo["codigo"], "contrato_antigo");
    assert_eq!(r.corpo["contratos_aceitos"], json!([1]));
    let r = amb.get("/api/v2/livro/contas", &cred).await;
    assert_eq!(r.status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(r.corpo["codigo"], "contrato_mais_novo");
    // Mesmo sem credencial, o contrato vem antes.
    let r = amb.pedir("GET", "/api/v0/estado", None, &[], None).await;
    assert_eq!(r.corpo["codigo"], "contrato_antigo");
}

#[tokio::test]
async fn estado_traz_dia_de_corte_padrao_20_ou_o_do_dono() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, sub) = novo_dono(&amb).await;
    let (_e, cred) = amb.parear(&email, &sub, "Estação").await;
    let r = amb.get("/api/v1/estado", &cred).await;
    assert_eq!(r.corpo["dia_de_corte"], 20);
    assert_eq!(r.corpo["contrato"], 1);
    assert_eq!(r.corpo["regras"], "0.1.0-teste");
    assert_eq!(r.corpo["fuso"], "America/Sao_Paulo");
    assert!(r.corpo["hora_do_servidor"].as_str().unwrap().ends_with('Z'));
    let _: sbf_contrato::Estado = serde_json::from_slice(&r.bruto).unwrap();
    amb.sql(&format!(
        "INSERT INTO public.user_financial_preferences (user_id, financial_cycle_start_day) VALUES ('{dono}', 5)"
    ))
    .await;
    let r = amb.get("/api/v1/estado", &cred).await;
    assert_eq!(r.corpo["dia_de_corte"], 5);
}

#[tokio::test]
async fn estado_conta_os_documentos_a_revisar_so_do_dono() {
    let Some(amb) = ambiente().await else { return };
    let (dono_a, email_a, sub_a) = novo_dono(&amb).await;
    let (dono_b, _eb, _sb) = novo_dono(&amb).await;
    let (_e, cred) = amb.parear(&email_a, &sub_a, "Estação").await;
    let r = amb.get("/api/v1/estado", &cred).await;
    assert_eq!(r.corpo["documentos_a_revisar"], 0);
    let run = uuid::Uuid::new_v4();
    let mut sql = format!(
        "INSERT INTO public.ingestion_runs (id, user_id, source_type) VALUES ('{run}', '{dono_a}', 'manual_upload');"
    );
    for (dono, status) in [
        (dono_a, "pending_review"),
        (dono_a, "drafted"),
        (dono_a, "drafted"),
        (dono_a, "posted"),
        (dono_a, "failed"),
        (dono_b, "pending_review"),
        (dono_b, "drafted"),
    ] {
        sql.push_str(&format!(
            "INSERT INTO public.ingestion_jobs (run_id, user_id, status) VALUES ('{run}', '{dono}', '{status}');"
        ));
    }
    amb.sql(&sql).await;
    let r = amb.get("/api/v1/estado", &cred).await;
    assert_eq!(r.corpo["documentos_a_revisar"], 3);
}

#[tokio::test]
async fn mudar_categoria_aceita_e_recusada_lado_a_lado() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, sub) = novo_dono(&amb).await;
    let s = semear(&amb, dono).await;
    let (_e1, cred1) = amb.parear(&email, &sub, "Notebook").await;
    let (_e2, cred2) = amb.parear(&email, &sub, "Desktop").await;

    // As duas estações têm a mesma cópia de base.
    let livro = amb.get("/api/v1/livro/movimentacoes", &cred1).await;
    let etag_antes = livro.etag.clone().unwrap();
    let base = movimentacao(&livro.corpo, s.movimentacao)["versao"]
        .as_str()
        .unwrap()
        .to_owned();

    // Estação 1 muda para "Casa": aceita.
    let id1 = uuid::Uuid::new_v4();
    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred1,
            proposta(id1, s.movimentacao, Some(s.outra_categoria), &base),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);
    let aceita: RespostaDaAlteracao = serde_json::from_slice(&r.bruto).unwrap();
    let RespostaDaAlteracao::Aceita { id, linhas } = &aceita else {
        panic!("deveria ser aceita: {:?}", r.corpo)
    };
    assert_eq!(*id, id1);
    assert_eq!(
        r.corpo["linhas"][0]["linha"]["categoria"],
        json!(s.outra_categoria)
    );
    let nova = r.corpo["linhas"][0]["linha"]["versao"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(nova, base);
    assert_eq!(linhas.len(), 1);

    // O livro mostra a categoria nova, a versão nova e muda o ETag.
    let depois = amb.get("/api/v1/livro/movimentacoes", &cred2).await;
    assert_ne!(depois.etag.unwrap(), etag_antes);
    let m = movimentacao(&depois.corpo, s.movimentacao);
    assert_eq!(m["categoria"], json!(s.outra_categoria));
    assert_eq!(m["versao"], json!(nova));

    // Estação 2, com a base velha, tira a categoria: recusada, lado a lado.
    let id2 = uuid::Uuid::new_v4();
    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred2,
            proposta(id2, s.movimentacao, None, &base),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.corpo["resultado"], "recusada");
    assert_eq!(r.corpo["motivo"], "versao_de_base_divergente");
    assert_eq!(r.corpo["sua_versao"]["acao"]["categoria"], Value::Null);
    assert_eq!(
        r.corpo["sua_versao"]["versoes_de_base"][0]["versao"],
        json!(base)
    );
    assert_eq!(r.corpo["no_servidor_agora"][0]["colecao"], "movimentacoes");
    assert_eq!(
        r.corpo["no_servidor_agora"][0]["linha"]["categoria"],
        json!(s.outra_categoria)
    );
    assert_eq!(
        r.corpo["no_servidor_agora"][0]["linha"]["versao"],
        json!(nova)
    );
    // Nada mudou com a recusa.
    let m = amb.get("/api/v1/livro/movimentacoes", &cred2).await;
    assert_eq!(
        movimentacao(&m.corpo, s.movimentacao)["categoria"],
        json!(s.outra_categoria)
    );

    // Refazer sobre a nova: aceita.
    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred2,
            proposta(uuid::Uuid::new_v4(), s.movimentacao, None, &nova),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "aceita", "{:?}", r.corpo);
    assert_eq!(r.corpo["linhas"][0]["linha"]["categoria"], Value::Null);

    // Reenviar a primeira (mesmo id) devolve a mesma resposta, não uma recusa.
    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred1,
            proposta(id1, s.movimentacao, Some(s.outra_categoria), &base),
        )
        .await;
    let repetida: RespostaDaAlteracao = serde_json::from_slice(&r.bruto).unwrap();
    assert_eq!(repetida, aceita);

    // As três ficaram registradas.
    let c = amb.banco.get().await.unwrap();
    let n: i64 = c
        .query_one(
            "SELECT count(*) FROM public.alteracoes WHERE user_id = $1",
            &[&dono],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 3);
}

#[tokio::test]
async fn alteracao_nao_atravessa_o_dono() {
    let Some(amb) = ambiente().await else { return };
    let (dono_a, email_a, sub_a) = novo_dono(&amb).await;
    let (dono_b, _eb, _sb) = novo_dono(&amb).await;
    let a = semear(&amb, dono_a).await;
    let b = semear(&amb, dono_b).await;
    let (_e, cred) = amb.parear(&email_a, &sub_a, "Estação A").await;
    let livro = amb.get("/api/v1/livro/movimentacoes", &cred).await;
    let base = movimentacao(&livro.corpo, a.movimentacao)["versao"]
        .as_str()
        .unwrap()
        .to_owned();

    // Categoria do outro dono: recusada, nada muda.
    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred,
            proposta(
                uuid::Uuid::new_v4(),
                a.movimentacao,
                Some(b.categoria),
                &base,
            ),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "recusada");
    assert_eq!(r.corpo["motivo"], "referencia_nao_encontrada");

    // Movimentação do outro dono: como se não existisse.
    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred,
            proposta(uuid::Uuid::new_v4(), b.movimentacao, None, &base),
        )
        .await;
    assert_eq!(r.corpo["resultado"], "recusada");
    assert_eq!(r.corpo["motivo"], "item_nao_encontrado");
    assert_eq!(r.corpo["no_servidor_agora"], json!([]));
    let c = amb.banco.get().await.unwrap();
    let cat: Option<uuid::Uuid> = c
        .query_one(
            "SELECT category_id FROM public.transactions WHERE id = $1",
            &[&b.movimentacao],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(cat, Some(b.categoria));
}

#[tokio::test]
async fn alteracao_fora_do_contrato() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, sub) = novo_dono(&amb).await;
    let s = semear(&amb, dono).await;
    let (_e, cred) = amb.parear(&email, &sub, "Estação").await;

    let r = amb
        .post(
            "/api/v1/alteracoes",
            &cred,
            json!({ "id": uuid::Uuid::new_v4(), "acao": { "tipo": "apagar_movimentacao" } }),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.corpo["codigo"], "acao_ainda_nao_aceita");

    // Sem versão de base: pedido inválido.
    let mut p = proposta(uuid::Uuid::new_v4(), s.movimentacao, None, "1");
    p["versoes_de_base"] = json!([]);
    let r = amb.post("/api/v1/alteracoes", &cred, p).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.corpo["codigo"], "pedido_invalido");

    // Item que não é a movimentação da ação.
    let mut p = proposta(uuid::Uuid::new_v4(), s.movimentacao, None, "1");
    p["item"]["id"] = json!(s.movimentacao_dez);
    let r = amb.post("/api/v1/alteracoes", &cred, p).await;
    assert_eq!(r.corpo["codigo"], "pedido_invalido");

    let r = amb
        .pedir("POST", "/api/v1/alteracoes", Some(&cred), &[], None)
        .await;
    assert_eq!(r.corpo["codigo"], "pedido_invalido");
}

#[tokio::test]
async fn emitir_credencial_cria_estacao_que_autentica_e_so_guarda_hash() {
    let Some(amb) = ambiente().await else { return };
    let (dono, email, _sub) = novo_dono(&amb).await;
    let mut c = amb.banco.get().await.unwrap();
    let (estacao, credencial) =
        sbf_servidor::estacoes::emitir_credencial(&mut c, &email.to_uppercase(), "Casa", "linux")
            .await
            .expect("emitir");
    assert!(credencial.starts_with("sbfe_"));
    assert_eq!(estacao.nome, "Casa");
    let linha = c
        .query_one(
            "SELECT row_to_json(e)::text AS t, user_id, octet_length(credencial_sha256) AS n \
             FROM public.estacoes e WHERE id = $1",
            &[&estacao.id],
        )
        .await
        .unwrap();
    let texto: String = linha.get("t");
    assert!(!texto.contains(credencial.rsplit('_').next().unwrap()));
    assert_eq!(linha.get::<_, uuid::Uuid>("user_id"), dono);
    assert_eq!(linha.get::<_, i32>("n"), 32);
    // A credencial emitida autentica na API como a do pareamento.
    let r = amb.get("/api/v1/estado", &credencial).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.corpo);

    // E-mail sem cadastro e nome vazio são recusados, sem criar nada.
    assert!(
        sbf_servidor::estacoes::emitir_credencial(&mut c, "ninguem@example.com", "X", "linux")
            .await
            .is_err()
    );
    assert!(
        sbf_servidor::estacoes::emitir_credencial(&mut c, &email, "  ", "linux")
            .await
            .is_err()
    );
}
