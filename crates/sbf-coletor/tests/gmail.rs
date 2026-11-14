//! O coletor do Gmail contra um servidor falso local (respostas gravadas à mão) e, com
//! `SBF_PG_TESTE=1`, contra o Postgres de teste.

mod comum;

use comum::{Falso, Pedido, arquivo, banco, novo_dono, subir};
use sbf_coletor::config::{ConfigGmail, Segredo};
use sbf_coletor::gmail::cliente::ClienteGmail;
use sbf_coletor::gmail::varredura::{OpcoesDaVarredura, varrer};
use sbf_servidor::armazenamento::ArmazenamentoFalso;

const SEGREDO: &str = "segredo-do-cliente-ficticio";
const REFRESH: &str = "refresh-ficticio-nao-vazar";

fn roteiro_do_gmail(p: &Pedido) -> (u16, String) {
    match (p.metodo.as_str(), p.caminho.as_str()) {
        ("POST", "/token") => {
            if p.corpo.contains(&format!("refresh_token={REFRESH}")) {
                (200, arquivo("gmail/token.json"))
            } else {
                (400, arquivo("gmail/token_revogado.json"))
            }
        }
        ("GET", "/labels") => (200, arquivo("gmail/rotulos.json")),
        ("GET", "/messages") => match p.parametro("pageToken").as_deref() {
            None => (200, arquivo("gmail/lista_p1.json")),
            Some("pagina-2") => (200, arquivo("gmail/lista_p2.json")),
            Some(_) => (400, "{}".into()),
        },
        ("GET", c) if c.starts_with("/messages/") && c.contains("/attachments/") => {
            let id = c.rsplit('/').next().unwrap_or("");
            (200, arquivo(&format!("gmail/anexo_{id}.json")))
        }
        ("GET", c) if c.starts_with("/messages/") => {
            let id = c.trim_start_matches("/messages/");
            (200, arquivo(&format!("gmail/mensagem_{id}.json")))
        }
        _ => (404, "{}".into()),
    }
}

async fn gmail_falso() -> (Falso, ClienteGmail) {
    let falso = subir(roteiro_do_gmail).await;
    let mut config = ConfigGmail::com_enderecos_padrao(
        "cliente-ficticio.apps.googleusercontent.com".into(),
        Segredo::novo(SEGREDO),
        Segredo::novo(REFRESH),
    );
    config.base_api = falso.base.clone();
    config.url_do_token = format!("{}/token", falso.base);
    (falso, ClienteGmail::novo(config))
}

#[tokio::test]
async fn token_e_renovado_uma_vez_e_vai_no_cabecalho() {
    let (falso, gmail) = gmail_falso().await;
    assert_eq!(
        gmail
            .achar_rotulo("comprovantes")
            .await
            .expect("rótulo")
            .as_deref(),
        Some("Label_1")
    );
    assert!(
        gmail
            .achar_rotulo("Inexistente")
            .await
            .expect("lista")
            .is_none()
    );
    let pagina = gmail
        .listar_mensagens(Some("Label_1"), Some("from:x"), 50, None)
        .await
        .expect("lista");
    assert_eq!(pagina.messages.len(), 2);
    assert_eq!(pagina.next_page_token.as_deref(), Some("pagina-2"));

    assert_eq!(
        falso.pedidos_em("/token").len(),
        1,
        "o token é reaproveitado"
    );
    let lista = falso.pedidos_em("/messages");
    assert_eq!(
        lista[0].autorizacao.as_deref(),
        Some("Bearer token-de-acesso-ficticio")
    );
    assert_eq!(lista[0].parametro("labelIds").as_deref(), Some("Label_1"));
    assert_eq!(lista[0].parametro("maxResults").as_deref(), Some("50"));
    assert_eq!(lista[0].parametro("q").as_deref(), Some("from%3Ax"));
    let token = &falso.pedidos_em("/token")[0];
    assert!(token.corpo.contains("grant_type=refresh_token"));
    assert!(token.corpo.contains("client_id=cliente-ficticio"));
    assert!(token.autorizacao.is_none());
}

