//! O cliente do Gmail atrás de um trait, para a coleta ser testável sem rede.
//!
//! O real é o [`ClienteGmail`] do `sbf-coletor` (OAuth2 com refresh token, os mesmos
//! endereços e o mesmo tratamento de erro do `sbf coletor-vps gmail`), numa thread só, com um
//! runtime do tokio por cima. Os segredos vêm de variáveis de ambiente, lidas no momento de
//! montar [`ClienteReal::do_ambiente`]; nunca de argv, de arquivo do projeto nem de log.

use sbf_coletor::config::ConfigGmail;
use sbf_coletor::erro::ErroDeColeta;
use sbf_coletor::gmail::cliente::{ClienteGmail, PaginaDeMensagens};
use sbf_coletor::gmail::mensagem::MensagemDoGmail;

use crate::erro::{Erro, Resultado};

/// As variáveis de ambiente que guardam as credenciais OAuth2 (as mesmas do coletor da VPS).
pub const VARIAVEIS: [&str; 3] = [
    "GOOGLE_MAIL_CLIENT_ID",
    "GOOGLE_MAIL_CLIENT_SECRET",
    "GMAIL_REFRESH_TOKEN",
];

/// O que a coleta precisa do Gmail.
pub trait FonteDeGmail {
    /// Uma página de ids de mensagem que casam com `consulta` (do mais novo para o mais
    /// velho, como o Gmail devolve).
    ///
    /// # Errors
    /// [`Erro::Gmail`].
    fn listar(&self, consulta: &str, pagina: Option<&str>) -> Resultado<PaginaDeMensagens>;

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
}

/// As variáveis de credencial que estão preenchidas (só um sim/não; nunca o valor).
#[must_use]
pub fn credenciais_presentes() -> bool {
    faltando().is_empty()
}

/// Os **nomes** das variáveis que faltam.
#[must_use]
pub fn faltando() -> Vec<&'static str> {
    VARIAVEIS
        .iter()
        .copied()
        .filter(|n| {
            std::env::var(n)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
        })
        .collect()
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

fn do_gmail(e: ErroDeColeta) -> Erro {
    Erro::Gmail(e.to_string())
}

impl ClienteReal {
    /// Monta o cliente com as credenciais do ambiente.
    ///
    /// Não chame de dentro de um runtime do tokio (use uma thread própria).
    ///
    /// # Errors
    /// [`Erro::SemCredenciais`] com os nomes que faltam; [`Erro::Gmail`] se o runtime não subiu.
    pub fn do_ambiente() -> Resultado<Self> {
        let falta = faltando();
        if !falta.is_empty() {
            return Err(Erro::SemCredenciais(falta.join(", ")));
        }
        let config = ConfigGmail::do_ambiente().map_err(do_gmail)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Erro::Gmail(format!("o runtime não subiu: {}", e.kind())))?;
        Ok(Self {
            cliente: ClienteGmail::novo(config),
            runtime,
        })
    }
}

impl FonteDeGmail for ClienteReal {
    fn listar(&self, consulta: &str, pagina: Option<&str>) -> Resultado<PaginaDeMensagens> {
        self.runtime
            .block_on(
                self.cliente
                    .listar_mensagens(None, Some(consulta), 100, pagina),
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
}
