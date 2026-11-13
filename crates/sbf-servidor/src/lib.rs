//! O modo `sbf servidor` (spec 07, ADR-013): a API HTTP que é a única autoridade do
//! livro-caixa. Lê o Postgres (Neon) **sempre filtrando pelo dono** da credencial, fala
//! o contrato de `sbf-contrato` e roda em HTTP simples em `127.0.0.1`: o TLS fica com o
//! Caddy na VPS (grill Q1).
//!
//! Só este crate tem a URL do banco, que chega por `SBF_DATABASE_URL` e nunca por argv.

pub mod alteracoes;
pub mod banco;
pub mod credencial;
pub mod dinheiro;
pub mod estacoes;
pub mod google;
pub mod http;
pub mod livro;
mod valores;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use deadpool_postgres::Pool;
use sbf_contrato::{Colecao, Estado, VERSAO_DO_CONTRATO};
use zeroize::Zeroizing;

use crate::banco::sql_instante;
use crate::estacoes::{EstacaoAutenticada, ler_estacao};
use crate::google::VerificadorGoogle;
use crate::http::{ErroHttp, com_etag, contratos_aceitos};

/// O dia de corte quando o dono não escolheu um (grill Q14).
pub const DIA_DE_CORTE_PADRAO: u8 = 20;

/// O fuso do ciclo.
pub const FUSO_DO_CICLO: &str = "America/Sao_Paulo";

/// O que toda rota recebe.
#[derive(Clone, Debug)]
pub struct EstadoDoServidor {
    pub banco: Pool,
    pub google: Arc<VerificadorGoogle>,
    /// A versão das regras: a versão do binário (ADR-012).
    pub regras: String,
}

/// As rotas da API, com a checagem de contrato e o log de acesso.
pub fn rotas(estado: EstadoDoServidor) -> Router {
    let api = Router::new()
        .route("/api/{versao}/estado", get(ver_estado))
        .route("/api/{versao}/livro/{colecao}", get(ver_colecao))
        .route("/api/{versao}/estacoes/pareamento", post(estacoes::parear))
        .route("/api/{versao}/estacoes", get(estacoes::listar))
        .route(
            "/api/{versao}/estacoes/{id}/revogar",
            post(estacoes::revogar),
        )
        .route("/api/{versao}/alteracoes", post(alteracoes::receber))
        .route_layer(axum::middleware::from_fn(http::exigir_contrato));
    Router::new()
        .merge(api)
        .fallback(|| async { ErroHttp::nao_encontrado("recurso desconhecido") })
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(axum::middleware::from_fn(http::registrar_acesso))
        .with_state(estado)
}

/// `GET /estado`.
async fn ver_estado(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
) -> Result<axum::Json<Estado>, ErroHttp> {
    let cliente = estado.banco.get().await?;
    let linha = cliente
        .query_one(
            concat!(
                "SELECT ",
                sql_instante!("now()"),
                " AS agora, (SELECT financial_cycle_start_day FROM public.user_financial_preferences \
                 WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 1) AS dia, \
                 (SELECT count(*) FROM public.ingestion_jobs WHERE user_id = $1 \
                 AND status IN ('pending_review', 'drafted')) AS a_revisar"
            ),
            &[&quem.dono],
        )
        .await?;
    let agora: String = linha.try_get("agora")?;
    let dia: Option<i32> = linha.try_get("dia")?;
    let a_revisar: i64 = linha.try_get("a_revisar")?;
    let dia_de_corte = dia
        .and_then(|d| u8::try_from(d).ok())
        .filter(|d| (1..=31).contains(d))
        .unwrap_or(DIA_DE_CORTE_PADRAO);
    let estacao = ler_estacao(&**cliente, quem.dono, quem.id)
        .await?
        .ok_or_else(ErroHttp::nao_autenticado)?;
    Ok(axum::Json(Estado {
        contrato: VERSAO_DO_CONTRATO,
        contratos_aceitos: contratos_aceitos(),
        regras: estado.regras.clone(),
        hora_do_servidor: agora,
        dia_de_corte,
        fuso: FUSO_DO_CICLO.into(),
        documentos_a_revisar: u32::try_from(a_revisar).unwrap_or(u32::MAX),
        estacao,
    }))
}