#[tokio::test]
async fn refresh_revogado_da_erro_sem_vazar_segredo() {
    let falso = subir(roteiro_do_gmail).await;
    let mut config = ConfigGmail::com_enderecos_padrao(
        "cliente".into(),
        Segredo::novo(SEGREDO),
        Segredo::novo("outro-refresh"),
    );
    config.base_api = falso.base.clone();
    config.url_do_token = format!("{}/token", falso.base);
    let erro = ClienteGmail::novo(config)
        .listar_rotulos()
        .await
        .expect_err("revogado")
        .to_string();
    assert!(erro.contains("HTTP 400"), "{erro}");
    assert!(erro.contains("invalid_grant"), "{erro}");
    assert!(
        !erro.contains(SEGREDO) && !erro.contains("outro-refresh"),
        "{erro}"
    );
}

#[tokio::test]
async fn anexo_volta_em_bytes() {
    let (_falso, gmail) = gmail_falso().await;
    let bytes = gmail.anexo("m-001", "a-001").await.expect("anexo");
    assert!(bytes.starts_with(b"%PDF-1.4"));
}

async fn contar(cliente: &deadpool_postgres::Client, sql: &str, dono: uuid::Uuid) -> i64 {
    cliente
        .query_one(sql, &[&dono])
        .await
        .expect("contagem")
        .get(0)
}

