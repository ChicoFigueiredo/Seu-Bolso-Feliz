//! Erros do worker. As mensagens não carregam caminho de documento nem segredo.

/// Falhas do worker.
#[derive(Debug, thiserror::Error)]
pub enum Erro {
    /// Falha de disco.
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    /// Falha do banco.
    #[error("falha no banco: {0}")]
    Banco(#[from] rusqlite::Error),
    /// A configuração não é um JSON válido.
    #[error("a configuração do worker está ilegível")]
    Configuracao,
    /// Outro worker já roda nesta pasta de dados.
    #[error("já há um worker rodando")]
    JaRodando,
    /// Serviço do usuário em sistema ainda não suportado.
    #[error(
        "ainda não suportado: o serviço do usuário só existe no Linux (systemd); no Windows e no macOS use `sbf worker rodar` à mão"
    )]
    NaoSuportado,
    /// O `systemctl` falhou ou não foi achado.
    #[error("systemctl: {0}")]
    Systemctl(String),
    /// Caminho que não pode entrar numa unit.
    #[error("caminho inválido para a unit (precisa ser absoluto e sem quebra de linha)")]
    CaminhoInvalido,
    /// Falha da coleta de pastas.
    #[error("coleta: {0}")]
    Coleta(#[from] sbf_coleta::Erro),
}

/// Resultado do crate.
pub type Resultado<T> = Result<T, Erro>;
