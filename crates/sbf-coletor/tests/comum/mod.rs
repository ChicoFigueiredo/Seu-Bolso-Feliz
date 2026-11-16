//! Apoio dos testes: um servidor HTTP falso local (Gmail e Pluggy, com respostas gravadas à
//! mão em `tests/dados/`) e o Postgres de teste (só com `SBF_PG_TESTE=1`).
#![allow(dead_code)]

use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use deadpool_postgres::Pool;
use sbf_coletor::config::{ConfigGmail, Segredo};
use sbf_coletor::gmail::cliente::ClienteGmail;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Um pedido que o servidor falso recebeu.
#[derive(Debug, Clone)]
pub struct Pedido {
    pub metodo: String,
    pub caminho: String,
    pub consulta: String,
    pub autorizacao: Option<String>,
    pub chave_da_api: Option<String>,
    pub corpo: String,
}

impl Pedido {
    /// O valor de um parâmetro da query string (sem decodificar `%`; os testes usam valores simples).
    pub fn parametro(&self, nome: &str) -> Option<String> {
        self.consulta.split('&').find_map(|par| {
            let (k, v) = par.split_once('=')?;
            (k == nome).then(|| v.to_owned())
        })
    }
}

type Roteiro = dyn Fn(&Pedido) -> (u16, String) + Send + Sync;

struct Estado {
    pedidos: Mutex<Vec<Pedido>>,
    roteiro: Box<Roteiro>,
}

/// O servidor falso: `base` aponta para ele; `pedidos()` devolve o que chegou.
pub struct Falso {
    pub base: String,
    estado: Arc<Estado>,
}

impl Falso {
    pub fn pedidos(&self) -> Vec<Pedido> {
        self.estado
            .pedidos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn pedidos_em(&self, prefixo: &str) -> Vec<Pedido> {
        self.pedidos()
            .into_iter()
            .filter(|p| p.caminho.starts_with(prefixo))
            .collect()
    }
}

async fn tratar(State(e): State<Arc<Estado>>, req: Request<Body>) -> Response {
    let (partes, corpo) = req.into_parts();
    let corpo = axum::body::to_bytes(corpo, 4 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let pedido = Pedido {
        metodo: partes.method.to_string(),
        caminho: partes.uri.path().to_owned(),
        consulta: partes.uri.query().unwrap_or("").to_owned(),
        autorizacao: partes
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        chave_da_api: partes
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
        corpo: String::from_utf8_lossy(&corpo).into_owned(),
    };
    let (status, texto) = (e.roteiro)(&pedido);
    e.pedidos
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(pedido);
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
        .header("content-type", "application/json")
        .body(Body::from(texto))
        .unwrap_or_default()
}

pub async fn subir(roteiro: impl Fn(&Pedido) -> (u16, String) + Send + Sync + 'static) -> Falso {
    let estado = Arc::new(Estado {
        pedidos: Mutex::default(),
        roteiro: Box::new(roteiro),
    });
    let app = Router::new()
        .fallback(tratar)
        .with_state(Arc::clone(&estado));
    let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("porta");
    let porta = ouvinte.local_addr().expect("endereço").port();
    tokio::spawn(async move {
        let _ = axum::serve(ouvinte, app).await;
    });
    Falso {
        base: format!("http://127.0.0.1:{porta}"),
        estado,
    }
}

// ───────────────────────────── Postgres de teste ─────────────────────────────

/// A URL do Postgres de teste, ou `None` (e o teste pula) sem `SBF_PG_TESTE=1`.
pub fn url_de_teste() -> Option<String> {
    if std::env::var("SBF_PG_TESTE").as_deref() != Ok("1") {
        eprintln!("pulado: rode com scripts/desktop/postgres-de-teste.sh testar-coletor");
        return None;
    }
    Some(std::env::var("SBF_PG_TESTE_URL").expect("SBF_PG_TESTE_URL"))
}

pub async fn banco() -> Option<Pool> {
    let url = Zeroizing::new(url_de_teste()?);
    Some(sbf_servidor::banco::abrir_pool(&url, 4).expect("pool de teste"))
}

/// Um dono novo (e-mail único), para os testes não se enxergarem.
pub async fn novo_dono(banco: &Pool) -> Uuid {
    let cliente = banco.get().await.expect("conexão");
    let id = Uuid::new_v4();
    cliente
        .execute(
            "INSERT INTO public.users (id, email, name, role, google_sub) \
             VALUES ($1, $2, 'Dono Fictício', 'owner', $3)",
            &[&id, &format!("{id}@example.com"), &id.to_string()],
        )
        .await
        .expect("dono");
    id
}

pub fn arquivo(nome: &str) -> String {
    let caminho = format!("{}/tests/dados/{nome}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&caminho).unwrap_or_else(|e| panic!("{caminho}: {e}"))
}

// ───────────────────────────── Gmail falso ─────────────────────────────

pub const SEGREDO: &str = "segredo-do-cliente-ficticio";
pub const REFRESH: &str = "refresh-ficticio-nao-vazar";

pub fn roteiro_do_gmail(p: &Pedido) -> (u16, String) {
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

pub async fn gmail_falso() -> (Falso, ClienteGmail) {
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
