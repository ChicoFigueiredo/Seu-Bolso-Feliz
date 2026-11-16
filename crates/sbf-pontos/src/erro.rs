//! Erro dos pontos.

#[derive(Debug, thiserror::Error)]
pub enum ErroDosPontos {
    #[error("banco: {0}")]
    Banco(#[from] rusqlite::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("disco: {0}")]
    Disco(#[from] std::io::Error),
    #[error("{0}")]
    Entrada(String),
}

pub type Resultado<T> = Result<T, ErroDosPontos>;
