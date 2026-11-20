//! O cliente do Gmail atrás de um trait, para a coleta ser testável sem rede.
//!
//! O real é o [`ClienteGmail`] do `sbf-coletor` (OAuth2 com refresh token, os mesmos
//! endereços e o mesmo tratamento de erro do `sbf coletor-vps gmail`), numa thread só, com um
//! runtime do tokio por cima. Os segredos vêm de [`Credenciais`] (chaveiro, ambiente, `.env`);
//! nunca de argv, de arquivo do projeto nem de log.

use sbf_chaveiro::Chaveiro;
use sbf_coletor::config::{ConfigGmail, Segredo};
use sbf_coletor::erro::ErroDeColeta;
use sbf_coletor::gmail::cliente::{ClienteGmail, PaginaDeMensagens};
use sbf_coletor::gmail::mensagem::MensagemDoGmail;

use crate::credenciais::{Alvo, Credenciais};
use crate::erro::{Erro, Resultado};

/// Um rótulo (label) da conta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rotulo {
    /// O id que a API usa (`Label_123`, `INBOX`...).
    pub id: String,
    /// O nome que a pessoa vê.
    pub nome: String,
}

/// O que a coleta precisa do Gmail.
pub trait FonteDeGmail {
    /// Uma página de ids de mensagem que casam com `consulta` e, se dado, estão no rótulo
    /// `rotulo` (id). Do mais novo para o mais velho, como o Gmail devolve.
    ///
    /// # Errors
    /// [`Erro::Gmail`] (ou [`Erro::Autorizacao`]).
    fn listar(
        &self,
        rotulo: Option<&str>,
        consulta: &str,
        pagina: Option<&str>,
    ) -> Resultado<PaginaDeMensagens>;

    /// A mensagem inteira (cabeçalhos, partes e corpo).
    ///
    /// # Errors
    /// [`Erro::Gmail`].
    fn mensagem(&self, id: &str) -> Resultado<MensagemDoGmail>;

    /// Os bytes de um anexo.
    ///
    /// # Errors
    /// [`Erro::Gmail`].
    fn anexo(&self, id_da_mensagem: &str, id_do_anexo: &str) -> Resultado<Vec<u8>>;

    /// Os rótulos da conta.
    ///
    /// # Errors
    /// [`Erro::Gmail`].
    fn rotulos(&self) -> Resultado<Vec<Rotulo>>;
}

/// O cliente de verdade.
pub struct ClienteReal {
    cliente: ClienteGmail,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for ClienteReal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClienteReal").finish_non_exhaustive()
    }
}

/// A resposta de erro é a de renovação do token com `invalid_grant`?
#[must_use]
pub fn eh_autorizacao_vencida(e: &ErroDeColeta) -> bool {
    matches!(
        e,
        ErroDeColeta::Api { status, corpo, .. }
            if (*status == 400 || *status == 401) && corpo.contains("invalid_grant")
    )
}

/// O erro do coletor como erro do Gmail no desktop (`invalid_grant` vira
/// [`Erro::Autorizacao`], o resto [`Erro::Gmail`]).
#[must_use]
pub fn do_gmail(e: ErroDeColeta) -> Erro {
    if eh_autorizacao_vencida(&e) {
        Erro::Autorizacao
    } else {
        Erro::Gmail(e.to_string())
    }
}

impl ClienteReal {
    /// Monta o cliente com estas credenciais.
    ///
    /// Não chame de dentro de um runtime do tokio (use uma thread própria).
    ///
    /// # Errors
    /// [`Erro::Gmail`] se o runtime não subiu.
    pub fn novo(credenciais: &Credenciais) -> Resultado<Self> {
        let config = ConfigGmail::com_enderecos_padrao(
            credenciais.client_id.as_str().to_owned(),
            Segredo::novo(credenciais.client_secret.as_str()),
            Segredo::novo(credenciais.refresh_token.as_str()),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Erro::Gmail(format!("o runtime não subiu: {}", e.kind())))?;
        Ok(Self {
            cliente: ClienteGmail::novo(config),
            runtime,
        })
    }

    /// Monta o cliente da conta `alvo` com as credenciais do chaveiro, do ambiente e do `.env`.
    ///
    /// # Errors
    /// [`Erro::SemCredenciais`] com os nomes que faltam; [`Erro::Gmail`].
    pub fn resolver(chaveiro: Option<&Chaveiro>, alvo: Alvo<'_>) -> Resultado<Self> {
        Self::novo(&Credenciais::do_ambiente(chaveiro, alvo)?)
    }

    /// O e-mail da conta (`users/me/profile`).
    ///
    /// # Errors
    /// [`Erro::Gmail`] ou [`Erro::Autorizacao`].
    pub fn conta(&self) -> Resultado<String> {
        self.runtime
            .block_on(self.cliente.email_da_conta())
            .map_err(do_gmail)
    }
}

impl FonteDeGmail for ClienteReal {
    fn listar(
        &self,
        rotulo: Option<&str>,
        consulta: &str,
        pagina: Option<&str>,
    ) -> Resultado<PaginaDeMensagens> {
        self.runtime
            .block_on(
                self.cliente
                    .listar_mensagens(rotulo, Some(consulta), 100, pagina),
            )
            .map_err(do_gmail)
    }

    fn mensagem(&self, id: &str) -> Resultado<MensagemDoGmail> {
        self.runtime
            .block_on(self.cliente.mensagem(id))
            .map_err(do_gmail)
    }

    fn anexo(&self, id_da_mensagem: &str, id_do_anexo: &str) -> Resultado<Vec<u8>> {
        self.runtime
            .block_on(self.cliente.anexo(id_da_mensagem, id_do_anexo))
            .map_err(do_gmail)
    }

    fn rotulos(&self) -> Resultado<Vec<Rotulo>> {
        Ok(self
            .runtime
            .block_on(self.cliente.listar_rotulos())
            .map_err(do_gmail)?
            .into_iter()
            .map(|r| Rotulo {
                id: r.id,
                nome: r.name,
            })
            .collect())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn api(status: u16, corpo: &str) -> ErroDeColeta {
        ErroDeColeta::Api {
            servico: "gmail",
            status,
            caminho: "renovação do access token".to_owned(),
            corpo: corpo.to_owned(),
        }
    }

    #[test]
    fn invalid_grant_vira_autorizacao_e_o_resto_vira_gmail() {
        let vencida = api(
            400,
            r#"{ "error": "invalid_grant", "error_description": "Token has been expired or revoked." }"#,
        );
        assert!(eh_autorizacao_vencida(&vencida));
        assert_eq!(do_gmail(vencida), Erro::Autorizacao);
        assert_eq!(do_gmail(api(401, "invalid_grant")), Erro::Autorizacao);
        // Outros erros 400 / 5xx / rede não são autorização vencida.
        for e in [
            api(400, r#"{"error":"invalid_client"}"#),
            api(500, "invalid_grant"),
            ErroDeColeta::Rede {
                servico: "gmail",
                detalhe: "sem rede".to_owned(),
            },
        ] {
            assert!(matches!(do_gmail(e), Erro::Gmail(_)));
        }
    }
}
