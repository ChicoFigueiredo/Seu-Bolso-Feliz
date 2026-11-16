//! Onde os bytes dos documentos enviados ficam (spec 08). O servidor grava por este trait;
//! o pipeline Bun os lê depois pelo `storage_path` (`@sbf/storage`, bucket
//! `ingestion-originals`).
//!
//! - [`ArmazenamentoFalso`]: em memória, para os testes.
//! - [`VercelBlob`]: `PUT` na API do Vercel Blob. O token vem **só** da variável de ambiente
//!   `BLOB_READ_WRITE_TOKEN`, lida uma vez no arranque ([`VercelBlob::do_ambiente`]); vive em
//!   `Zeroizing`, fora de `Debug`, de log e de mensagem de erro.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};

use zeroize::Zeroizing;

/// O bucket do pipeline para os originais (`BUCKET_ORIGINAIS` em `workers/local-scanner`).
pub const BUCKET_DOS_ORIGINAIS: &str = "ingestion-originals";

/// A variável que guarda o token do Vercel Blob (a mesma do `@sbf/storage`).
pub const VARIAVEL_DO_TOKEN: &str = "BLOB_READ_WRITE_TOKEN";

/// Falha ao gravar. A mensagem nunca leva o token nem o conteúdo.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDeArmazenamento {
    #[error("o armazenamento não respondeu: {0}")]
    Indisponivel(String),
    #[error("o armazenamento recusou o arquivo (HTTP {0})")]
    Recusado(u16),
}

pub type Futuro<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Grava um arquivo. `chave` é o `storage_path` que vai para o banco (sem o bucket).
pub trait Armazenamento: Send + Sync + fmt::Debug {
    fn gravar<'a>(
        &'a self,
        chave: &'a str,
        bytes: &'a [u8],
        tipo_mime: &'a str,
    ) -> Futuro<'a, Result<(), ErroDeArmazenamento>>;
}

/// Em memória. Clonar compartilha o mesmo conteúdo.
#[derive(Debug, Clone, Default)]
pub struct ArmazenamentoFalso {
    dentro: Arc<Mutex<FalsoPorDentro>>,
}

#[derive(Debug, Default)]
struct FalsoPorDentro {
    arquivos: HashMap<String, (Vec<u8>, String)>,
    gravacoes: usize,
    falhar: bool,
}

impl ArmazenamentoFalso {
    pub fn novo() -> Self {
        Self::default()
    }

    /// As próximas gravações falham (ou voltam a dar certo).
    pub fn falhar(&self, sim: bool) {
        self.travar().falhar = sim;
    }

    /// Quantas gravações deram certo.
    pub fn gravacoes(&self) -> usize {
        self.travar().gravacoes
    }

    /// Os bytes e o tipo de um arquivo gravado.
    pub fn arquivo(&self, chave: &str) -> Option<(Vec<u8>, String)> {
        self.travar().arquivos.get(chave).cloned()
    }

    pub fn chaves(&self) -> Vec<String> {
        let mut c: Vec<_> = self.travar().arquivos.keys().cloned().collect();
        c.sort();
        c
    }

    fn travar(&self) -> std::sync::MutexGuard<'_, FalsoPorDentro> {
        self.dentro.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Armazenamento for ArmazenamentoFalso {
    fn gravar<'a>(
        &'a self,
        chave: &'a str,
        bytes: &'a [u8],
        tipo_mime: &'a str,
    ) -> Futuro<'a, Result<(), ErroDeArmazenamento>> {
        Box::pin(async move {
            let mut d = self.travar();
            if d.falhar {
                return Err(ErroDeArmazenamento::Indisponivel("falha de teste".into()));
            }
            d.arquivos
                .insert(chave.to_owned(), (bytes.to_vec(), tipo_mime.to_owned()));
            d.gravacoes += 1;
            Ok(())
        })
    }
}

/// Sem armazenamento configurado (sem token no ambiente): toda gravação falha com uma
/// mensagem clara, e o resto da API funciona.
#[derive(Debug, Clone, Copy)]
pub struct ArmazenamentoAusente;

