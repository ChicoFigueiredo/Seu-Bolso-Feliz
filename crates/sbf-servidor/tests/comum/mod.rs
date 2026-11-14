//! Apoio dos testes: um "Google" falso (chave RSA gerada no teste e o JWKS dela), o
//! Postgres de teste (só com `SBF_PG_TESTE=1`) e pedidos HTTP direto no `Router`.
#![allow(dead_code)]

use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::BodyExt as _;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair, RsaPublicKeyComponents};
use sbf_servidor::google::{ChavesFixas, Jwks, VerificadorGoogle};
use serde_json::{Value, json};
use tower::ServiceExt as _;

pub const CLIENT_ID: &str = "cliente-de-teste.apps.googleusercontent.com";
pub const KID: &str = "chave-de-teste-1";

/// Uma chave RSA de teste com o JWKS dela. As chaves em `tests/dados/*.pk8.der` foram
/// geradas com `openssl genpkey` só para estes testes: não assinam nada de verdade.
pub struct GoogleFalso {
    pub privada: RsaKeyPair,
    pub jwks: Arc<Jwks>,
}

fn carregar_chave(der: &[u8], kid: &str) -> GoogleFalso {
    let privada = RsaKeyPair::from_pkcs8(der).expect("chave RSA de teste");
    let publica = RsaPublicKeyComponents::<Vec<u8>>::from(privada.public());
    let jwks: Jwks = serde_json::from_value(json!({
        "keys": [{
            "kty": "RSA", "kid": kid, "alg": "RS256", "use": "sig",
            "n": URL_SAFE_NO_PAD.encode(&publica.n),
            "e": URL_SAFE_NO_PAD.encode(&publica.e),
        }]
    }))
    .expect("jwks de teste");
    GoogleFalso {
        privada,
        jwks: Arc::new(jwks),
    }
}

/// O "Google" dos testes.
pub fn google() -> &'static GoogleFalso {
    static G: OnceLock<GoogleFalso> = OnceLock::new();
    G.get_or_init(|| carregar_chave(include_bytes!("../dados/google-falso-1.pk8.der"), KID))
}

/// Outra chave, com a mesma `kid`: um token assinado por ela é falso.
pub fn google_impostor() -> &'static GoogleFalso {
    static G: OnceLock<GoogleFalso> = OnceLock::new();
    G.get_or_init(|| carregar_chave(include_bytes!("../dados/google-falso-2.pk8.der"), KID))
}

pub fn verificador() -> VerificadorGoogle {
    VerificadorGoogle::novo(CLIENT_ID, Box::new(ChavesFixas(Arc::clone(&google().jwks))))
}

pub fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("relógio")
        .as_secs()
}

/// As reivindicações de um login válido; o teste muda o que quiser.
pub fn reivindicacoes(email: &str, sub: &str, nonce: &str) -> Value {
    json!({
        "iss": "https://accounts.google.com",
        "aud": CLIENT_ID,
        "sub": sub,
        "email": email,
        "email_verified": true,
        "nonce": nonce,
        "name": "Pessoa Fictícia",
        "iat": agora(),
        "exp": agora() + 600,
    })
}

/// Monta um JWT com o cabeçalho e as reivindicações dados, assinado em RS256.
pub fn assinar_jwt(chave: &GoogleFalso, cabecalho: &Value, reivindicacoes: &Value) -> String {
    let mut jwt = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(cabecalho).expect("json")),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(reivindicacoes).expect("json")),
    );
    let mut assinatura = vec![0u8; chave.privada.public().modulus_len()];
    chave
        .privada
        .sign(
            &RSA_PKCS1_SHA256,
            &ring::rand::SystemRandom::new(),
            jwt.as_bytes(),
            &mut assinatura,
        )
        .expect("assinar");
    jwt.push('.');
    jwt.push_str(&URL_SAFE_NO_PAD.encode(assinatura));
    jwt
}

pub fn assinar_com(chave: &GoogleFalso, kid: &str, reivindicacoes: &Value) -> String {
    assinar_jwt(
        chave,
        &json!({ "alg": "RS256", "typ": "JWT", "kid": kid }),
        reivindicacoes,
    )
}

