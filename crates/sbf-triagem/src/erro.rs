//! O erro comum da triagem.

use sbf_inventario::ErroDeComando;

#[derive(Debug, thiserror::Error)]
pub enum ErroDaTriagem {
    #[error("erro do banco: {0}")]
    Banco(#[from] rusqlite::Error),
    #[error("erro ao decidir: {0}")]
    Comando(#[from] ErroDeComando),
    #[error("erro da IA: {0}")]
    Ia(#[from] sbf_ia::ErroDeIa),
    #[error("{0}")]
    Outro(String),
}

pub type Resultado<T> = Result<T, ErroDaTriagem>;
