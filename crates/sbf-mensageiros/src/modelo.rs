//! O vocabulário comum das fontes de mensagens, igual para WhatsApp e Telegram.

use serde::{Deserialize, Serialize};

/// De qual mensageiro vem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Plataforma {
    /// WhatsApp (dispositivo vinculado, ADR-017).
    Whatsapp,
    /// Telegram (export do Desktop).
    Telegram,
}

impl Plataforma {
    /// `whatsapp` ou `telegram`: o prefixo da origem e o `origin_type`.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Whatsapp => "whatsapp",
            Self::Telegram => "telegram",
        }
    }

    /// O id da fonte no inventário (`mensageiro:whatsapp`).
    #[must_use]
    pub const fn fonte(self) -> &'static str {
        match self {
            Self::Whatsapp => "mensageiro:whatsapp",
            Self::Telegram => "mensageiro:telegram",
        }
    }

    /// Descrição da fonte, para quando ela ainda não existir.
    #[must_use]
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Whatsapp => "WhatsApp (dispositivo vinculado, só leitura)",
            Self::Telegram => "Telegram (export do Desktop)",
        }
    }
}

/// Uma conversa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversa {
    /// Id estável na plataforma (não é o nome: o nome muda).
    pub id: String,
    /// Nome para mostrar na lista de escolha.
    pub nome: String,
    /// É grupo ou canal?
    pub grupo: bool,
}

/// O anexo (mídia ou documento) de uma mensagem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anexo {
    /// Nome do arquivo (`boleto.pdf`); sem nome, a fonte dá um com a extensão certa.
    pub nome: String,
    /// Tipo MIME, se a plataforma disse.
    #[serde(default)]
    pub mime: Option<String>,
    /// Tamanho em bytes, se conhecido.
    #[serde(default)]
    pub tamanho: Option<u64>,
}

/// Uma mensagem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mensagem {
    /// Id da mensagem na conversa.
    pub id: String,
    /// Id da conversa.
    pub conversa: String,
    /// Instante, em segundos unix (UTC).
    pub quando: u64,
    /// O texto (ou a legenda da mídia).
    #[serde(default)]
    pub texto: Option<String>,
    /// O anexo.
    #[serde(default)]
    pub anexo: Option<Anexo>,
}

/// Até onde a conversa já foi lida. Mensagem nova é a que vem depois, por `(quando, id)`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Instante da última mensagem lida (segundos unix).
    pub quando: u64,
    /// Id da última mensagem lida (desempata o mesmo segundo).
    #[serde(default)]
    pub id: Option<String>,
}

impl Checkpoint {
    /// A mensagem é posterior a este ponto?
    #[must_use]
    pub fn antes_de(&self, m: &Mensagem) -> bool {
        match (m.quando.cmp(&self.quando), &self.id) {
            (std::cmp::Ordering::Greater, _) => true,
            (std::cmp::Ordering::Equal, Some(id)) => m.id.as_str() > id.as_str(),
            (std::cmp::Ordering::Equal, None) => false,
            (std::cmp::Ordering::Less, _) => false,
        }
    }

    /// O ponto logo depois de `m`.
    #[must_use]
    pub fn de(m: &Mensagem) -> Self {
        Self {
            quando: m.quando,
            id: Some(m.id.clone()),
        }
    }
}
