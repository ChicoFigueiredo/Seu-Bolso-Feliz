//! Os processos filhos do PDF (`pdftotext`, `qpdf`) pela regra única do `sbf-processo`: o que
//! é sensível entra só pela entrada padrão, nunca pelo argv nem pelo ambiente; todo filho tem
//! limite de tempo e de saída; o programa só roda se for [`sbf_processo::confiavel`], e com o
//! ambiente mínimo.

use sbf_processo::{Chamada, FalhaDeProcesso};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::Duration;
use zeroize::Zeroizing;

/// Por que uma ferramenta externa não entregou o texto. Sem saída nem argumento dentro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum FalhaDeFerramenta {
    /// O programa não foi encontrado (ou não pôde ser executado).
    #[error("ferramenta não encontrada")]
    NaoEncontrada,
    /// Passou do limite de tempo e foi morto.
    #[error("a ferramenta passou do limite de tempo")]
    TempoEsgotado,
    /// Terminou com erro e sem saída.
    #[error("a ferramenta terminou com erro e sem saída")]
    SemSaida,
    /// Falha de entrada e saída com o processo.
    #[error("falha de entrada e saída com a ferramenta: {0}")]
    Io(std::io::ErrorKind),
}

impl From<FalhaDeProcesso> for FalhaDeFerramenta {
    fn from(falha: FalhaDeProcesso) -> Self {
        match falha {
            FalhaDeProcesso::TempoEsgotado => Self::TempoEsgotado,
            FalhaDeProcesso::Io(tipo) => Self::Io(tipo),
            _ => Self::NaoEncontrada,
        }
    }
}

impl From<std::io::Error> for FalhaDeFerramenta {
    fn from(erro: std::io::Error) -> Self {
        FalhaDeProcesso::from(erro).into()
    }
}

/// O máximo guardado da saída de um filho (achado A10 da revisão de segurança da fatia 1).
/// O texto de 3 páginas tem poucos KiB; o que passar disto é drenado e descartado.
pub const SAIDA_MAXIMA_DO_FILHO: usize = sbf_processo::SAIDA_MAXIMA_PADRAO;
/// O máximo guardado do stderr de um filho (só o do `qpdf` é lido, e só para achar "invalid
/// password").
pub(crate) const ERRO_MAXIMO_DO_FILHO: usize = sbf_processo::ERRO_MAXIMO_PADRAO;

/// O que um filho devolveu.
#[derive(Debug)]
pub(crate) struct Saida {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Zeroizing<Vec<u8>>,
}

/// Um comando com a entrada, a saída e o erro em pipe, e o ambiente mínimo.
pub(crate) fn comando(programa: &Path, args: &[&str]) -> Result<Command, FalhaDeFerramenta> {
    Ok(sbf_processo::comando(programa, args, &[])?)
}

/// Roda o programa com `entrada` pela entrada padrão e devolve a saída.
pub(crate) fn rodar_com_entrada(
    programa: &Path,
    args: &[&str],
    entrada: Zeroizing<Vec<u8>>,
    limite: Duration,
) -> Result<Saida, FalhaDeFerramenta> {
    let saida = Chamada::nova(programa)
        .argumentos(args)
        .com_entrada(entrada)
        .com_prazo(limite)
        .com_saida_maxima(SAIDA_MAXIMA_DO_FILHO)
        .com_erro_maximo(ERRO_MAXIMO_DO_FILHO)
        .rodar()?;
    Ok(Saida {
        status: saida.status,
        stdout: saida.stdout,
    })
}
