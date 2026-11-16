//! O bot e os alertas de ponta a ponta: Telegram falso em loopback (nunca a API real) e
//! Postgres de teste (`SBF_PG_TESTE=1`, via `scripts/desktop/postgres-de-teste.sh testar-canais`).
//! Dados todos fictícios.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use deadpool_postgres::Pool;
use sbf_canais::agenda::{ContextoDeAlertas, passo, rodar_uma_vez};
use sbf_canais::bot::Bot;
use sbf_canais::config::{ConfigAlertas, ConfigTelegram};
use sbf_canais::repo;
use sbf_canais::telegram::ClienteTelegram;
use sbf_coletor::config::Segredo;
use sbf_livro::Data;
use sbf_servidor::armazenamento::ArmazenamentoFalso;
use serde_json::{Value, json};
use uuid::Uuid;
use zeroize::Zeroizing;

const TOKEN: &str = "123456:token-ficticio-nao-vazar";
/// Um chat_id só deste teste: os testes rodam em paralelo no mesmo banco.
fn chat_novo() -> i64 {
    i64::try_from(Uuid::new_v4().as_u128() % 1_000_000_000).unwrap_or(1) + 10_000
}

// ───────────────────────────── Telegram falso ─────────────────────────────

#[derive(Default)]
struct Cena {
    fila: Vec<Value>,
    enviadas: Vec<(i64, String)>,
    caminhos: Vec<String>,
    arquivos: Vec<(String, Vec<u8>)>,
}

#[derive(Clone)]
struct Falso {
    base: String,
    cena: Arc<Mutex<Cena>>,
}

impl Falso {
    fn cena(&self) -> std::sync::MutexGuard<'_, Cena> {
        self.cena.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn mandar(&self, update: Value) {
        self.cena().fila.push(update);
    }
    fn enviadas(&self) -> Vec<(i64, String)> {
        self.cena().enviadas.clone()
    }
    fn limpar(&self) {
        self.cena().enviadas.clear();
    }
}

fn json_ok(v: Value) -> Response {
    Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(json!({ "ok": true, "result": v }).to_string()))
        .unwrap_or_default()
}

async fn tratar(State(cena): State<Arc<Mutex<Cena>>>, req: Request<Body>) -> Response {
    let (partes, corpo) = req.into_parts();
    let corpo = axum::body::to_bytes(corpo, 1 << 20)
        .await
        .unwrap_or_default();
    let corpo: Value = serde_json::from_slice(&corpo).unwrap_or(Value::Null);
    let caminho = partes.uri.path().to_owned();
    let mut c = cena.lock().unwrap_or_else(PoisonError::into_inner);
    c.caminhos.push(caminho.clone());
    let prefixo = format!("/bot{TOKEN}/");
    if let Some(metodo) = caminho.strip_prefix(&prefixo) {
        return match metodo {
            "getUpdates" => {
                let a_partir = corpo["offset"].as_i64().unwrap_or(0);
                c.fila
                    .retain(|u| u["update_id"].as_i64().unwrap_or(0) >= a_partir);
                json_ok(Value::Array(c.fila.clone()))
            }
            "sendMessage" => {
                let chat = corpo["chat_id"].as_i64().unwrap_or(0);
                let texto = corpo["text"].as_str().unwrap_or("").to_owned();
                c.enviadas.push((chat, texto));
                json_ok(json!({ "message_id": 1 }))
            }
            "getFile" => {
                let id = corpo["file_id"].as_str().unwrap_or("");
                match c.arquivos.iter().find(|(i, _)| i == id) {
                    Some((_, b)) => {
                        json_ok(json!({ "file_path": format!("docs/{id}"), "file_size": b.len() }))
                    }
                    None => Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(
                            json!({ "ok": false, "description": "file not found" }).to_string(),
                        ))
                        .unwrap_or_default(),
                }
            }
            _ => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap_or_default(),
        };
    }
    if let Some(id) = caminho.strip_prefix(&format!("/file/bot{TOKEN}/docs/"))
        && let Some((_, b)) = c.arquivos.iter().find(|(i, _)| i == id)
    {
        return Response::builder()
            .body(Body::from(b.clone()))
            .unwrap_or_default();
    }
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap_or_default()
}