#[tokio::test]
async fn varredura_cria_documentos_e_jobs_e_e_idempotente() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let (falso, gmail) = gmail_falso().await;
    let armazenamento = ArmazenamentoFalso::novo();
    let opcoes = OpcoesDaVarredura::default();

    let e = varrer(&gmail, &banco, &armazenamento, dono, &opcoes)
        .await
        .expect("varredura");
    // m-001: boleto (novo). m-002: ofx (novo) e copia.pdf (mesmo conteúdo do boleto). m-003: sem anexo.
    assert_eq!(e.messages_listed, 3);
    assert_eq!(e.messages_with_attachments, 2);
    assert_eq!(e.attachments_found, 3);
    assert_eq!(e.attachments_uploaded, 2);
    assert_eq!(e.jobs_created, 2);
    assert_eq!(e.skipped_by_content_hash, 1);
    assert_eq!(e.errors, 0);
    assert_eq!(armazenamento.gravacoes(), 2);

    let c = banco.get().await.expect("conexão");
    let linha = c
        .query_one(
            "SELECT d.origin_type, d.origin_key, d.gmail_message_id, d.gmail_thread_id, \
             d.gmail_attachment_id, d.gmail_label, d.gmail_from, d.gmail_subject, d.filename, \
             d.mime_type, d.file_size_bytes, d.storage_path, d.status, d.content_hash, \
             to_char(d.gmail_date AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS') AS data, \
             j.status AS job_status, j.requires, j.metadata, j.max_retries, r.source_type, r.status AS run_status \
             FROM public.source_documents d \
             JOIN public.ingestion_jobs j ON j.source_document_id = d.id \
             JOIN public.ingestion_runs r ON r.id = j.run_id \
             WHERE d.user_id = $1 AND d.filename = 'Boleto Março.pdf'",
            &[&dono],
        )
        .await
        .expect("linha do boleto");
    let sha: String = linha.get("content_hash");
    assert_eq!(linha.get::<_, String>("origin_type"), "gmail");
    assert_eq!(
        linha.get::<_, String>("origin_key"),
        format!("gmail:m-001:{sha}")
    );
    assert_eq!(linha.get::<_, String>("gmail_message_id"), "m-001");
    assert_eq!(linha.get::<_, String>("gmail_thread_id"), "t-001");
    assert_eq!(linha.get::<_, String>("gmail_attachment_id"), "a-001");
    assert_eq!(linha.get::<_, String>("gmail_label"), "INBOX,Label_1");
    assert_eq!(
        linha.get::<_, String>("gmail_from"),
        "Cobrança Exemplo <cobranca@exemplo.test>"
    );
    assert_eq!(linha.get::<_, String>("gmail_subject"), "Boleto de março");
    assert_eq!(linha.get::<_, String>("mime_type"), "application/pdf");
    assert_eq!(linha.get::<_, i64>("file_size_bytes"), 42);
    assert_eq!(linha.get::<_, String>("status"), "active");
    assert_eq!(linha.get::<_, String>("data"), "2026-03-17T17:06:02");
    assert_eq!(
        linha.get::<_, String>("storage_path"),
        format!("{dono}/{sha}/Boleto_Marco.pdf")
    );
    assert_eq!(linha.get::<_, String>("job_status"), "discovered");
    assert_eq!(linha.get::<_, Vec<String>>("requires"), vec!["ocr"]);
    assert_eq!(linha.get::<_, i32>("max_retries"), 3);
    assert_eq!(linha.get::<_, String>("source_type"), "gmail");
    assert_eq!(linha.get::<_, String>("run_status"), "completed");
    let meta: serde_json::Value = linha.get("metadata");
    assert_eq!(
        meta,
        serde_json::json!({
            "filename": "Boleto Março.pdf",
            "origin": "gmail_scanner",
            "gmail_subject": "Boleto de março",
            "gmail_from": "Cobrança Exemplo <cobranca@exemplo.test>",
            "gmail_date": "2026-03-17T17:06:02.000Z"
        })
    );
    let (bytes, tipo) = armazenamento
        .arquivo(&format!("{dono}/{sha}/Boleto_Marco.pdf"))
        .expect("blob");
    assert!(bytes.starts_with(b"%PDF-1.4"));
    assert_eq!(tipo, "application/pdf");

    // O OFX servido como octet-stream não exige OCR e a data de -0300 vira UTC.
    let ofx = c
        .query_one(
            "SELECT j.requires, to_char(d.gmail_date AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS') AS data \
             FROM public.source_documents d JOIN public.ingestion_jobs j ON j.source_document_id = d.id \
             WHERE d.user_id = $1 AND d.filename = 'extrato.ofx'",
            &[&dono],
        )
        .await
        .expect("ofx");
    assert!(ofx.get::<_, Vec<String>>("requires").is_empty());
    assert_eq!(ofx.get::<_, String>("data"), "2026-03-18T12:06:40");

    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.source_documents WHERE user_id = $1",
            dono
        )
        .await,
        2
    );
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.ingestion_jobs WHERE user_id = $1",
            dono
        )
        .await,
        2
    );
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.document_fingerprints WHERE user_id = $1",
            dono
        )
        .await,
        2
    );
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.ingestion_runs WHERE user_id = $1",
            dono
        )
        .await,
        1
    );
    let stats: serde_json::Value = c
        .query_one(
            "SELECT stats FROM public.ingestion_runs WHERE user_id = $1",
            &[&dono],
        )
        .await
        .expect("run")
        .get(0);
    assert_eq!(stats["jobsCreated"], 2);
    assert_eq!(stats["skippedByContentHash"], 1);

    // Segunda varredura completa: nada novo (fase 1 por mensagem+nome, fase 2 por hash).
    let completa = OpcoesDaVarredura {
        completa: true,
        ..OpcoesDaVarredura::default()
    };
    let e2 = varrer(&gmail, &banco, &armazenamento, dono, &completa)
        .await
        .expect("2a varredura");
    assert_eq!(e2.jobs_created, 0);
    assert_eq!(e2.attachments_uploaded, 0);
    assert_eq!(e2.skipped_by_msg_filename, 2);
    assert_eq!(e2.skipped_by_content_hash, 1);
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.ingestion_jobs WHERE user_id = $1",
            dono
        )
        .await,
        2
    );
    assert_eq!(armazenamento.gravacoes(), 2);
    // Sem documento novo não nasce `ingestion_runs` novo.
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.ingestion_runs WHERE user_id = $1",
            dono
        )
        .await,
        1
    );

    // O checkpoint guardou a maior data interna, e a varredura incremental estreita a consulta.
    let marca: String = c
        .query_one(
            "SELECT cursor_value FROM public.ingestion_checkpoints WHERE user_id = $1 AND source_type = 'gmail'",
            &[&dono],
        )
        .await
        .expect("checkpoint")
        .get(0);
    assert_eq!(marca, "1773938800000");
    let antes = falso
        .pedidos_em("/messages")
        .iter()
        .filter(|p| p.caminho == "/messages")
        .count();
    let e3 = varrer(
        &gmail,
        &banco,
        &armazenamento,
        dono,
        &OpcoesDaVarredura::default(),
    )
    .await
    .expect("incremental");
    assert_eq!(e3.jobs_created, 0);
    let listas = falso
        .pedidos()
        .into_iter()
        .filter(|p| p.caminho == "/messages")
        .collect::<Vec<_>>();
    assert!(listas.len() > antes);
    // 1773938800 s menos dois dias.
    let q = listas
        .last()
        .and_then(|p| p.parametro("q"))
        .unwrap_or_default();
    assert_eq!(
        q, "after%3A1773766000",
        "o incremental leva o after: do checkpoint"
    );
    assert!(
        !listas[..antes]
            .iter()
            .any(|p| p.parametro("q").is_some_and(|q| q.contains("after"))),
        "a completa não estreita"
    );
}

