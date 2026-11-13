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
    /// A senha escolhida pelo usuário é curta demais.
    #[error("a senha da cópia de segurança precisa de pelo menos {minimo} caracteres")]
    SenhaCurta {
        /// O mínimo ([`crate::copia::MINIMO_DA_SENHA_ESCOLHIDA`]).
        minimo: usize,
    },
    /// O sistema não entregou bytes aleatórios para a senha ou o sal.
    #[error("o sistema não entregou bytes aleatórios")]
    Aleatorio,
    /// O destino tem arquivos e não tem o sal: não é uma cópia deste app (ou é de antes do sal
    /// próprio). A cópia não mistura.
    #[error("o destino da cópia já tem arquivos e não tem o sal da cópia; escolha uma pasta vazia")]
    DestinoSemSal,
    /// O sal guardado no destino não está no formato (64 hexadecimais).
    #[error("o sal guardado no destino da cópia está adulterado")]
    SalInvalido,
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
