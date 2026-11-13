//! A prova do login Google no pareamento: o `id_token` é verificado contra o JWKS do
//! Google, não apenas decodificado (mesmas regras de `apps/web/src/lib/auth/google.ts`):
//! assinatura RS256, emissor do Google, audiência = o client id, validade, `nonce`
//! igual ao da estação e `email_verified`.
//!
//! A assinatura é conferida pelo `ring` (o mesmo do TLS), só com a chave **pública**:
//! o servidor nunca tem chave privada RSA.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use serde::Deserialize;
use tokio::sync::Mutex;

/// Os emissores aceitos, como no web.
pub const EMISSORES: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];

/// O endereço do JWKS do Google.
pub const URL_DO_JWKS: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// Folga de relógio para `exp` e `nbf`, em segundos.
const FOLGA_DE_RELOGIO: u64 = 60;

/// Um JWKS: as chaves públicas do emissor.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

/// Uma chave do JWKS. Só RSA interessa.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub kid: Option<String>,
    pub alg: Option<String>,
    /// Módulo, base64url sem preenchimento.
    pub n: Option<String>,
    /// Expoente, base64url sem preenchimento.
    pub e: Option<String>,
}

impl Jwks {
    fn rsa(&self, kid: &str) -> Option<&Jwk> {
        self.keys
            .iter()
            .find(|k| k.kty == "RSA" && k.kid.as_deref() == Some(kid))
    }
}

/// Um futuro em caixa (o trait precisa ser usável como objeto).
pub type Futuro<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// De onde vêm as chaves públicas. Em produção, o JWKS do Google; nos testes, um fixo.
pub trait FonteDeChaves: Send + Sync {
    /// As chaves atuais. `renovar` pede para ignorar o cache (a `kid` do token não
    /// estava nas chaves guardadas).
    fn chaves(&self, renovar: bool) -> Futuro<'_, Result<Arc<Jwks>, ErroDoGoogle>>;
}

/// Chaves fixas, para testes.
#[derive(Debug)]
pub struct ChavesFixas(pub Arc<Jwks>);

impl FonteDeChaves for ChavesFixas {
    fn chaves(&self, _renovar: bool) -> Futuro<'_, Result<Arc<Jwks>, ErroDoGoogle>> {
        let chaves = Arc::clone(&self.0);
        Box::pin(async move { Ok(chaves) })
    }
}

/// O JWKS do Google por HTTPS, guardado por uma hora.
#[derive(Debug)]
pub struct ChavesDoGoogle {
    http: reqwest::Client,
    cache: Mutex<Option<(Instant, Arc<Jwks>)>>,
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
    fn chaves(&self, renovar: bool) -> Futuro<'_, Result<Arc<Jwks>, ErroDoGoogle>> {
        Box::pin(async move {
            let mut cache = self.cache.lock().await;
            if let Some((quando, chaves)) = cache.as_ref()
                && !renovar
                && quando.elapsed() < VALIDADE_DO_CACHE
            {
                return Ok(Arc::clone(chaves));
            }
            let chaves: Jwks = self
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

fn invalido(motivo: &str) -> ErroDoGoogle {
    ErroDoGoogle::TokenInvalido(motivo.into())
}

#[derive(Debug, Deserialize)]
struct Cabecalho {
    alg: String,
    kid: Option<String>,
}

/// `aud` pode ser um texto ou uma lista.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Audiencia {
    Uma(String),
    Varias(Vec<String>),
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
    iss: String,
    aud: Audiencia,
    exp: u64,
    nbf: Option<u64>,
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

fn base64url(parte: &str) -> Result<Vec<u8>, ErroDoGoogle> {
    URL_SAFE_NO_PAD
        .decode(parte)
        .map_err(|_| invalido("base64url inválido"))
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
        let mut partes = id_token.split('.');
        let (Some(cab), Some(corpo), Some(assinatura), None) =
            (partes.next(), partes.next(), partes.next(), partes.next())
        else {
            return Err(invalido("não é um JWT"));
        };
        let cabecalho: Cabecalho =
            serde_json::from_slice(&base64url(cab)?).map_err(|_| invalido("cabeçalho ilegível"))?;
        if cabecalho.alg != "RS256" {
            return Err(invalido("algoritmo não aceito"));
        }
        let kid = cabecalho.kid.ok_or_else(|| invalido("sem kid"))?;

        let mut chaves = self.chaves.chaves(false).await?;
        if chaves.rsa(&kid).is_none() {
            chaves = self.chaves.chaves(true).await?;
        }
        let jwk = chaves
            .rsa(&kid)
            .ok_or_else(|| invalido("kid desconhecida"))?;
        if jwk.alg.as_deref().is_some_and(|a| a != "RS256") {
            return Err(invalido("a chave não é RS256"));
        }
        let publica = RsaPublicKeyComponents {
            n: base64url(jwk.n.as_deref().ok_or_else(|| invalido("chave sem n"))?)?,
            e: base64url(jwk.e.as_deref().ok_or_else(|| invalido("chave sem e"))?)?,
        };
        let assinado = &id_token[..cab.len() + 1 + corpo.len()];
        publica
            .verify(
                &RSA_PKCS1_2048_8192_SHA256,
                assinado.as_bytes(),
                &base64url(assinatura)?,
            )
            .map_err(|_| invalido("assinatura não confere"))?;

        // Só depois da assinatura o conteúdo merece ser lido.
        let r: Reivindicacoes = serde_json::from_slice(&base64url(corpo)?)
            .map_err(|_| invalido("reivindicações ilegíveis"))?;
        if !EMISSORES.contains(&r.iss.as_str()) {
            return Err(invalido("emissor não é o Google"));
        }
        let audiencia_certa = match &r.aud {
            Audiencia::Uma(a) => *a == self.client_id,
            Audiencia::Varias(v) => v.contains(&self.client_id),
        };
        if !audiencia_certa {
            return Err(invalido("audiência não é este servidor"));
        }
        let agora = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalido("relógio do servidor antes de 1970"))?
            .as_secs();
        if r.exp.saturating_add(FOLGA_DE_RELOGIO) < agora {
            return Err(invalido("expirado"));
        }
        if r.nbf
            .is_some_and(|nbf| nbf > agora.saturating_add(FOLGA_DE_RELOGIO))
        {
            return Err(invalido("ainda não vale"));
        }

        if nonce.is_empty() || r.nonce.as_deref() != Some(nonce) {
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
            return Err(invalido("sem sub"));
        }
        Ok(IdentidadeGoogle {
            sub: r.sub,
            email,
            nome: r.name,
        })
    }
}
