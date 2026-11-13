//! Erros da estante e da cópia de segurança. Nenhum carrega senha.

use std::path::PathBuf;

/// Falhas da estante e da cópia de segurança.
#[derive(Debug, thiserror::Error)]
pub enum Erro {
    /// O programa `rclone` não foi encontrado.
    #[error("o rclone não está instalado ou não está no PATH")]
    RcloneAusente,
    /// O rclone saiu com erro. `mensagem` é o fim do stderr dele (nunca a senha).
    #[error("rclone {comando} falhou (código {codigo:?}): {mensagem}")]
    Rclone {
        /// Subcomando (`copy`, `lsf`...).
        comando: String,
        /// Código de saída, se houve.
        codigo: Option<i32>,
        /// Fim do stderr.
        mensagem: String,
    },
    /// Falha de disco.
    #[error("falha de disco: {0}")]
    Io(#[from] std::io::Error),
    /// Falha do SQLite.
    #[error("falha no banco: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Falha da cópia/integridade do banco.
    #[error("falha na cópia do banco: {0}")]
    Armazem(#[from] bancada_armazem::Erro),
    /// A senha da cópia está vazia.
    #[error("a senha da cópia de segurança está vazia")]
    SenhaVazia,
    /// O destino não é aceitável (vazio ou começando com `-`).
    #[error("destino inválido: {0:?}")]
    DestinoInvalido(String),
    /// A cópia remota não tem nenhum instantâneo do banco (senha errada ou cópia vazia).
    #[error("nenhum instantâneo do banco na cópia: a senha está errada ou a cópia está vazia")]
    SemInstantaneo,
    /// O banco restaurado não confere com o hash gravado no nome do instantâneo.
    #[error("o banco restaurado não confere com o instantâneo ({esperado} != {obtido})")]
    HashDiferente {
        /// Prefixo do hash que o nome prometia.
        esperado: String,
        /// Prefixo do hash que o arquivo tem.
        obtido: String,
    },
    /// A restauração não sobrescreve nada.
    #[error("já existe {0}; a restauração não sobrescreve")]
    JaExiste(PathBuf),
}

/// Resultado do crate.
pub type Resultado<T> = Result<T, Erro>;
