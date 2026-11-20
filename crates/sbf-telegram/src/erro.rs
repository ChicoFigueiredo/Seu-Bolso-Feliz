//! Erros do crate. Nenhuma mensagem carrega conteúdo de conversa, número de telefone,
//! código de login, senha nem a sessão.

/// Falhas do Telegram.
#[derive(Debug, thiserror::Error)]
pub enum Erro {
    /// Falha do banco do inventário.
    #[error("falha no banco: {0}")]
    Banco(#[from] rusqlite::Error),
    /// Falha de disco.
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    /// Arquivo de configuração, de estado ou sessão ilegível.
    #[error("arquivo ilegível: {0}")]
    Formato(String),
    /// O Telegram (ou a rede) falhou.
    #[error("o Telegram falhou: {0}")]
    Fonte(String),
    /// Não há sessão guardada, ou o Telegram a recusou: é preciso rodar `sbf telegram entrar`.
    #[error("precisa entrar: rode `sbf telegram entrar` no terminal")]
    PrecisaEntrar,
    /// `TELEGRAM_API_ID` e `TELEGRAM_API_HASH` não estão no ambiente nem no `.env`.
    #[error(
        "faltam TELEGRAM_API_ID e TELEGRAM_API_HASH no ambiente ou no .env da raiz do repositório"
    )]
    SemCredenciais,
    /// O chaveiro recusou a operação.
    #[error("chaveiro: {0}")]
    Chaveiro(String),
    /// Nenhuma conversa com esse id ou nome.
    #[error("nenhuma conversa chamada assim; rode `sbf telegram conversas`")]
    ConversaDesconhecida,
    /// Mais de uma conversa com esse nome.
    #[error("mais de uma conversa casa com esse nome ({0}); use o id")]
    ConversaAmbigua(usize),
    /// A coleta foi cancelada.
    #[error("cancelado")]
    Cancelado,
    /// O login não terminou (código ou senha errados, ou abandonado).
    #[error("login recusado: {0}")]
    Login(String),
}

/// Resultado do crate.
pub type Resultado<T> = Result<T, Erro>;

impl From<sbf_mensageiros::Erro> for Erro {
    fn from(e: sbf_mensageiros::Erro) -> Self {
        match e {
            sbf_mensageiros::Erro::Banco(e) => Self::Banco(e),
            sbf_mensageiros::Erro::Io(e) => Self::Io(e),
            outro => Self::Formato(outro.to_string()),
        }
    }
}