#[tokio::test]
async fn falha_no_armazenamento_conta_erro_e_nao_cria_documento() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let (_falso, gmail) = gmail_falso().await;
    let armazenamento = ArmazenamentoFalso::novo();
    armazenamento.falhar(true);
    let e = varrer(
        &gmail,
        &banco,
        &armazenamento,
        dono,
        &OpcoesDaVarredura::default(),
    )
    .await
    .expect("varredura");
    assert_eq!(e.errors, 3);
    assert_eq!(e.jobs_created, 0);
    let c = banco.get().await.expect("conexão");
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.source_documents WHERE user_id = $1",
            dono
        )
        .await,
        0
    );
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.ingestion_runs WHERE user_id = $1",
            dono
        )
        .await,
        0
    );
    // Com erro, o checkpoint não avança a data: o próximo ciclo tenta de novo.
    let marca: Option<String> = c
        .query_one(
            "SELECT cursor_value FROM public.ingestion_checkpoints WHERE user_id = $1 AND source_type = 'gmail'",
            &[&dono],
        )
        .await
        .expect("checkpoint")
        .get(0);
    assert_eq!(marca, None);
}

#[tokio::test]
async fn simulacao_nao_escreve_nada() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let (_falso, gmail) = gmail_falso().await;
    let armazenamento = ArmazenamentoFalso::novo();
    let opcoes = OpcoesDaVarredura {
        simulacao: true,
        ..OpcoesDaVarredura::default()
    };
    let e = varrer(&gmail, &banco, &armazenamento, dono, &opcoes)
        .await
        .expect("varredura");
    assert_eq!(e.attachments_found, 3);
    assert_eq!(e.jobs_created, 0);
    assert_eq!(armazenamento.gravacoes(), 0);
    let c = banco.get().await.expect("conexão");
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.source_documents WHERE user_id = $1",
            dono
        )
        .await,
        0
    );
    assert_eq!(
        contar(
            &c,
            "SELECT count(*) FROM public.ingestion_checkpoints WHERE user_id = $1",
            dono
        )
        .await,
        0
    );
}

#[tokio::test]
async fn rotulo_inexistente_lista_os_disponiveis() {
    let Some(banco) = banco().await else { return };
    let dono = novo_dono(&banco).await;
    let (_falso, gmail) = gmail_falso().await;
    let opcoes = OpcoesDaVarredura {
        rotulo: "Nao Existe".into(),
        ..OpcoesDaVarredura::default()
    };
    let erro = varrer(&gmail, &banco, &ArmazenamentoFalso::novo(), dono, &opcoes)
        .await
        .expect_err("sem rótulo")
        .to_string();
    assert!(
        erro.contains("Comprovantes") && erro.contains("INBOX"),
        "{erro}"
    );
}
