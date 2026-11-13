//! A prova do login Google no pareamento: o `id_token` é verificado contra o JWKS do
//! Google, não apenas decodificado (mesmas regras de `apps/web/src/lib/auth/google.ts`):
//! assinatura RS256, emissor do Google, audiência = o client id, validade, `nonce`
//! igual ao da estação e `email_verified`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::Mutex;

/// Os emissores aceitos, como no web.
pub const EMISSORES: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];

/// O endereço do JWKS do Google.
pub const URL_DO_JWKS: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// Um futuro em caixa (o trait precisa ser usável como objeto).
pub type Futuro<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// De onde vêm as chaves públicas. Em produção, o JWKS do Google; nos testes, um fixo.
pub trait FonteDeChaves: Send + Sync {
    /// As chaves atuais. `renovar` pede para ignorar o cache (a `kid` do token não
    /// estava nas chaves guardadas).
    fn chaves(&self, renovar: bool) -> Futuro<'_, Result<Arc<JwkSet>, ErroDoGoogle>>;
}

/// Chaves fixas, para testes e para o modo local.
#[derive(Debug)]
pub struct ChavesFixas(pub Arc<JwkSet>);

impl FonteDeChaves for ChavesFixas {
    fn chaves(&self, _renovar: bool) -> Futuro<'_, Result<Arc<JwkSet>, ErroDoGoogle>> {
        let chaves = Arc::clone(&self.0);
        Box::pin(async move { Ok(chaves) })
    }
}

/// O JWKS do Google por HTTPS, guardado por uma hora.
#[derive(Debug)]
pub struct ChavesDoGoogle {
    http: reqwest::Client,
    cache: Mutex<Option<(Instant, Arc<JwkSet>)>>,
}

const VALIDADE_DO_CACHE: Duration = Duration::from_secs(3600);

impl ChavesDoGoogle {
    pub fn nova() -> Result<Self, ErroDoGoogle> {
        let http = reqwest::Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ErroDoGoogle::Chaves(e.to_string()))?;
        Ok(Self {
            http,
            cache: Mutex::new(None),
        })
    }
}

impl FonteDeChaves for ChavesDoGoogle {
    fn chaves(&self, renovar: bool) -> Futuro<'_, Result<Arc<JwkSet>, ErroDoGoogle>> {
        Box::pin(async move {
            let mut cache = self.cache.lock().await;
            if let Some((quando, chaves)) = cache.as_ref()
                && !renovar
                && quando.elapsed() < VALIDADE_DO_CACHE
            {
                return Ok(Arc::clone(chaves));
            }
            let chaves: JwkSet = self
                .http
                .get(URL_DO_JWKS)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| ErroDoGoogle::Chaves(e.to_string()))?
                .json()
                .await
                .map_err(|e| ErroDoGoogle::Chaves(e.to_string()))?;
            let chaves = Arc::new(chaves);
            *cache = Some((Instant::now(), Arc::clone(&chaves)));
            Ok(chaves)
        })
    }
}

/// Quem o Google disse que é.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentidadeGoogle {
    pub sub: String,
    /// Em minúsculas.
    pub email: String,
    pub nome: Option<String>,
}

/// Por que o login não foi aceito. A mensagem não leva o token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDoGoogle {
    #[error("id_token inválido: {0}")]
    TokenInvalido(String),
    #[error("nonce não confere")]
    NonceDiferente,
    #[error("e-mail não verificado")]
    EmailNaoVerificado,
    #[error("o Google não devolveu e-mail")]
    SemEmail,
    #[error("não deu para obter as chaves do Google: {0}")]
    Chaves(String),
}

/// `email_verified` vem como booleano, mas tokens antigos o mandavam como texto.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Verificado {
    Booleano(bool),
    Texto(String),
}

#[derive(Debug, Deserialize)]
struct Reivindicacoes {
    sub: String,
    email: Option<String>,
    email_verified: Option<Verificado>,
    nonce: Option<String>,
    name: Option<String>,
}

/// Verifica o `id_token` de um login Google.
pub struct VerificadorGoogle {
    client_id: String,
    chaves: Box<dyn FonteDeChaves>,
}

impl std::fmt::Debug for VerificadorGoogle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerificadorGoogle")
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

impl VerificadorGoogle {
    pub fn novo(client_id: impl Into<String>, chaves: Box<dyn FonteDeChaves>) -> Self {
        Self {
            client_id: client_id.into(),
            chaves,
        }
    }

    /// Confere o token e devolve a identidade. `nonce` é o que a estação mandou no
    /// pedido de autorização.
    pub async fn verificar(
        &self,
        id_token: &str,
        nonce: &str,
    ) -> Result<IdentidadeGoogle, ErroDoGoogle> {
        let cabecalho = jsonwebtoken::decode_header(id_token)
            .map_err(|e| ErroDoGoogle::TokenInvalido(e.to_string()))?;
        if cabecalho.alg != Algorithm::RS256 {
            return Err(ErroDoGoogle::TokenInvalido("algoritmo não aceito".into()));
        }
        let kid = cabecalho
            .kid
            .ok_or_else(|| ErroDoGoogle::TokenInvalido("sem kid".into()))?;
        let mut chaves = self.chaves.chaves(false).await?;
        if chaves.find(&kid).is_none() {
            chaves = self.chaves.chaves(true).await?;
        }
        let jwk = chaves
            .find(&kid)
            .ok_or_else(|| ErroDoGoogle::TokenInvalido("kid desconhecida".into()))?;
        let chave =
            DecodingKey::from_jwk(jwk).map_err(|e| ErroDoGoogle::TokenInvalido(e.to_string()))?;

        let mut validacao = Validation::new(Algorithm::RS256);
        validacao.set_audience(&[&self.client_id]);
        validacao.set_issuer(&EMISSORES);
        validacao.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let dados = jsonwebtoken::decode::<Reivindicacoes>(id_token, &chave, &validacao)
            .map_err(|e| ErroDoGoogle::TokenInvalido(e.to_string()))?;
        let r = dados.claims;

        if r.nonce.as_deref() != Some(nonce) || nonce.is_empty() {
            return Err(ErroDoGoogle::NonceDiferente);
        }
        let verificado = match r.email_verified {
            Some(Verificado::Booleano(b)) => b,
            Some(Verificado::Texto(t)) => t == "true",
            None => false,
        };
        if !verificado {
            return Err(ErroDoGoogle::EmailNaoVerificado);
        }
        let email = r
            .email
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .ok_or(ErroDoGoogle::SemEmail)?;
        if r.sub.is_empty() {
            return Err(ErroDoGoogle::TokenInvalido("sem sub".into()));
        }
        Ok(IdentidadeGoogle {
            sub: r.sub,
            email,
            nome: r.name,
        })
    }
}