async fn telegram_falso() -> Falso {
    let cena = Arc::new(Mutex::new(Cena::default()));
    let app = Router::new().fallback(tratar).with_state(Arc::clone(&cena));
    let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("porta");
    let porta = ouvinte.local_addr().expect("endereço").port();
    tokio::spawn(async move {
        let _ = axum::serve(ouvinte, app).await;
    });
    Falso {
        base: format!("http://127.0.0.1:{porta}"),
        cena,
    }
}

fn arquivo_no_telegram(f: &Falso, id: &str, bytes: &[u8]) {
    f.cena().arquivos.push((id.to_owned(), bytes.to_vec()));
}

fn texto(update_id: i64, chat: i64, t: &str) -> Value {
    json!({ "update_id": update_id, "message": { "chat": { "id": chat }, "text": t } })
}

fn documento(update_id: i64, chat: i64, file_id: &str, nome: &str) -> Value {
    json!({ "update_id": update_id, "message": { "chat": { "id": chat },
        "document": { "file_id": file_id, "file_unique_id": "u1", "file_name": nome } } })
}

// ───────────────────────────── Postgres de teste ─────────────────────────────

async fn banco() -> Option<Pool> {
    if std::env::var("SBF_PG_TESTE").as_deref() != Ok("1") {
        eprintln!("pulado: rode com scripts/desktop/postgres-de-teste.sh testar-canais");
        return None;
    }
    let url = Zeroizing::new(std::env::var("SBF_PG_TESTE_URL").expect("SBF_PG_TESTE_URL"));
    Some(sbf_servidor::banco::abrir_pool(&url, 4).expect("pool de teste"))
}

async fn novo_dono(banco: &Pool) -> Uuid {
    let c = banco.get().await.expect("conexão");
    let id = Uuid::new_v4();
    c.execute(
        "INSERT INTO public.users (id, email, name, role, google_sub) \
         VALUES ($1, $2, 'Dono Fictício', 'owner', $3)",
        &[&id, &format!("{id}@example.com"), &id.to_string()],
    )
    .await
    .expect("dono");
    id
}

async fn sql(banco: &Pool, comandos: &str) {
    banco
        .get()
        .await
        .expect("conexão")
        .batch_execute(comandos)
        .await
        .expect("sql");
}

fn bot(
    banco: &Pool,
    falso: &Falso,
    dono: Uuid,
    chat_dono: i64,
    armazenamento: ArmazenamentoFalso,
) -> Bot {
    let tg = ClienteTelegram::novo(&falso.base, Segredo::novo(TOKEN)).expect("cliente");
    let mut bot = Bot::novo(
        banco.clone(),
        tg,
        Arc::new(armazenamento),
        BTreeSet::from([chat_dono]),
        dono,
    );
    // Cada teste tem o seu offset: no serviço de verdade há um bot só.
    bot.chave_do_offset = format!("telegram.offset.{chat_dono}");
    bot
}

async fn contar(banco: &Pool, tabela: &str, dono: Uuid) -> i64 {
    banco
        .get()
        .await
        .expect("conexão")
        .query_one(
            &format!("SELECT count(*) FROM public.{tabela} WHERE user_id = $1"),
            &[&dono],
        )
        .await
        .expect("contagem")
        .get(0)
}

// ───────────────────────────── vinculação ─────────────────────────────