/// `GET /livro/{colecao}`: a coleção inteira, com `ETag` (grill Q5).
async fn ver_colecao(
    State(estado): State<EstadoDoServidor>,
    quem: EstacaoAutenticada,
    Path((_versao, colecao)): Path<(String, String)>,
    cabecalhos: HeaderMap,
) -> Result<Response, ErroHttp> {
    let colecao = Colecao::do_caminho(&colecao)
        .ok_or_else(|| ErroHttp::nao_encontrado("coleção desconhecida"))?;
    let cliente = estado.banco.get().await?;
    let linhas = livro::ler_colecao(&**cliente, colecao, quem.dono).await?;
    let corpo = serde_json::to_vec(&linhas).map_err(ErroHttp::interno)?;
    Ok(com_etag(&cabecalhos, corpo).into_response())
}

/// A configuração do `sbf servidor`. A URL do banco nunca aparece em `Debug`.
pub struct Configuracao {
    pub endereco: SocketAddr,
    pub url_do_banco: Zeroizing<String>,
    /// O client id OAuth do Google (a audiência do `id_token`).
    pub google_client_id: String,
    pub regras: String,
}

impl std::fmt::Debug for Configuracao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Configuracao")
            .field("endereco", &self.endereco)
            .field("url_do_banco", &"<oculta>")
            .field("google_client_id", &self.google_client_id)
            .field("regras", &self.regras)
            .finish()
    }
}

/// Falha ao subir o servidor ou ao rodar um comando de administração.
#[derive(Debug, thiserror::Error)]
pub enum ErroDoServidor {
    #[error(transparent)]
    Banco(#[from] banco::ErroDeConexao),
    #[error("o banco não respondeu ou falta a migração 0004_estacoes: {0}")]
    BancoForaDoAr(String),
    #[error(transparent)]
    Google(#[from] google::ErroDoGoogle),
    #[error("não deu para escutar em {0}: {1}")]
    Escuta(SocketAddr, std::io::Error),
    #[error("o servidor parou com erro: {0}")]
    Http(std::io::Error),
    #[error("{0}")]
    Comando(String),
}

/// Abre o pool e confere que o banco responde e já tem a tabela de estações.
pub async fn abrir_banco(url: &Zeroizing<String>) -> Result<Pool, ErroDoServidor> {
    let banco = banco::abrir_pool(url, 8)?;
    let cliente = banco
        .get()
        .await
        .map_err(|e| ErroDoServidor::BancoForaDoAr(e.to_string()))?;
    cliente
        .query("SELECT 1 FROM public.estacoes LIMIT 1", &[])
        .await
        .map_err(|e| ErroDoServidor::BancoForaDoAr(e.to_string()))?;
    drop(cliente);
    Ok(banco)
}

/// Sobe o servidor e atende até receber Ctrl+C ou SIGTERM.
pub async fn servir(config: Configuracao) -> Result<(), ErroDoServidor> {
    let banco = abrir_banco(&config.url_do_banco).await?;
    let google = VerificadorGoogle::novo(
        config.google_client_id.clone(),
        Box::new(google::ChavesDoGoogle::nova()?),
    );
    let estado = EstadoDoServidor {
        banco,
        google: Arc::new(google),
        regras: config.regras.clone(),
    };
    let ouvinte = tokio::net::TcpListener::bind(config.endereco)
        .await
        .map_err(|e| ErroDoServidor::Escuta(config.endereco, e))?;
    eprintln!(
        "sbf servidor: contrato v{VERSAO_DO_CONTRATO}, regras {}, escutando em http://{}",
        config.regras, config.endereco
    );
    axum::serve(ouvinte, rotas(estado))
        .with_graceful_shutdown(sinal_de_parada())
        .await
        .map_err(ErroDoServidor::Http)
}

async fn sinal_de_parada() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let termino = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let termino = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = termino => {},
    }
    eprintln!("sbf servidor: parando");
}