pub fn id_token(email: &str, sub: &str, nonce: &str) -> String {
    assinar_com(google(), KID, &reivindicacoes(email, sub, nonce))
}

// ───────────────────────────── Postgres de teste ─────────────────────────────

/// A URL do Postgres de teste, ou `None` (e o teste pula) sem `SBF_PG_TESTE=1`.
pub fn url_de_teste() -> Option<String> {
    if std::env::var("SBF_PG_TESTE").as_deref() != Ok("1") {
        eprintln!("pulado: rode com scripts/desktop/postgres-de-teste.sh testar");
        return None;
    }
    Some(std::env::var("SBF_PG_TESTE_URL").expect("SBF_PG_TESTE_URL"))
}

pub struct Ambiente {
    pub app: Router,
    pub banco: deadpool_postgres::Pool,
    pub armazenamento: sbf_servidor::armazenamento::ArmazenamentoFalso,
}

pub async fn ambiente() -> Option<Ambiente> {
    let url = zeroize::Zeroizing::new(url_de_teste()?);
    let banco = sbf_servidor::abrir_banco(&url)
        .await
        .expect("banco de teste");
    let armazenamento = sbf_servidor::armazenamento::ArmazenamentoFalso::novo();
    let estado = sbf_servidor::EstadoDoServidor {
        banco: banco.clone(),
        google: Arc::new(verificador()),
        regras: "0.1.0-teste".into(),
        armazenamento: Arc::new(armazenamento.clone()),
    };
    Some(Ambiente {
        app: sbf_servidor::rotas(estado),
        banco,
        armazenamento,
    })
}

pub struct Resposta {
    pub status: StatusCode,
    pub etag: Option<String>,
    pub corpo: Value,
    pub bruto: Vec<u8>,
}

impl Ambiente {
    pub async fn pedir(
        &self,
        metodo: &str,
        caminho: &str,
        credencial: Option<&str>,
        cabecalhos: &[(&str, &str)],
        corpo: Option<Value>,
    ) -> Resposta {
        let mut r = Request::builder().method(metodo).uri(caminho);
        if let Some(c) = credencial {
            r = r.header("authorization", format!("Bearer {c}"));
        }
        for (k, v) in cabecalhos {
            r = r.header(*k, *v);
        }
        let corpo = match corpo {
            Some(v) => {
                r = r.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&v).expect("json"))
            }
            None => Body::empty(),
        };
        let resposta = self
            .app
            .clone()
            .oneshot(r.body(corpo).expect("pedido"))
            .await
            .expect("resposta");
        let status = resposta.status();
        let etag = resposta
            .headers()
            .get("etag")
            .map(|v| v.to_str().expect("etag").to_owned());
        let bruto = resposta
            .into_body()
            .collect()
            .await
            .expect("corpo")
            .to_bytes()
            .to_vec();
        let corpo = if bruto.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bruto).unwrap_or(Value::Null)
        };
        Resposta {
            status,
            etag,
            corpo,
            bruto,
        }
    }

    pub async fn get(&self, caminho: &str, credencial: &str) -> Resposta {
        self.pedir("GET", caminho, Some(credencial), &[], None)
            .await
    }

    pub async fn post(&self, caminho: &str, credencial: &str, corpo: Value) -> Resposta {
        self.pedir("POST", caminho, Some(credencial), &[], Some(corpo))
            .await
    }

    /// Pareia uma estação para o usuário do e-mail e devolve a credencial.
    pub async fn parear(&self, email: &str, sub: &str, nome: &str) -> (Value, String) {
        let r = self
            .pedir(
                "POST",
                "/api/v1/estacoes/pareamento",
                None,
                &[],
                Some(json!({
                    "id_token": id_token(email, sub, "nonce-de-teste"),
                    "nonce": "nonce-de-teste",
                    "nome": nome,
                    "sistema": "linux",
                })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.corpo);
        let credencial = r.corpo["credencial"]
            .as_str()
            .expect("credencial")
            .to_owned();
        (r.corpo["estacao"].clone(), credencial)
    }

    /// Um pedido com corpo bruto (multipart).
    pub async fn pedir_bruto(
        &self,
        caminho: &str,
        credencial: &str,
        tipo_de_conteudo: &str,
        corpo: Vec<u8>,
    ) -> Resposta {
        let r = Request::builder()
            .method("POST")
            .uri(caminho)
            .header("authorization", format!("Bearer {credencial}"))
            .header("content-type", tipo_de_conteudo)
            .body(Body::from(corpo))
            .expect("pedido");
        let resposta = self.app.clone().oneshot(r).await.expect("resposta");
        let status = resposta.status();
        let bruto = resposta
            .into_body()
            .collect()
            .await
            .expect("corpo")
            .to_bytes()
            .to_vec();
        let corpo = serde_json::from_slice(&bruto).unwrap_or(Value::Null);
        Resposta {
            status,
            etag: None,
            corpo,
            bruto,
        }
    }

    pub async fn sql(&self, sql: &str) {
        let c = self.banco.get().await.expect("conexão");
        c.batch_execute(sql).await.expect("sql de teste");
    }
}

