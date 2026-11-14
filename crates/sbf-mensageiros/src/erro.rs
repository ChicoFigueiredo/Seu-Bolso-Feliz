//! Erros do crate. Nenhuma mensagem carrega conteúdo de conversa, nome de contato nem segredo.

/// Falhas dos mensageiros.
#[derive(Debug, thiserror::Error)]
pub enum Erro {
    /// Falha do banco do inventário.
    #[error("falha no banco: {0}")]
    Banco(#[from] rusqlite::Error),
    /// Falha de disco.
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    /// Arquivo de configuração, de checkpoint ou export ilegível.
    #[error("arquivo ilegível: {0}")]
    Formato(String),
    /// A fonte (ponte, export) falhou.
    #[error("a fonte falhou: {0}")]
    Fonte(String),
    /// A fonte não está pareada ou o export não existe.
    #[error("a fonte não está pronta")]
    NaoPronta,
    /// O chaveiro recusou a operação.
    #[error("chaveiro: {0}")]
    Chaveiro(String),
    /// A sessão guardada não abre (adulterada ou chave errada).
    #[error("a sessão guardada não abre")]
    SessaoInvalida,
}

/// Resultado do crate.
pub type Resultado<T> = Result<T, Erro>;
