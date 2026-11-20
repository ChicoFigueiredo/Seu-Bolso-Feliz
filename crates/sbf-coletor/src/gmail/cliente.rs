//! O cliente da API do Gmail com OAuth2 por refresh token (`gmail-client.ts`).
//!
//! O access token é renovado sozinho, 60 s antes de expirar. O refresh token e o client secret
//! só aparecem no corpo do pedido ao endpoint de token; nenhuma mensagem de erro os carrega.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::mensagem::{MensagemDoGmail, decodificar_base64url};
use crate::config::{ConfigGmail, Segredo};
use crate::erro::{ErroDeColeta, aparar};

const SERVICO: &str = "gmail";
const FOLGA_DO_TOKEN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize)]
pub struct Rotulo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReferenciaDeMensagem {
    pub id: String,
}

/// Uma página de `GET /messages`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginaDeMensagens {
    #[serde(default)]
    pub messages: Vec<ReferenciaDeMensagem>,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct RespostaDoToken {
    access_token: String,
    #[serde(default = "uma_hora")]
    expires_in: u64,
}

fn uma_hora() -> u64 {
    3600
}

pub struct ClienteGmail {
    http: reqwest::Client,
    config: ConfigGmail,
    token: Mutex<Option<(Segredo, Instant)>>,
}

impl std::fmt::Debug for ClienteGmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClienteGmail")
            .field("base_api", &self.config.base_api)
            .finish_non_exhaustive()
    }
}

impl ClienteGmail {
    pub fn novo(config: ConfigGmail) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            http,
            config,
            token: Mutex::new(None),
        }
    }

    fn token_valido(&self) -> Option<String> {
        let guarda = self.token.lock().unwrap_or_else(|e| e.into_inner());
        guarda
            .as_ref()
            .filter(|(_, expira)| Instant::now() < *expira)
            .map(|(t, _)| t.expor().to_owned())
    }

    /// Um access token válido, renovando se expirou.
    async fn access_token(&self) -> Result<String, ErroDeColeta> {
        if let Some(t) = self.token_valido() {
            return Ok(t);
        }
        let resposta = self
            .http
            .post(&self.config.url_do_token)
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.expor()),
                ("refresh_token", self.config.refresh_token.expor()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|e| ErroDeColeta::Rede {
                servico: SERVICO,
                detalhe: sem_url(e),
            })?;
        let status = resposta.status();
        if !status.is_success() {
            let corpo = resposta.text().await.unwrap_or_default();
            return Err(ErroDeColeta::Api {
                servico: SERVICO,
                status: status.as_u16(),
                caminho: "renovação do access token".to_owned(),
                corpo: aparar(&corpo),
            });
        }
        let dados: RespostaDoToken = resposta.json().await.map_err(|_| ErroDeColeta::Resposta {
            servico: SERVICO,
            detalhe: "a resposta do token não tem access_token".to_owned(),
        })?;
        let vale_ate =
            Instant::now() + Duration::from_secs(dados.expires_in).saturating_sub(FOLGA_DO_TOKEN);
        let token = dados.access_token;
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Segredo::novo(token.clone()), vale_ate));
        Ok(token)
    }

    async fn pedir<T: DeserializeOwned>(
        &self,
        caminho: &str,
        parametros: &[(&str, &str)],
    ) -> Result<T, ErroDeColeta> {
        let token = self.access_token().await?;
        let resposta = self
            .http
            .get(format!("{}{caminho}", self.config.base_api))
            .query(parametros)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| ErroDeColeta::Rede {
                servico: SERVICO,
                detalhe: sem_url(e),
            })?;
        let status = resposta.status();
        if !status.is_success() {
            let corpo = resposta.text().await.unwrap_or_default();
            return Err(ErroDeColeta::Api {
                servico: SERVICO,
                status: status.as_u16(),
                caminho: caminho.to_owned(),
                corpo: aparar(&corpo),
            });
        }
        resposta.json().await.map_err(|e| ErroDeColeta::Resposta {
            servico: SERVICO,
            detalhe: format!("{caminho}: {}", sem_url(e)),
        })
    }

    /// O e-mail da conta autorizada (`users/me/profile`).
    pub async fn email_da_conta(&self) -> Result<String, ErroDeColeta> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PerfilDoGmail {
            email_address: String,
        }
        Ok(self
            .pedir::<PerfilDoGmail>("/profile", &[])
            .await?
            .email_address)
    }

    pub async fn listar_rotulos(&self) -> Result<Vec<Rotulo>, ErroDeColeta> {
        #[derive(Deserialize)]
        struct RotulosDaConta {
            #[serde(default)]
            labels: Vec<Rotulo>,
        }
        Ok(self.pedir::<RotulosDaConta>("/labels", &[]).await?.labels)
    }

    /// O id de um rótulo pelo nome (sem diferenciar maiúsculas).
    pub async fn achar_rotulo(&self, nome: &str) -> Result<Option<String>, ErroDeColeta> {
        Ok(self
            .listar_rotulos()
            .await?
            .into_iter()
            .find(|r| r.name.to_lowercase() == nome.to_lowercase())
            .map(|r| r.id))
    }

    /// Lista mensagens por rótulo e/ou consulta, uma página por vez.
    pub async fn listar_mensagens(
        &self,
        rotulo: Option<&str>,
        consulta: Option<&str>,
        maximo: u32,
        pagina: Option<&str>,
    ) -> Result<PaginaDeMensagens, ErroDeColeta> {
        let maximo = maximo.to_string();
        let mut p: Vec<(&str, &str)> = vec![("maxResults", &maximo)];
        if let Some(r) = rotulo {
            p.push(("labelIds", r));
        }
        if let Some(q) = consulta.filter(|q| !q.is_empty()) {
            p.push(("q", q));
        }
        if let Some(t) = pagina {
            p.push(("pageToken", t));
        }
        self.pedir("/messages", &p).await
    }

    /// A mensagem completa (sem os bytes dos anexos).
    pub async fn mensagem(&self, id: &str) -> Result<MensagemDoGmail, ErroDeColeta> {
        self.pedir(&format!("/messages/{id}"), &[("format", "full")])
            .await
    }

    /// Os bytes de um anexo.
    pub async fn anexo(
        &self,
        id_da_mensagem: &str,
        id_do_anexo: &str,
    ) -> Result<Vec<u8>, ErroDeColeta> {
        #[derive(Deserialize)]
        struct AnexoDaApi {
            #[serde(default)]
            data: String,
        }
        let a: AnexoDaApi = self
            .pedir(
                &format!("/messages/{id_da_mensagem}/attachments/{id_do_anexo}"),
                &[],
            )
            .await?;
        decodificar_base64url(&a.data)
    }
}

/// A mensagem de um erro do `reqwest` sem a URL (que pode levar parâmetros).
pub(crate) fn sem_url(e: reqwest::Error) -> String {
    e.without_url().to_string()
}
