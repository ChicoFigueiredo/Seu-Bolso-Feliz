//! Erros dos roteiros. Nenhuma mensagem leva valor de campo, senha ou conteúdo de página.

use crate::bloqueio::Bloqueio;

/// Falha de um roteiro.
#[derive(Debug, thiserror::Error)]
pub enum ErroDeRoteiro {
    #[error("banco dos roteiros: {0}")]
    Banco(#[from] rusqlite::Error),
    #[error("disco: {0}")]
    Disco(#[from] std::io::Error),
    /// Falha do navegador ou do CDP (sem valores enviados).
    #[error("navegador: {0}")]
    Navegador(String),
    /// Um passo falhou (o site mudou, o seletor sumiu, o prazo estourou).
    #[error("passo {passo} falhou: {motivo}")]
    PassoFalhou { passo: usize, motivo: String },
    /// A lista de bloqueio barrou uma ação perigosa.
    #[error("ação bloqueada: {0}")]
    Bloqueado(Bloqueio),
    /// O chaveiro não tem o que o passo pede.
    #[error("chaveiro: {0}")]
    Chaveiro(String),
    #[error("roteiro inválido: {0}")]
    Invalido(String),
    #[error("ia: {0}")]
    Ia(#[from] sbf_ia::ErroDeIa),
    #[error("cancelado")]
    Cancelado,
}

impl From<bancada_navegador::ErroDoNavegador> for ErroDeRoteiro {
    fn from(e: bancada_navegador::ErroDoNavegador) -> Self {
        match e {
            bancada_navegador::ErroDoNavegador::Disco(e) => Self::Disco(e),
            bancada_navegador::ErroDoNavegador::Navegador(m) => Self::Navegador(m),
        }
    }
}

pub type Resultado<T> = Result<T, ErroDeRoteiro>;
