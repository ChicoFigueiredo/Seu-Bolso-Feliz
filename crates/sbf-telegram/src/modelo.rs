//! O vocabulário do Telegram pela conta do usuário.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Que tipo de conversa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tipo {
    /// Grupo.
    Grupo,
    /// Canal (só administradores publicam).
    Canal,
    /// Conversa com uma pessoa (inclui "Mensagens salvas").
    Privada,
}

impl Tipo {
    /// Para a lista no terminal e na janela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Grupo => "grupo",
            Self::Canal => "canal",
            Self::Privada => "privada",
        }
    }
}

/// Uma conversa. O id é o `bot_api_dialog_id` (estável; o nome muda).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversa {
    /// Id estável, em texto (pode ser negativo).
    pub id: String,
    /// Nome para mostrar.
    pub nome: String,
    /// O tipo.
    pub tipo: Tipo,
}

/// O que pedir à fonte: as mídias de uma conversa depois de um id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    /// A conversa (o id de [`Conversa::id`]).
    pub conversa: String,
    /// Só mensagens com id maior que este (0 = desde o começo).
    pub depois_do_id: i64,
    /// Só mensagens de data igual ou posterior (segundos unix). Vale só quando
    /// `depois_do_id` é 0 (a primeira coleta); depois disso o checkpoint manda.
    pub desde_data: Option<u64>,
    /// No máximo quantas mídias baixar nesta rodada.
    pub limite: usize,
}

/// Uma mídia baixada, já no disco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Midia {
    /// Id da mensagem na conversa.
    pub mensagem: i64,
    /// Instante da mensagem, em segundos unix (UTC).
    pub quando: u64,
    /// Nome do arquivo como o Telegram o conhece (`boleto.pdf`, `foto.jpg`).
    pub nome: String,
    /// Onde o arquivo foi gravado (dentro da pasta pedida).
    pub caminho: PathBuf,
}

/// Até onde a fonte olhou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Avanco {
    /// O maior id de mensagem já tratado nesta rodada (o checkpoint, se tudo deu certo).
    pub ate_id: i64,
}
