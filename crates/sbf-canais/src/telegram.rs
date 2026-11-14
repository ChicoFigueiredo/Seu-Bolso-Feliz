//! O cliente da Bot API do Telegram, só o que o bot usa: `getUpdates` (long polling, sem
//! webhook), `sendMessage`, `getFile` e o download do arquivo.
//!
//! O token faz parte da URL da Bot API. Por isso nenhum erro leva a URL
//! (`reqwest::Error::without_url`) e o token vive em [`Segredo`].

use std::time::Duration;

use sbf_coletor::config::Segredo;
use serde::Deserialize;
use serde_json::json;

use crate::erro::ErroDeCanal;

/// O Telegram só deixa o bot baixar arquivos de até 20 MB.
pub const TAMANHO_MAXIMO_DE_DOWNLOAD: u64 = 20 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Resposta<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Mensagem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mensagem {
    pub chat: Chat,
    pub text: Option<String>,
    pub document: Option<Documento>,
    pub photo: Option<Vec<Foto>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Documento {
    pub file_id: String,
    pub file_unique_id: Option<String>,
    pub file_name: Option<String>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Foto {
    pub file_id: String,
    pub file_unique_id: Option<String>,
    pub file_size: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ArquivoNoTelegram {
    file_path: Option<String>,
    file_size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ClienteTelegram {
    http: reqwest::Client,
    base: String,
    token: Segredo,
}

/// A rede sem a URL (que leva o token).
fn rede(e: reqwest::Error) -> ErroDeCanal {
    ErroDeCanal::Rede(e.without_url().to_string())
}

impl ClienteTelegram {
    pub fn novo(base: &str, token: Segredo) -> Result<Self, ErroDeCanal> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(rede)?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            token,
        })
    }

    fn url(&self, metodo: &str) -> String {
        format!("{}/bot{}/{metodo}", self.base, self.token.expor())
    }

    async fn chamar<T: serde::de::DeserializeOwned>(
        &self,
        metodo: &str,
        corpo: serde_json::Value,
        prazo: Duration,
    ) -> Result<T, ErroDeCanal> {
        let resposta = self
            .http
            .post(self.url(metodo))
            .timeout(prazo)
            .json(&corpo)
            .send()
            .await
            .map_err(rede)?;
        let status = resposta.status();
        let texto = resposta.text().await.map_err(rede)?;
        let lido: Resposta<T> = serde_json::from_str(&texto).map_err(|_| {
            ErroDeCanal::Telegram(format!("{metodo}: HTTP {status}, resposta inesperada"))
        })?;
        match (lido.ok, lido.result) {
            (true, Some(r)) => Ok(r),
            _ => Err(ErroDeCanal::Telegram(format!(
                "{metodo}: HTTP {status}: {}",
                lido.description.unwrap_or_else(|| "sem descrição".into())
            ))),
        }
    }

    /// `getUpdates` com espera de até `espera` segundos (long polling).
    pub async fn buscar_atualizacoes(
        &self,
        a_partir_de: Option<i64>,
        espera: u64,
    ) -> Result<Vec<Update>, ErroDeCanal> {
        let mut corpo = json!({ "timeout": espera, "allowed_updates": ["message"] });
        if let Some(o) = a_partir_de {
            corpo["offset"] = json!(o);
        }
        self.chamar("getUpdates", corpo, Duration::from_secs(espera + 15))
            .await
    }

    pub async fn enviar_texto(&self, chat: i64, texto: &str) -> Result<(), ErroDeCanal> {
        let corpo = json!({
            "chat_id": chat,
            "text": texto,
            "disable_web_page_preview": true,
        });
        self.chamar::<serde_json::Value>("sendMessage", corpo, Duration::from_secs(30))
            .await
            .map(|_| ())
    }

    /// O arquivo que o Telegram guardou: baixa os bytes (até 20 MB).
    pub async fn baixar(&self, file_id: &str) -> Result<Vec<u8>, ErroDeCanal> {
        let arquivo: ArquivoNoTelegram = self
            .chamar(
                "getFile",
                json!({ "file_id": file_id }),
                Duration::from_secs(30),
            )
            .await?;
        if arquivo
            .file_size
            .is_some_and(|n| n > TAMANHO_MAXIMO_DE_DOWNLOAD)
        {
            return Err(ErroDeCanal::Entrada("o arquivo passa de 20 MB".into()));
        }
        let caminho = arquivo
            .file_path
            .ok_or_else(|| ErroDeCanal::Telegram("getFile sem file_path".into()))?;
        let url = format!("{}/file/bot{}/{caminho}", self.base, self.token.expor());
        let resposta = self
            .http
            .get(url)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(rede)?;
        if !resposta.status().is_success() {
            return Err(ErroDeCanal::Telegram(format!(
                "download: HTTP {}",
                resposta.status()
            )));
        }
        let bytes = resposta.bytes().await.map_err(rede)?;
        if bytes.len() as u64 > TAMANHO_MAXIMO_DE_DOWNLOAD {
            return Err(ErroDeCanal::Entrada("o arquivo passa de 20 MB".into()));
        }
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[tokio::test]
    async fn erro_de_rede_nao_vaza_o_token() {
        // Porta 9 (discard) em loopback: recusa a conexão.
        let c = ClienteTelegram::new_para_teste();
        let e = c.enviar_texto(1, "oi").await.expect_err("sem servidor");
        let texto = format!("{e} {e:?}");
        assert!(!texto.contains("123456:token-ficticio"), "{texto}");
    }

    impl ClienteTelegram {
        fn new_para_teste() -> Self {
            Self::novo("http://127.0.0.1:9", Segredo::novo("123456:token-ficticio"))
                .expect("cliente")
        }
    }
}
