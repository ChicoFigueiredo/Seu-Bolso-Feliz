//! O que é comum às rotas: o erro da API, a versão do contrato no caminho, o `ETag` e o
//! log de acesso (sem corpo e sem cabeçalhos).

use std::time::Instant;

use axum::Json;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sbf_contrato::{CodigoDeErro, ErroDaApi, PRIMEIRA_VERSAO_DO_CONTRATO, VERSAO_DO_CONTRATO};
use sha2::{Digest, Sha256};

/// Um erro que vira resposta: status + [`ErroDaApi`].
#[derive(Debug)]
pub struct ErroHttp {
    pub status: StatusCode,
    pub corpo: ErroDaApi,
}

impl ErroHttp {
    pub fn novo(status: StatusCode, codigo: CodigoDeErro, mensagem: impl Into<String>) -> Self {
        Self {
            status,
            corpo: ErroDaApi {
                codigo,
                mensagem: mensagem.into(),
                contratos_aceitos: None,
            },
        }
    }

    pub fn nao_autenticado() -> Self {
        Self::novo(
            StatusCode::UNAUTHORIZED,
            CodigoDeErro::NaoAutenticado,
            "credencial ausente ou desconhecida",
        )
    }

    pub fn estacao_revogada() -> Self {
        Self::novo(
            StatusCode::UNAUTHORIZED,
            CodigoDeErro::EstacaoRevogada,
            "esta estação foi revogada; pareie de novo para voltar a sincronizar",
        )
    }

    pub fn nao_encontrado(mensagem: impl Into<String>) -> Self {
        Self::novo(StatusCode::NOT_FOUND, CodigoDeErro::NaoEncontrado, mensagem)
    }

    pub fn pedido_invalido(mensagem: impl Into<String>) -> Self {
        Self::novo(
            StatusCode::UNPROCESSABLE_ENTITY,
            CodigoDeErro::PedidoInvalido,
            mensagem,
        )
    }

    /// Falha interna: o detalhe vai para o log do servidor, não para a estação.
    pub fn interno(detalhe: impl std::fmt::Display) -> Self {
        eprintln!("sbf servidor: erro interno: {detalhe}");
        Self::novo(
            StatusCode::INTERNAL_SERVER_ERROR,
            CodigoDeErro::Interno,
            "falha no servidor; tente de novo mais tarde",
        )
    }
}

impl IntoResponse for ErroHttp {
    fn into_response(self) -> Response {
        (self.status, Json(self.corpo)).into_response()
    }
}

impl From<tokio_postgres::Error> for ErroHttp {
    fn from(e: tokio_postgres::Error) -> Self {
        ErroHttp::interno(format_args!("banco: {e}"))
    }
}

impl From<deadpool_postgres::PoolError> for ErroHttp {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        ErroHttp::interno(format_args!("pool do banco: {e}"))
    }
}

impl From<crate::banco::ErroDeLeitura> for ErroHttp {
    fn from(e: crate::banco::ErroDeLeitura) -> Self {
        ErroHttp::interno(format_args!("leitura: {e}"))
    }
}

/// As versões de contrato aceitas: a atual e a anterior, nunca abaixo da primeira (Q4).
pub fn contratos_aceitos() -> Vec<u32> {
    let menor = VERSAO_DO_CONTRATO
        .saturating_sub(1)
        .max(PRIMEIRA_VERSAO_DO_CONTRATO);
    (menor..=VERSAO_DO_CONTRATO).collect()
}

/// A versão do contrato do pedido, tirada do caminho `/api/v{N}/…`. Fica nas extensões
/// do pedido para as rotas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContratoDoPedido(pub u32);

/// Lê `/api/v{N}/…`. `None` quando o caminho não tem esse formato.
pub fn versao_do_caminho(caminho: &str) -> Option<u32> {
    let resto = caminho.strip_prefix("/api/v")?;
    let numero = resto.split('/').next()?;
    if numero.is_empty() || !numero.bytes().all(|b| b.is_ascii_digit()) || numero.len() > 6 {
        return None;
    }
    numero.parse().ok()
}

