//! A configuração por variável de ambiente (nunca por argv, que aparece em `ps`).
//!
//! Os nomes são os mesmos dos workers Bun, para a VPS poder reaproveitar o
//! `EnvironmentFile` que já tem.

use std::fmt;

use zeroize::Zeroizing;

use crate::erro::ErroDeColeta;

/// Um valor secreto: vive em `Zeroizing`, e `Debug` não o mostra.
#[derive(Clone)]
pub struct Segredo(Zeroizing<String>);

impl Segredo {
    pub fn novo(valor: impl Into<String>) -> Self {
        Self(Zeroizing::new(valor.into()))
    }

    /// O valor em claro. Só para montar o corpo ou o cabeçalho de um pedido.
    pub fn expor(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Segredo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<oculto>")
    }
}

/// Lê uma variável obrigatória e não vazia.
pub fn exigir(nome: &str) -> Result<String, ErroDeColeta> {
    match std::env::var(nome) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(ErroDeColeta::Configuracao(format!(
            "a variável {nome} é obrigatória"
        ))),
    }
}

/// Lê uma variável opcional (vazia conta como ausente).
pub fn opcional(nome: &str) -> Option<String> {
    std::env::var(nome).ok().filter(|v| !v.trim().is_empty())
}

pub const GMAIL_API: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
pub const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
pub const PLUGGY_API: &str = "https://api.pluggy.ai";

/// As credenciais OAuth2 do Gmail e os endereços (os testes apontam para um servidor local).
#[derive(Debug, Clone)]
pub struct ConfigGmail {
    pub client_id: String,
    pub client_secret: Segredo,
    pub refresh_token: Segredo,
    pub base_api: String,
    pub url_do_token: String,
}

impl ConfigGmail {
    /// `GOOGLE_MAIL_CLIENT_ID`, `GOOGLE_MAIL_CLIENT_SECRET`, `GMAIL_REFRESH_TOKEN`.
    pub fn do_ambiente() -> Result<Self, ErroDeColeta> {
        Ok(Self::com_enderecos_padrao(
            exigir("GOOGLE_MAIL_CLIENT_ID")?,
            Segredo::novo(exigir("GOOGLE_MAIL_CLIENT_SECRET")?),
            Segredo::novo(exigir("GMAIL_REFRESH_TOKEN")?),
        ))
    }

    pub fn com_enderecos_padrao(
        client_id: String,
        client_secret: Segredo,
        refresh_token: Segredo,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            refresh_token,
            base_api: GMAIL_API.to_owned(),
            url_do_token: GOOGLE_TOKEN.to_owned(),
        }
    }
}

/// As credenciais da Pluggy.
#[derive(Debug, Clone)]
pub struct ConfigPluggy {
    pub client_id: String,
    pub client_secret: Segredo,
    pub base_api: String,
}

impl ConfigPluggy {
    /// `PLUGGY_CLIENT_ID`, `PLUGGY_CLIENT_SECRET`.
    pub fn do_ambiente() -> Result<Self, ErroDeColeta> {
        Ok(Self {
            client_id: exigir("PLUGGY_CLIENT_ID")?,
            client_secret: Segredo::novo(exigir("PLUGGY_CLIENT_SECRET")?),
            base_api: PLUGGY_API.to_owned(),
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn debug_nao_mostra_segredo() {
        let c = ConfigGmail::com_enderecos_padrao(
            "id-publico".into(),
            Segredo::novo("segredo-ficticio-1"),
            Segredo::novo("refresh-ficticio-2"),
        );
        let texto = format!("{c:?}");
        assert!(texto.contains("id-publico"));
        assert!(!texto.contains("segredo-ficticio-1"));
        assert!(!texto.contains("refresh-ficticio-2"));
        assert!(texto.contains("<oculto>"));
    }
}
