//! Os erros do Gmail no desktop. Nenhuma mensagem leva conteúdo de e-mail, segredo ou
//! endereço: só a categoria e, nas credenciais, o **nome** das variáveis que faltam.

/// Falha da coleta do Gmail.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Erro {
    /// Faltam variáveis de ambiente com as credenciais OAuth (só os nomes).
    #[error("faltam as credenciais do Gmail: {0}")]
    SemCredenciais(String),
    /// O Gmail (ou a rede) respondeu com erro.
    #[error("falha ao falar com o Gmail: {0}")]
    Gmail(String),
    /// O inventário não abriu ou recusou a gravação.
    #[error("falha do inventário: {0}")]
    Banco(String),
    /// Falha de disco no cofre.
    #[error("falha de disco no cofre: {0}")]
    Disco(String),
    /// O reconhecimento falhou de um jeito que impede seguir.
    #[error("falha no reconhecimento: {0}")]
    Reconhecimento(String),
    /// Um argumento inválido (data, limite).
    #[error("argumento inválido: {0}")]
    Argumento(String),
    /// Cancelado pelo usuário.
    #[error("cancelado")]
    Cancelado,
}

/// O resultado do crate.
pub type Resultado<T> = Result<T, Erro>;

impl From<rusqlite::Error> for Erro {
    fn from(e: rusqlite::Error) -> Self {
        Erro::Banco(e.to_string())
    }
}

impl From<std::io::Error> for Erro {
    fn from(e: std::io::Error) -> Self {
        Erro::Disco(e.kind().to_string())
    }
}