/// Um dono fictício novo, com um e-mail único. Devolve (id, email, sub).
pub async fn novo_dono(amb: &Ambiente) -> (uuid::Uuid, String, String) {
    let id = uuid::Uuid::new_v4();
    let email = format!("dono-{}@example.com", id.simple());
    let sub = format!("sub-{}", id.simple());
    amb.sql(&format!(
        "INSERT INTO public.users (id, email, name, role) VALUES ('{id}', '{email}', 'Dono Fictício', 'owner')"
    ))
    .await;
    (id, email, sub)
}

/// Os ids das linhas semeadas para um dono: uma linha em cada coleção.
pub struct Semeado {
    pub ids: Vec<uuid::Uuid>,
    pub movimentacao: uuid::Uuid,
    pub movimentacao_dez: uuid::Uuid,
    pub movimentacao_vinte: uuid::Uuid,
    pub categoria: uuid::Uuid,
    pub outra_categoria: uuid::Uuid,
    pub conta: uuid::Uuid,
    pub centro: uuid::Uuid,
    pub rateio: uuid::Uuid,
    pub fornecedor: uuid::Uuid,
}

/// Semeia uma linha (pelo menos) em cada uma das 19 coleções do dono, com valores
/// fictícios: 0,10 e 0,20 nas movimentações para o teste de centavos.
pub async fn semear(amb: &Ambiente, dono: uuid::Uuid) -> Semeado {
    let n: Vec<uuid::Uuid> = (0..24).map(|_| uuid::Uuid::new_v4()).collect();
    let [
        inst,
        conta,
        cartao_conta,
        cartao,
        fatura,
        item,
        mov,
        mov10,
        mov20,
        transf,
        rateio,
        cat,
        cat2,
        tag,
        centro,
        forn,
        apelido,
        contrato,
        divida,
        parcela,
        recorrente,
        ocorrencia,
        doc,
        divida_conta,
    ] = n[..]
    else {
        unreachable!()
    };
    amb.sql(&format!(
        r#"
INSERT INTO public.institutions (id, user_id, name, type) VALUES ('{inst}', '{dono}', 'Banco Exemplo', 'credit_union');
INSERT INTO public.financial_products (id, user_id, institution_id, name, type, current_balance, credit_limit) VALUES
  ('{conta}', '{dono}', '{inst}', 'Conta Exemplo', 'checking_account', 1234.56, NULL),
  ('{cartao_conta}', '{dono}', '{inst}', 'Cartão Exemplo', 'credit_card', 0, 5000.00),
  ('{divida_conta}', '{dono}', '{inst}', 'Empréstimo Exemplo', 'personal_loan', -9999999999999.99, NULL);
INSERT INTO public.cards (id, user_id, financial_product_id, last_four_digits, credit_limit, closing_day, due_day)
  VALUES ('{cartao}', '{dono}', '{cartao_conta}', '0000', 5000.00, 3, 10);
INSERT INTO public.statement_cycles (id, user_id, card_id, reference_month, cycle_start_date, cycle_end_date, due_date, total_amount, paid_amount)
  VALUES ('{fatura}', '{dono}', '{cartao}', '2026-11-01', '2026-10-04', '2026-11-03', '2026-11-10', 350.75, 0);
INSERT INTO public.categories (id, user_id, name) VALUES ('{cat}', '{dono}', 'Mercado'), ('{cat2}', '{dono}', 'Casa');
INSERT INTO public.tags (id, user_id, name, suggested_priority) VALUES ('{tag}', '{dono}', 'viagem', 'high');
INSERT INTO public.suppliers (id, user_id, name, type) VALUES ('{forn}', '{dono}', 'Fornecedor Exemplo', 'utility');
INSERT INTO public.supplier_tags (user_id, supplier_id, tag_id) VALUES ('{dono}', '{forn}', '{tag}');
INSERT INTO public.supplier_aliases (id, user_id, supplier_id, alias_name, alias_type) VALUES ('{apelido}', '{dono}', '{forn}', 'FORN EXEMPLO {apelido}', 'billing_name');
INSERT INTO public.supplier_contracts (id, user_id, supplier_id, contract_type, identifier) VALUES ('{contrato}', '{dono}', '{forn}', 'utility', 'INST-0001');
INSERT INTO public.source_documents (id, user_id, origin_type, origin_key, filename, status, supplier_id)
  VALUES ('{doc}', '{dono}', 'manual_upload', 'chave-{doc}', 'fatura-exemplo.pdf', 'new', '{forn}');
INSERT INTO public.transactions (id, user_id, financial_product_id, type, amount, description, event_date, is_confirmed, category_id, supplier_id, source_document_id) VALUES
  ('{mov}', '{dono}', '{conta}', 'expense', 150.00, 'Conta de luz exemplo', '2026-11-05', true, '{cat}', '{forn}', '{doc}'),
  ('{mov10}', '{dono}', '{conta}', 'expense', 0.10, 'Bala exemplo', '2026-11-06', true, NULL, NULL, NULL),
  ('{mov20}', '{dono}', '{conta}', 'refund', 0.20, 'Estorno exemplo', '2026-11-07', false, NULL, NULL, NULL);
INSERT INTO public.transaction_tags (transaction_id, tag_id) VALUES ('{mov}', '{tag}');
INSERT INTO public.statement_items (id, user_id, statement_cycle_id, transaction_id, description, amount, transaction_date)
  VALUES ('{item}', '{dono}', '{fatura}', NULL, 'Compra exemplo', 350.75, '2026-10-15');
INSERT INTO public.transfers (id, user_id, source_product_id, target_product_id, amount, event_date)
  VALUES ('{transf}', '{dono}', '{conta}', '{cartao_conta}', 350.75, '2026-11-10');
INSERT INTO public.financial_centers (id, user_id, name, type) VALUES ('{centro}', '{dono}', 'Centro {centro}', 'pessoa');
INSERT INTO public.transaction_allocations (id, user_id, transaction_id, financial_center_id, amount) VALUES ('{rateio}', '{dono}', '{mov}', '{centro}', 100.00);
INSERT INTO public.liabilities (id, user_id, financial_product_id, name, type, original_amount, outstanding_balance, interest_rate, rate_type, amortization_system, total_installments)
  VALUES ('{divida}', '{dono}', '{divida_conta}', 'Empréstimo Exemplo', 'personal_loan', 10000.00, 7500.50, 0.012500, 'monthly', 'price', 24);
INSERT INTO public.liability_tags (liability_id, tag_id) VALUES ('{divida}', '{tag}');
INSERT INTO public.liability_installments (id, user_id, liability_id, installment_number, due_date, total_amount, principal_amount, interest_amount)
  VALUES ('{parcela}', '{dono}', '{divida}', 1, '2026-12-10', 480.33, 400.00, 80.33);
INSERT INTO public.recurring_templates (id, user_id, financial_product_id, name, type, amount, frequency, day_of_month)
  VALUES ('{recorrente}', '{dono}', '{conta}', 'Internet exemplo', 'expense', 99.90, 'monthly', 15);
INSERT INTO public.recurring_template_tags (recurring_template_id, tag_id) VALUES ('{recorrente}', '{tag}');
INSERT INTO public.recurring_instances (id, user_id, recurring_template_id, expected_date, expected_amount)
  VALUES ('{ocorrencia}', '{dono}', '{recorrente}', '2026-11-15', 99.90);
"#
    ))
    .await;
    Semeado {
        ids: n,
        movimentacao: mov,
        movimentacao_dez: mov10,
        movimentacao_vinte: mov20,
        categoria: cat,
        outra_categoria: cat2,
        conta,
        centro,
        rateio,
        fornecedor: forn,
    }
}