#[tokio::test]
async fn chat_desconhecido_so_entra_com_codigo_de_uso_unico_que_expira() {
    let Some(banco) = banco().await else { return };
    let falso = telegram_falso().await;
    let dono = novo_dono(&banco).await;
    let (chat_dono, chat_estranho) = (chat_novo(), chat_novo());
    let (chat_terceiro, chat_quarto) = (chat_novo(), chat_novo());
    let mut bot = bot(&banco, &falso, dono, chat_dono, ArmazenamentoFalso::novo());

    // Desconhecido: qualquer conversa é ignorada em silêncio.
    falso.mandar(texto(1, chat_estranho, "oi"));
    falso.mandar(texto(2, chat_estranho, "/resumo"));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas().is_empty());

    // Código errado: só diz que é inválido.
    falso.mandar(texto(3, chat_estranho, "/vincular ZZZZZZZZ"));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas()[0].1.contains("inválido"));
    falso.limpar();

    // Código bom: vincula, mascarado, e só vale uma vez.
    let codigo = repo::gerar_codigo(&banco, dono).await.expect("código");
    falso.mandar(texto(4, chat_estranho, &format!("/vincular {codigo}")));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas()[0].1.starts_with("Vinculado"));
    let d = repo::destinatario_do_chat(&banco, chat_estranho)
        .await
        .expect("consulta")
        .expect("vinculado");
    assert_eq!(d.nivel.codigo(), "mascarado");
    assert_eq!(d.dono, dono);
    falso.limpar();
    falso.mandar(texto(5, chat_terceiro, &format!("/vincular {codigo}")));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas()[0].1.contains("inválido"));
    assert!(
        repo::destinatario_do_chat(&banco, chat_terceiro)
            .await
            .expect("c")
            .is_none()
    );

    // Código vencido (passou dos 10 minutos).
    let velho = repo::gerar_codigo(&banco, dono).await.expect("código");
    sql(
        &banco,
        &format!("UPDATE public.canais_codigos SET expira_em = now() - interval '1 minute' WHERE user_id = '{dono}' AND usado_em IS NULL"),
    )
    .await;
    falso.limpar();
    falso.mandar(texto(6, chat_quarto, &format!("/vincular {velho}")));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas()[0].1.contains("inválido"));
    assert!(
        repo::destinatario_do_chat(&banco, chat_quarto)
            .await
            .expect("c")
            .is_none()
    );

    // O token só aparece no caminho da Bot API, e o offset foi guardado.
    assert!(
        falso
            .cena()
            .caminhos
            .iter()
            .all(|c| c.starts_with(&format!("/bot{TOKEN}/")))
    );
    assert_eq!(
        repo::ler_estado_do_servico(&banco, &format!("telegram.offset.{chat_dono}"))
            .await
            .expect("estado"),
        Some("7".to_owned())
    );
}

// ───────────────────────────── entrada de documentos ─────────────────────────────

#[tokio::test]
async fn documento_vai_para_a_caixa_de_entrada_uma_vez_so() {
    let Some(banco) = banco().await else { return };
    let falso = telegram_falso().await;
    let dono = novo_dono(&banco).await;
    let armazenamento = ArmazenamentoFalso::novo();
    let (chat_dono, chat_estranho) = (chat_novo(), chat_novo());
    let mut bot = bot(&banco, &falso, dono, chat_dono, armazenamento.clone());
    let pdf = b"%PDF-1.4 fatura ficticia do telegram";
    arquivo_no_telegram(&falso, "f1", pdf);

    falso.mandar(documento(10, chat_dono, "f1", "fatura-exemplo.pdf"));
    bot.uma_rodada(0).await.expect("rodada");
    assert_eq!(
        falso.enviadas()[0].1,
        "Recebido, vai para a caixa de entrada."
    );
    assert_eq!(contar(&banco, "source_documents", dono).await, 1);
    assert_eq!(contar(&banco, "ingestion_jobs", dono).await, 1);
    let linha = banco
        .get()
        .await
        .expect("c")
        .query_one(
            "SELECT s.origin_type, s.content_hash, j.status, j.metadata->>'origin' \
             FROM public.source_documents s JOIN public.ingestion_jobs j ON j.source_document_id = s.id \
             WHERE s.user_id = $1",
            &[&dono],
        )
        .await
        .expect("linha");
    assert_eq!(linha.get::<_, String>(0), "manual_upload");
    assert_eq!(linha.get::<_, String>(1).len(), 64);
    assert_eq!(linha.get::<_, String>(2), "discovered");
    assert_eq!(linha.get::<_, String>(3), "telegram");
    assert_eq!(armazenamento.gravacoes(), 1);

    // O mesmo arquivo de novo: idempotente pelo SHA-256.
    falso.limpar();
    falso.mandar(documento(11, chat_dono, "f1", "outro-nome.pdf"));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(
        falso.enviadas()[0].1.contains("já estava"),
        "{:?}",
        falso.enviadas()
    );
    assert_eq!(contar(&banco, "source_documents", dono).await, 1);
    assert_eq!(contar(&banco, "ingestion_jobs", dono).await, 1);

    // Foto: vira JPG.
    arquivo_no_telegram(&falso, "p1", b"\xff\xd8\xff\xe0 foto ficticia");
    falso.limpar();
    falso.mandar(json!({ "update_id": 12, "message": { "chat": { "id": chat_dono },
        "photo": [ { "file_id": "p0", "file_size": 10 }, { "file_id": "p1", "file_size": 999, "file_unique_id": "uf" } ] } }));
    bot.uma_rodada(0).await.expect("rodada");
    assert_eq!(
        falso.enviadas()[0].1,
        "Recebido, vai para a caixa de entrada."
    );
    assert_eq!(contar(&banco, "source_documents", dono).await, 2);

    // Formato que o pipeline não abre: avisa, não grava.
    arquivo_no_telegram(&falso, "z1", b"MZ binario");
    falso.limpar();
    falso.mandar(documento(13, chat_dono, "z1", "programa.exe"));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas()[0].1.contains("formato não aceito"));
    assert_eq!(contar(&banco, "source_documents", dono).await, 2);

    // Chat não autorizado: nada é baixado nem gravado.
    arquivo_no_telegram(&falso, "e1", b"%PDF-1.4 do estranho");
    falso.limpar();
    falso.mandar(documento(14, chat_estranho, "e1", "intruso.pdf"));
    bot.uma_rodada(0).await.expect("rodada");
    assert!(falso.enviadas().is_empty());
    assert_eq!(contar(&banco, "source_documents", dono).await, 2);

    // As respostas ficaram registradas (tipo, tamanho, nível), sem o texto.
    assert!(contar(&banco, "canais_saidas", dono).await >= 4);
}

