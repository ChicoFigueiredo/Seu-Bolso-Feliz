//! A credencial da estação: no chaveiro (`estacao:<servidor>`), nunca em arquivo, log ou
//! SQLite. O portão de rede a lê a cada pedido por [`CredencialDaEstacao`].

use bancada_rede::{Autorizacao, Endereco, FonteDeAutorizacao, Segredo};
use sbf_chaveiro::Chaveiro;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

/// Onde a credencial mora. O app usa o [`Chaveiro`]; os testes, [`GuardaEmMemoria`].
pub trait GuardaDeCredencial: Send + Sync + fmt::Debug {
    /// A credencial de `servidor` (o host), se houver.
    fn ler(&self, servidor: &str) -> Option<Segredo>;
    /// Guarda (ou troca) a credencial.
    ///
    /// # Errors
    /// Texto do erro do chaveiro (sem valor).
    fn gravar(&self, servidor: &str, credencial: Segredo) -> Result<(), String>;
    /// Apaga a credencial (não é erro se não houver).
    ///
    /// # Errors
    /// Texto do erro do chaveiro (sem valor).
    fn apagar(&self, servidor: &str) -> Result<(), String>;
    /// Há credencial (sem ler o valor).
    fn tem(&self, servidor: &str) -> bool;
}

impl GuardaDeCredencial for Chaveiro {
    fn ler(&self, servidor: &str) -> Option<Segredo> {
        self.credencial_de_estacao(servidor).ok()
    }

    fn gravar(&self, servidor: &str, credencial: Segredo) -> Result<(), String> {
        self.guardar_credencial_de_estacao(servidor, credencial)
            .map_err(|e| e.to_string())
    }

    fn apagar(&self, servidor: &str) -> Result<(), String> {
        self.apagar_credencial_de_estacao(servidor)
            .map_err(|e| e.to_string())
    }

    fn tem(&self, servidor: &str) -> bool {
        self.tem_credencial_de_estacao(servidor)
    }
}

/// Credenciais só em memória (testes e modo demonstração). O `Debug` mostra só os servidores.
#[derive(Default, Clone)]
pub struct GuardaEmMemoria {
    credenciais: Arc<Mutex<HashMap<String, Segredo>>>,
}

impl fmt::Debug for GuardaEmMemoria {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let servidores: Vec<String> = self
            .credenciais
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        f.debug_struct("GuardaEmMemoria")
            .field("servidores", &servidores)
            .finish()
    }
}

impl GuardaDeCredencial for GuardaEmMemoria {
    fn ler(&self, servidor: &str) -> Option<Segredo> {
        self.credenciais
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(servidor)
            .cloned()
    }

    fn gravar(&self, servidor: &str, credencial: Segredo) -> Result<(), String> {
        self.credenciais
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(servidor.to_owned(), credencial);
        Ok(())
    }

    fn apagar(&self, servidor: &str) -> Result<(), String> {
        self.credenciais
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(servidor);
        Ok(())
    }

    fn tem(&self, servidor: &str) -> bool {
        self.credenciais
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(servidor)
    }
}

/// A ponte entre a guarda e o portão: o `Authorization: Bearer <credencial>` do servidor,
/// lido a cada pedido (apagar a credencial vale no pedido seguinte).
#[derive(Debug, Clone)]
pub struct CredencialDaEstacao {
    guarda: Arc<dyn GuardaDeCredencial>,
    servidor: String,
}

impl CredencialDaEstacao {
    pub fn nova(guarda: Arc<dyn GuardaDeCredencial>, servidor: impl Into<String>) -> Self {
        Self {
            guarda,
            servidor: servidor.into(),
        }
    }
}

impl FonteDeAutorizacao for CredencialDaEstacao {
    fn autorizacao(&self, _endereco: &Endereco) -> Option<Autorizacao> {
        // O catálogo só marca o endereço do servidor como receptor da credencial.
        self.guarda
            .ler(&self.servidor)
            .as_ref()
            .and_then(Autorizacao::portador)
    }
}

/// Guarda no chaveiro uma credencial emitida pelo servidor fora do pareamento (`sbf
/// servidor emitir-credencial`). Confere só a forma (`sbfe_<32 hex>_<43 base64url>`);
/// quem diz se vale é o servidor. O texto nunca entra em erro nem em log.
///
/// # Errors
/// Forma inválida, ou chaveiro que não guardou.
pub fn importar_credencial(
    guarda: &dyn GuardaDeCredencial,
    servidor: &str,
    texto: &str,
) -> Result<(), crate::ErroDePareamento> {
    let texto = texto.trim();
    let forma_ok = texto.strip_prefix("sbfe_").is_some_and(|resto| {
        resto.split_once('_').is_some_and(|(id, segredo)| {
            id.len() == 32
                && id.bytes().all(|b| b.is_ascii_hexdigit())
                && segredo.len() == 43
                && segredo
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    });
    if !forma_ok {
        return Err(crate::ErroDePareamento::CredencialInvalida);
    }
    guarda
        .gravar(servidor, Segredo::novo(texto.to_owned()))
        .map_err(crate::ErroDePareamento::Chaveiro)
}

#[cfg(test)]
mod testes_de_importacao {
    use super::*;

    const BOA: &str =
        "sbfe_0123456789abcdef0123456789abcdef_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";

    #[test]
    fn importa_e_guarda_sob_o_servidor() {
        let guarda = GuardaEmMemoria::default();
        importar_credencial(&guarda, "127.0.0.1:8787", &format!("{BOA}\n")).unwrap();
        assert!(guarda.tem("127.0.0.1:8787"));
        assert!(!format!("{guarda:?}").contains("AbCdEf"));
    }

    #[test]
    fn recusa_forma_invalida_sem_gravar_nem_vazar() {
        let guarda = GuardaEmMemoria::default();
        for ruim in [
            "",
            "lixo",
            "sbfe_curto_x",
            &BOA[..BOA.len() - 1],
            &BOA.replace("sbfe", "sbfx"),
        ] {
            let e = importar_credencial(&guarda, "s", ruim).unwrap_err();
            assert_eq!(e, crate::ErroDePareamento::CredencialInvalida);
            assert!(!e.to_string().contains(ruim) || ruim.is_empty());
        }
        assert!(!guarda.tem("s"));
    }
}