impl Armazenamento for ArmazenamentoAusente {
    fn gravar<'a>(
        &'a self,
        _chave: &'a str,
        _bytes: &'a [u8],
        _tipo_mime: &'a str,
    ) -> Futuro<'a, Result<(), ErroDeArmazenamento>> {
        Box::pin(async {
            Err(ErroDeArmazenamento::Indisponivel(
                "o servidor está sem armazenamento configurado".into(),
            ))
        })
    }
}

/// O Vercel Blob por HTTP.
///
/// UNVERIFIED contra a API real: o protocolo (`PUT {base}/?pathname=`, cabeçalhos
/// `x-api-version`, `x-vercel-blob-access`, `x-add-random-suffix`, `x-allow-overwrite`,
/// `x-content-type`) foi reproduzido do `@vercel/blob` de memória, sem rede nesta rodada. A
/// primeira gravação de verdade confirma; um HTTP 4xx volta como
/// [`ErroDeArmazenamento::Recusado`] e o envio do lote fica para tentar de novo.
pub struct VercelBlob {
    cliente: reqwest::Client,
    base: String,
    token: Zeroizing<String>,
}

impl fmt::Debug for VercelBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VercelBlob")
            .field("base", &self.base)
            .field("token", &"<oculto>")
            .finish()
    }
}

/// A API do Vercel Blob.
pub const BASE_DO_VERCEL_BLOB: &str = "https://vercel.com/api/blob";

/// Prazo para abrir a conexão com o Blob.
const PRAZO_DE_CONEXAO: std::time::Duration = std::time::Duration::from_secs(10);

/// Prazo total de um PUT (o arquivo mais pesado aceito, 50 MB, em rede doméstica).
const PRAZO_TOTAL: std::time::Duration = std::time::Duration::from_secs(120);

/// A versão do protocolo que este cliente fala.
const VERSAO_DA_API: &str = "12";

impl VercelBlob {
    /// Lê o token de `BLOB_READ_WRITE_TOKEN`. `None` se a variável não existe ou está vazia.
    pub fn do_ambiente() -> Option<Self> {
        let token = std::env::var(VARIAVEL_DO_TOKEN).ok()?;
        let token = Zeroizing::new(token);
        (!token.trim().is_empty()).then(|| Self::novo(BASE_DO_VERCEL_BLOB, token))
    }

    /// Com a base e o token dados (os testes apontam a base para um servidor local).
    pub fn novo(base: &str, token: Zeroizing<String>) -> Self {
        Self::com_prazos(base, token, PRAZO_DE_CONEXAO, PRAZO_TOTAL)
    }

    /// Como [`VercelBlob::novo`], com os prazos de conexão e total escolhidos (os testes usam
    /// prazos curtos).
    pub fn com_prazos(
        base: &str,
        token: Zeroizing<String>,
        conexao: std::time::Duration,
        total: std::time::Duration,
    ) -> Self {
        let cliente = reqwest::Client::builder()
            .connect_timeout(conexao)
            .timeout(total)
            .build()
            .unwrap_or_default();
        Self {
            cliente,
            base: base.trim_end_matches('/').to_owned(),
            token,
        }
    }
}