// ───────────────────────────── alertas ─────────────────────────────

async fn semear_livro(banco: &Pool, dono: Uuid) {
    let n: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
    sql(
        banco,
        &format!(
            r#"
INSERT INTO public.institutions (id, user_id, name, type) VALUES ('{0}', '{dono}', 'Banco Exemplo', 'credit_union');
INSERT INTO public.financial_products (id, user_id, institution_id, name, type, current_balance)
  VALUES ('{1}', '{dono}', '{0}', 'Conta Exemplo', 'checking_account', 1000.00);
INSERT INTO public.suppliers (id, user_id, name, type) VALUES ('{2}', '{dono}', 'Padaria Exemplo', 'other');
INSERT INTO public.transactions (id, user_id, financial_product_id, type, amount, description, event_date, is_confirmed, supplier_id) VALUES
  ('{3}', '{dono}', '{1}', 'expense', 142.30, 'Boleto 23790.12345 60000.123456 Cemig Exemplo', '2026-11-16', false, NULL),
  (gen_random_uuid(), '{dono}', '{1}', 'expense', 800.00, 'Aluguel Exemplo', '2026-11-10', false, NULL),
  (gen_random_uuid(), '{dono}', '{1}', 'expense', 35.00, 'Pao', '2026-11-02', true, '{2}'),
  (gen_random_uuid(), '{dono}', '{1}', 'expense', 35.00, 'Pao', '2026-11-03', true, '{2}');
"#,
            n[0], n[1], n[2], n[3]
        ),
    )
    .await;
}

fn contexto(banco: &Pool, falso: &Falso, dono: Uuid, chat_dono: i64) -> ContextoDeAlertas {
    let tg = ConfigTelegram {
        token: Segredo::novo(TOKEN),
        base: falso.base.clone(),
        chats_autorizados: BTreeSet::from([chat_dono]),
    };
    ContextoDeAlertas {
        banco: banco.clone(),
        telegram: ClienteTelegram::novo(&tg.base, tg.token.clone()).expect("cliente"),
        dono,
        chats_configurados: vec![chat_dono],
        config: ConfigAlertas::default(),
        extras: Arc::new(Vec::new()),
    }
}

fn hoje() -> Data {
    Data::nova(2026, 11, 14).expect("data")
}

