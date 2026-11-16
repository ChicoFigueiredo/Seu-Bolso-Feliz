//! Erro do índice.

#[derive(Debug, thiserror::Error)]
pub enum ErroDoIndice {
    #[error("banco do índice: {0}")]
    Banco(#[from] rusqlite::Error),
    #[error("embedder: {0}")]
    Embedder(String),
    #[error("a leitura do livro-caixa falhou: {0}")]
    Livro(String),
    #[error("motor de IA: {0}")]
    Ia(#[from] sbf_ia::ErroDeIa),
    #[error("resposta fora do esperado: {0}")]
    Resposta(String),
    #[error("{0}")]
    Outro(String),
}

pub type Resultado<T> = Result<T, ErroDoIndice>;