impl Armazenamento for VercelBlob {
    fn gravar<'a>(
        &'a self,
        chave: &'a str,
        bytes: &'a [u8],
        tipo_mime: &'a str,
    ) -> Futuro<'a, Result<(), ErroDeArmazenamento>> {
        Box::pin(async move {
            let caminho = format!("{BUCKET_DOS_ORIGINAIS}/{}", chave.trim_start_matches('/'));
            let resposta = self
                .cliente
                .put(format!("{}/", self.base))
                .query(&[("pathname", caminho.as_str())])
                .bearer_auth(self.token.as_str())
                .header("x-api-version", VERSAO_DA_API)
                .header("x-vercel-blob-access", "private")
                .header("x-add-random-suffix", "0")
                .header("x-allow-overwrite", "1")
                .header("x-content-type", tipo_mime)
                .body(bytes.to_vec())
                .send()
                .await
                // O erro do reqwest pode trazer a URL; a URL não tem segredo (o token vai no
                // cabeçalho), mas a mensagem fica só com a categoria.
                .map_err(|e| {
                    ErroDeArmazenamento::Indisponivel(if e.is_timeout() {
                        "tempo esgotado".into()
                    } else if e.is_connect() {
                        "sem conexão".into()
                    } else {
                        "falha de rede".into()
                    })
                })?;
            let status = resposta.status();
            if status.is_success() {
                Ok(())
            } else {
                Err(ErroDeArmazenamento::Recusado(status.as_u16()))
            }
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use axum::Router;
    use axum::extract::{Query, State};
    use axum::http::HeaderMap;
    use axum::routing::put;

    type Visto = Arc<Mutex<Vec<(String, HeaderMap, usize)>>>;

    #[tokio::test]
    async fn vercel_blob_faz_o_put_com_token_so_no_cabecalho() {
        let visto: Visto = Arc::default();
        async fn receber(
            State(v): State<Visto>,
            Query(q): Query<HashMap<String, String>>,
            cabecalhos: HeaderMap,
            corpo: axum::body::Bytes,
        ) -> &'static str {
            v.lock().unwrap_or_else(PoisonError::into_inner).push((
                q.get("pathname").cloned().unwrap_or_default(),
                cabecalhos,
                corpo.len(),
            ));
            "{}"
        }
        let app = Router::new()
            .route("/", put(receber))
            .with_state(visto.clone());
        let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("porta");
        let porta = ouvinte.local_addr().expect("endereço").port();
        tokio::spawn(async move {
            let _ = axum::serve(ouvinte, app).await;
        });

        let blob = VercelBlob::novo(
            &format!("http://127.0.0.1:{porta}"),
            Zeroizing::new("token-ficticio-de-teste".into()),
        );
        assert!(!format!("{blob:?}").contains("token-ficticio"));
        blob.gravar("dono/doc/a.pdf", b"abc", "application/pdf")
            .await
            .expect("gravou");
        let v = visto.lock().unwrap_or_else(PoisonError::into_inner);
        let (caminho, cab, tamanho) = &v[0];
        assert_eq!(caminho, "ingestion-originals/dono/doc/a.pdf");
        assert_eq!(*tamanho, 3);
        assert_eq!(cab["authorization"], "Bearer token-ficticio-de-teste");
        assert_eq!(cab["x-content-type"], "application/pdf");
        assert_eq!(cab["x-add-random-suffix"], "0");
    }

    #[tokio::test]
    async fn vercel_blob_que_nao_responde_estoura_o_prazo_sem_vazar_o_token() {
        // Um servidor que aceita a conexão e nunca responde.
        let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("porta");
        let porta = ouvinte.local_addr().expect("endereço").port();
        tokio::spawn(async move {
            let mut seguradas = Vec::new();
            while let Ok((s, _)) = ouvinte.accept().await {
                seguradas.push(s);
            }
        });
        let blob = VercelBlob::com_prazos(
            &format!("http://127.0.0.1:{porta}"),
            Zeroizing::new("segredo-y".into()),
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(300),
        );
        let inicio = std::time::Instant::now();
        let erro = blob.gravar("a", b"x", "text/plain").await.unwrap_err();
        assert!(inicio.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(
            erro,
            ErroDeArmazenamento::Indisponivel("tempo esgotado".into())
        );
        assert!(!erro.to_string().contains("segredo-y"));
    }

    #[tokio::test]
    async fn vercel_blob_sem_servidor_nao_vaza_o_token_no_erro() {
        let blob = VercelBlob::novo("http://127.0.0.1:1", Zeroizing::new("segredo-x".into()));
        let erro = blob.gravar("a", b"x", "text/plain").await.unwrap_err();
        assert!(!erro.to_string().contains("segredo-x"));
    }
}