/// Middleware: recusa contrato fora da janela aceita antes de qualquer outra coisa.
pub async fn exigir_contrato(mut pedido: Request, proximo: Next) -> Response {
    let Some(versao) = versao_do_caminho(pedido.uri().path()) else {
        return ErroHttp::nao_encontrado("recurso desconhecido").into_response();
    };
    let aceitos = contratos_aceitos();
    if !aceitos.contains(&versao) {
        let (codigo, mensagem) = if versao < aceitos[0] {
            (
                CodigoDeErro::ContratoAntigo,
                "o app desta estação é antigo demais para este servidor: atualize o app",
            )
        } else {
            (
                CodigoDeErro::ContratoMaisNovo,
                "o app desta estação é mais novo que o servidor: o servidor ainda não foi atualizado",
            )
        };
        let mut erro = ErroHttp::novo(StatusCode::UPGRADE_REQUIRED, codigo, mensagem);
        erro.corpo.contratos_aceitos = Some(aceitos);
        return erro.into_response();
    }
    pedido.extensions_mut().insert(ContratoDoPedido(versao));
    proximo.run(pedido).await
}

/// Middleware: uma linha de log por pedido, sem corpo e sem cabeçalhos.
pub async fn registrar_acesso(pedido: Request, proximo: Next) -> Response {
    let inicio = Instant::now();
    let metodo = pedido.method().clone();
    let caminho = pedido.uri().path().to_owned();
    let resposta = proximo.run(pedido).await;
    eprintln!(
        "sbf servidor: {metodo} {caminho} {} {}ms",
        resposta.status().as_u16(),
        inicio.elapsed().as_millis()
    );
    resposta
}

/// O `ETag` forte de um corpo: o SHA-256 dele.
pub fn etag_de(corpo: &[u8]) -> String {
    let hash = Sha256::digest(corpo);
    let mut s = String::with_capacity(2 + 64);
    s.push('"');
    for b in hash {
        s.push_str(&format!("{b:02x}"));
    }
    s.push('"');
    s
}

/// `If-None-Match` casa com o `ETag` (lista separada por vírgula, `W/` ou `*`).
pub fn casa_com_etag(cabecalhos: &HeaderMap, etag: &str) -> bool {
    cabecalhos
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .any(|candidato| candidato == "*" || candidato.trim_start_matches("W/") == etag)
}

/// A resposta de uma coleção: `200` com o corpo e o `ETag`, ou `304` sem corpo.
pub fn com_etag(cabecalhos: &HeaderMap, corpo: Vec<u8>) -> Response {
    let etag = etag_de(&corpo);
    let valor = HeaderValue::from_str(&etag).unwrap_or(HeaderValue::from_static("\"\""));
    let cache = HeaderValue::from_static("private, no-cache");
    if casa_com_etag(cabecalhos, &etag) {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, valor), (header::CACHE_CONTROL, cache)],
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [
            (header::ETAG, valor),
            (header::CACHE_CONTROL, cache),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
        ],
        corpo,
    )
        .into_response()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn versao_no_caminho() {
        assert_eq!(versao_do_caminho("/api/v1/estado"), Some(1));
        assert_eq!(versao_do_caminho("/api/v12/livro/contas"), Some(12));
        assert_eq!(versao_do_caminho("/api/v0/estado"), Some(0));
        assert_eq!(versao_do_caminho("/api/vx/estado"), None);
        assert_eq!(versao_do_caminho("/api/v/estado"), None);
        assert_eq!(versao_do_caminho("/outra/v1"), None);
        assert_eq!(versao_do_caminho("/api/v99999999/estado"), None);
    }

    #[test]
    fn aceita_a_atual_e_a_anterior_sem_passar_da_primeira() {
        let aceitos = contratos_aceitos();
        assert!(aceitos.contains(&VERSAO_DO_CONTRATO));
        assert!(aceitos.len() <= 2);
        assert!(aceitos.iter().all(|v| *v >= PRIMEIRA_VERSAO_DO_CONTRATO));
        assert!(!aceitos.contains(&0));
    }

    #[test]
    fn etag_muda_com_o_corpo_e_casa_com_if_none_match() {
        let a = etag_de(b"[1]");
        let b = etag_de(b"[2]");
        assert_ne!(a, b);
        assert!(a.starts_with('"') && a.ends_with('"'));
        let mut h = HeaderMap::new();
        assert!(!casa_com_etag(&h, &a));
        h.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&b).unwrap());
        assert!(!casa_com_etag(&h, &a));
        h.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_str(&format!("{b}, W/{a}")).unwrap(),
        );
        assert!(casa_com_etag(&h, &a));
        h.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        assert!(casa_com_etag(&h, &a));
    }
}
