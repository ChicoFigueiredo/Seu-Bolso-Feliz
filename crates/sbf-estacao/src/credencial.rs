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
