//! `telegram-estado.json`: o checkpoint por conversa (último id baixado) e o que a janela
//! mostra (última coleta, quantos arquivos, último erro). Perder o arquivo só custa reler:
//! a idempotência por origem e por sha256 evita duplicar.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::arquivos::escrever_atomico;
use crate::{Erro, Resultado};

/// O arquivo do estado, na pasta de dados do app.
pub const ARQUIVO_DO_ESTADO: &str = "telegram-estado.json";

/// O estado de uma conversa.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EstadoDaConversa {
    /// O maior id de mensagem já tratado (0 = nada ainda).
    pub ultimo_id: i64,
    /// Quantos arquivos novos já entraram desta conversa.
    pub arquivos: u64,
    /// Quando foi a última coleta (ISO 8601 UTC).
    pub ultima_coleta: Option<String>,
}

/// O estado do Telegram.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Estado {
    /// Por id de conversa.
    pub conversas: BTreeMap<String, EstadoDaConversa>,
    /// Quando foi a última coleta de qualquer conversa.
    pub ultima_coleta: Option<String>,
    /// O que deu errado na última rodada (texto sem conteúdo de conversa); `None` se deu certo.
    pub ultimo_erro: Option<String>,
}

impl Estado {
    /// Lê `caminho`; ausente ou ilegível dá vazio.
    #[must_use]
    pub fn carregar(caminho: &Path) -> Self {
        std::fs::read_to_string(caminho)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Grava `caminho`.
    ///
    /// # Errors
    /// Falha de disco.
    pub fn salvar(&self, caminho: &Path) -> Resultado<()> {
        let texto = serde_json::to_vec(self).map_err(|e| Erro::Formato(e.to_string()))?;
        escrever_atomico(caminho, &texto)
    }

    /// O checkpoint da conversa.
    #[must_use]
    pub fn ultimo_id(&self, conversa: &str) -> i64 {
        self.conversas.get(conversa).map_or(0, |c| c.ultimo_id)
    }

    /// Total de arquivos novos de todas as conversas.
    #[must_use]
    pub fn arquivos(&self) -> u64 {
        self.conversas.values().map(|c| c.arquivos).sum()
    }
}
