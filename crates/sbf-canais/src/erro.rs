//! Os erros dos canais. Nenhuma mensagem leva o token do bot nem o texto de um aviso.

/// Falha de um canal.
#[derive(Debug, thiserror::Error)]
pub enum ErroDeCanal {
    /// Variável de ambiente ausente ou valor inválido.
    #[error("configuração: {0}")]
    Configuracao(String),
    /// O Postgres recusou ou não respondeu.
    #[error("banco: {0}")]
    Banco(String),
    /// A rede falhou antes de haver resposta (sem a URL, que leva o token).
    #[error("telegram: falha de rede: {0}")]
    Rede(String),
    /// O Telegram respondeu com erro.
    #[error("telegram: {0}")]
    Telegram(String),
    /// O livro-caixa não pôde ser lido.
    #[error("livro: {0}")]
    Livro(String),
    /// Entrada inválida (argumento, código).
    #[error("{0}")]
    Entrada(String),
}

impl From<tokio_postgres::Error> for ErroDeCanal {
    fn from(e: tokio_postgres::Error) -> Self {
        match e.as_db_error() {
            Some(db) => Self::Banco(format!("{} ({})", db.message(), db.code().code())),
            None => Self::Banco(e.to_string()),
        }
    }
}

impl From<deadpool_postgres::PoolError> for ErroDeCanal {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        Self::Banco(e.to_string())
    }
}

impl From<sbf_coletor::ErroDeColeta> for ErroDeCanal {
    fn from(e: sbf_coletor::ErroDeColeta) -> Self {
        match e {
            sbf_coletor::ErroDeColeta::Configuracao(m) => Self::Configuracao(m),
            outro => Self::Banco(outro.to_string()),
        }
    }
}
