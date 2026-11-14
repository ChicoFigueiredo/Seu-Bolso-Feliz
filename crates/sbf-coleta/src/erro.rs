//! Erros da coleta.

/// Falhas da coleta.
#[derive(Debug, thiserror::Error)]
pub enum Erro {
    /// O rclone falhou ou não existe.
    #[error("rclone: {0}")]
    Rclone(#[from] sbf_estante::Erro),
    /// Falha do banco.
    #[error("falha no banco: {0}")]
    Banco(#[from] rusqlite::Error),
    /// Falha de disco.
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    /// A listagem do rclone não é um JSON de itens.
    #[error("a listagem do rclone está ilegível: {0}")]
    Listagem(String),
    /// Não há remoto do tipo pedido no arquivo de configuração do rclone.
    #[error("nenhum remoto do tipo {0} no rclone")]
    SemRemoto(&'static str),
}

/// Resultado do crate.
pub type Resultado<T> = Result<T, Erro>;