/// Um multipart com os campos de texto e o arquivo, na ordem do contrato.
pub fn multipart(
    campos: &[(&str, &str)],
    arquivo: Option<(&str, &str, &[u8])>,
) -> (String, Vec<u8>) {
    let limite = "----sbf-teste-limite";
    let mut corpo = Vec::new();
    for (nome, valor) in campos {
        corpo.extend_from_slice(
            format!(
                "--{limite}\r\nContent-Disposition: form-data; name=\"{nome}\"\r\n\r\n{valor}\r\n"
            )
            .as_bytes(),
        );
    }
    if let Some((nome_do_arquivo, tipo, bytes)) = arquivo {
        corpo.extend_from_slice(
            format!(
                "--{limite}\r\nContent-Disposition: form-data; name=\"arquivo\"; filename=\"{nome_do_arquivo}\"\r\nContent-Type: {tipo}\r\n\r\n"
            )
            .as_bytes(),
        );
        corpo.extend_from_slice(bytes);
        corpo.extend_from_slice(b"\r\n");
    }
    corpo.extend_from_slice(format!("--{limite}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={limite}"), corpo)
}

/// Um lote com rascunhos de movimentação. `validos` rascunhos completos e `invalidos` sem
/// conta. Devolve (lote, ids dos rascunhos na ordem de criação).
pub async fn semear_lote(
    amb: &Ambiente,
    dono: uuid::Uuid,
    conta: uuid::Uuid,
    validos: usize,
    invalidos: usize,
) -> (uuid::Uuid, Vec<uuid::Uuid>) {
    let lote = uuid::Uuid::new_v4();
    amb.sql(&format!(
        "INSERT INTO public.draft_batches (id, user_id, name, status, total_drafts) VALUES ('{lote}', '{dono}', 'Lote exemplo', 'open', {})",
        validos + invalidos
    ))
    .await;
    let mut ids = Vec::new();
    for i in 0..(validos + invalidos) {
        let id = uuid::Uuid::new_v4();
        let dados = if i < validos {
            json!({
                "schema_version": 1, "draft_type": "transaction", "direction": "outflow",
                "transaction_type": "expense", "amount_cents": 1250 + i as i64,
                "event_date": "2026-11-10", "financial_product_id": conta,
                "description": format!("Rascunho {i}"),
                "provenance": {"field_sources": {}}
            })
        } else {
            json!({
                "schema_version": 1, "draft_type": "transaction", "direction": "outflow",
                "transaction_type": "expense", "amount_cents": 500,
                "event_date": "2026-11-10", "description": "Sem conta",
                "provenance": {"field_sources": {}}
            })
        };
        amb.sql(&format!(
            "INSERT INTO public.draft_records (id, batch_id, user_id, draft_type, status, draft_data, draft_schema_version, created_at) \
             VALUES ('{id}', '{lote}', '{dono}', 'transaction', 'pending_review', '{}'::jsonb, 1, now() + interval '{i} seconds')",
            dados.to_string().replace('\'', "''")
        ))
        .await;
        ids.push(id);
    }
    (lote, ids)
}
