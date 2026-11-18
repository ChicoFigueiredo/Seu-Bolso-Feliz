//! As ferramentas do Telegram (conexão da conta do usuário, só leitura): `telegram_conversas`,
//! `telegram_seguir` e `telegram_coletar`. Desde a D15 aplicam direto, sem confirmação na
//! janela. O servidor só as anuncia com a ponte ligada; quem a liga é o `sbf` (o MCP não
//! conhece o MTProto nem o chaveiro).
//!
//! Nada aqui envia, apaga ou marca como lida uma mensagem. Quem coleta é o mesmo código do
//! `sbf telegram coletar` e do worker.

use std::sync::Arc;

use rmcp::schemars;
use serde::Deserialize;
use serde_json::{Value, json};

/// Os nomes das ferramentas do Telegram.
pub const FERRAMENTAS: [&str; 3] = ["telegram_conversas", "telegram_seguir", "telegram_coletar"];

/// O que o `sbf` oferece ao MCP. Os erros são texto curto, sem número, código nem conteúdo.
pub trait PonteDoTelegram: Send + Sync {
    /// As conversas da conta com `seguida` marcada. `atualizar` pergunta ao Telegram; sem
    /// ele vale a última lista guardada.
    ///
    /// # Errors
    /// Precisa entrar (`sbf telegram entrar`), rede ou disco.
    fn conversas(&self, atualizar: bool) -> Result<Value, String>;

    /// Passa a seguir (ou, com `seguir = false`, deixa de seguir) a conversa (id ou nome).
    ///
    /// # Errors
    /// Conversa desconhecida ou ambígua; disco.
    fn seguir(&self, conversa: &str, seguir: bool) -> Result<Value, String>;

    /// Coleta já: baixa as mídias novas das conversas seguidas (ou só de uma), registra no
    /// inventário e reconhece. `desde` (`AAAA-MM-DD`) vale só na primeira coleta da conversa.
    ///
    /// # Errors
    /// Precisa entrar, rede, banco ou disco.
    fn coletar(&self, conversa: Option<&str>, desde: Option<&str>) -> Result<Value, String>;
}

/// A ponte, barata de clonar.
#[derive(Clone)]
pub struct Ponte(pub Arc<dyn PonteDoTelegram>);

impl std::fmt::Debug for Ponte {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ponte(telegram)")
    }
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeConversas {
    /// true: pergunta ao Telegram agora (precisa de sessão). Sem isso vale a última lista.
    pub atualizar: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeSeguir {
    /// O id (de telegram_conversas) ou o nome da conversa.
    pub conversa: String,
    /// true: deixa de seguir (padrão: segue).
    pub deixar: Option<bool>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct PedidoDeColeta {
    /// Só esta conversa (id ou nome); sem isso, todas as seguidas.
    pub conversa: Option<String>,
    /// AAAA-MM-DD: na primeira coleta de cada conversa, ignora mensagens mais antigas.
    pub desde: Option<String>,
}

/// `AAAA-MM-DD` com mês e dia plausíveis.
#[must_use]
pub fn data_valida(texto: &str) -> bool {
    let b = texto.as_bytes();
    if !texto.is_ascii() || b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let Ok(ano) = texto[0..4].parse::<u32>() else {
        return false;
    };
    let (Ok(mes), Ok(dia)) = (texto[5..7].parse::<u32>(), texto[8..10].parse::<u32>()) else {
        return false;
    };
    ano >= 2000 && (1..=12).contains(&mes) && (1..=31).contains(&dia)
}

/// `telegram_conversas`.
///
/// # Errors
/// O que a ponte devolver.
pub fn conversas(ponte: &Ponte, p: &PedidoDeConversas) -> Result<Value, String> {
    ponte.0.conversas(p.atualizar.unwrap_or(false))
}

/// `telegram_seguir`.
///
/// # Errors
/// Conversa em branco, ou o que a ponte devolver.
pub fn seguir(ponte: &Ponte, p: &PedidoDeSeguir) -> Result<Value, String> {
    let conversa = p.conversa.trim();
    if conversa.is_empty() {
        return Err("diga o id ou o nome da conversa".to_owned());
    }
    ponte.0.seguir(conversa, !p.deixar.unwrap_or(false))
}

/// `telegram_coletar`.
///
/// # Errors
/// Data inválida, ou o que a ponte devolver.
pub fn coletar(ponte: &Ponte, p: &PedidoDeColeta) -> Result<Value, String> {
    if let Some(d) = p.desde.as_deref()
        && !data_valida(d)
    {
        return Err("desde deve ser AAAA-MM-DD".to_owned());
    }
    let conversa = p
        .conversa
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let r = ponte.0.coletar(conversa, p.desde.as_deref())?;
    Ok(json!({ "coleta": r }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas() {
        assert!(data_valida("2026-11-18"));
        assert!(!data_valida("18/11/2026"));
        assert!(!data_valida("2026-13-01"));
        assert!(!data_valida("2026-11-1x"));
        assert!(!data_valida("1999-01-01"));
        assert!(!data_valida("2026-11-é8"), "não ASCII não derruba");
    }
}