#[tokio::test]
async fn alertas_saem_mascarados_sem_repetir_e_registram_o_que_saiu() {
    let Some(banco) = banco().await else { return };
    let falso = telegram_falso().await;
    let dono = novo_dono(&banco).await;
    semear_livro(&banco, dono).await;
    let chat_dono = chat_novo();
    let ctx = contexto(&banco, &falso, dono, chat_dono);

    let n = rodar_uma_vez(&ctx, hoje(), false).await.expect("rodada");
    assert_eq!(n, 3); // a vencer, vencida, duplicada
    let msgs = falso.enviadas();
    assert_eq!(msgs.len(), 3);
    let tudo = msgs
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(tudo.contains("Cemig Exemplo") && tudo.contains("R$ 142,30"));
    assert!(tudo.contains("vence em 2 dias (16/11)"));
    assert!(tudo.contains("Aluguel Exemplo") && tudo.contains("R$ 800,00"));
    assert!(tudo.contains("Padaria Exemplo") && tudo.contains("2 vezes"));
    // Mascarado: nada de linha digitável.
    assert!(!tudo.contains("23790") && !tudo.contains("60000"), "{tudo}");

    // Mesma situação: nada repete.
    falso.limpar();
    assert_eq!(rodar_uma_vez(&ctx, hoje(), false).await.expect("rodada"), 0);
    assert!(falso.enviadas().is_empty());

    // O estado muda (o valor da conta sobe): só esse alerta volta.
    sql(
        &banco,
        &format!("UPDATE public.transactions SET amount = 150.00 WHERE user_id = '{dono}' AND description LIKE 'Boleto%'"),
    )
    .await;
    let n = rodar_uma_vez(&ctx, hoje(), false).await.expect("rodada");
    assert_eq!(n, 1, "{:?}", falso.enviadas());
    assert!(falso.enviadas()[0].1.contains("R$ 150,00"));

    // O que saiu ficou registrado: destinatário, tipo, tamanho, nível, quando. Sem texto.
    let linhas = banco
        .get()
        .await
        .expect("c")
        .query(
            "SELECT identificador, tipo, nivel_de_confianca, tamanho_bytes, entregue, enviado_em IS NOT NULL \
             FROM public.canais_saidas WHERE user_id = $1 ORDER BY enviado_em, tipo",
            &[&dono],
        )
        .await
        .expect("saídas");
    assert_eq!(linhas.len(), 4);
    for l in &linhas {
        assert_eq!(l.get::<_, String>(0), chat_dono.to_string());
        assert_eq!(l.get::<_, String>(2), "mascarado");
        assert!(l.get::<_, i32>(3) > 10);
        assert!(l.get::<_, bool>(4) && l.get::<_, bool>(5));
    }
    let tipos: BTreeSet<String> = linhas.iter().map(|l| l.get(1)).collect();
    assert!(tipos.contains("conta_a_vencer") && tipos.contains("cobranca_duplicada"));

    // Resolvido e de volta: o alerta que sumiu é esquecido e avisa de novo se voltar.
    sql(
        &banco,
        &format!("UPDATE public.transactions SET is_confirmed = true WHERE user_id = '{dono}' AND description LIKE 'Boleto%'"),
    )
    .await;
    rodar_uma_vez(&ctx, hoje(), false).await.expect("rodada");
    sql(
        &banco,
        &format!("UPDATE public.transactions SET is_confirmed = false WHERE user_id = '{dono}' AND description LIKE 'Boleto%'"),
    )
    .await;
    falso.limpar();
    assert_eq!(rodar_uma_vez(&ctx, hoje(), false).await.expect("rodada"), 1);
}

#[tokio::test]
async fn nivel_parcial_mostra_so_os_ultimos_digitos_e_a_agenda_respeita_silencio_e_dia() {
    let Some(banco) = banco().await else { return };
    let falso = telegram_falso().await;
    let dono = novo_dono(&banco).await;
    semear_livro(&banco, dono).await;
    let chat_dono = chat_novo();
    repo::vincular(&banco, dono, chat_dono)
        .await
        .expect("vincular");
    repo::definir_nivel(&banco, chat_dono, sbf_canais::confianca::Nivel::Parcial)
        .await
        .expect("nível");
    let mut ctx = contexto(&banco, &falso, dono, chat_dono);
    ctx.config.resumo_diario = true;

    // 2026-11-14 04:00 UTC = 01:00 em São Paulo: silêncio noturno, nada sai.
    let meia_noite_utc = hoje().dias_desde_1970() * 86_400;
    assert!(!passo(&ctx, meia_noite_utc + 4 * 3600).await.expect("passo"));
    assert!(falso.enviadas().is_empty());

    // 08:05 em São Paulo (11:05 UTC): roda, com o resumo antes dos avisos.
    assert!(
        passo(&ctx, meia_noite_utc + 11 * 3600 + 5 * 60)
            .await
            .expect("passo")
    );
    let msgs = falso.enviadas();
    assert!(msgs[0].1.starts_with("Resumo de 14/11"), "{:?}", msgs[0]);
    let tudo = msgs
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        tudo.contains("•••0000") || tudo.contains("•••3456"),
        "{tudo}"
    );
    assert!(!tudo.contains("23790"));

    // Na mesma data a agenda não roda de novo.
    falso.limpar();
    assert!(
        !passo(&ctx, meia_noite_utc + 15 * 3600)
            .await
            .expect("passo")
    );
    assert!(falso.enviadas().is_empty());
}
