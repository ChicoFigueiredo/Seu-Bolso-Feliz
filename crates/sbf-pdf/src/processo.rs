//! Os processos filhos (`pdftotext`, `qpdf`): o que é sensível entra só pela entrada padrão,
//! nunca pelo argv nem pelo ambiente, e todo filho tem limite de tempo e é morto ao passar.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
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

impl From<std::io::Error> for FalhaDeFerramenta {
    fn from(erro: std::io::Error) -> Self {
        match erro.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
                Self::NaoEncontrada
            }
            outro => Self::Io(outro),
        }
    }
}

/// O que um filho devolveu.
#[derive(Debug)]
pub(crate) struct Saida {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
}

/// Um comando com a entrada, a saída e o erro em pipe.
pub(crate) fn comando(programa: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(programa);
    c.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

/// Escreve `bytes` na entrada do filho numa thread (para não travar com a saída cheia) e
/// fecha a entrada no fim. O buffer é zerado quando a thread termina. Um filho que fecha a
/// entrada antes (pipe quebrado) não é erro: quem decide é o código de saída.
pub(crate) fn escrever_e_fechar(
    entrada: ChildStdin,
    bytes: Zeroizing<Vec<u8>>,
) -> JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let mut entrada = entrada;
        match entrada.write_all(&bytes) {
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            outro => outro,
        }
    })
}

/// Lê tudo de um pipe numa thread.
pub(crate) fn ler_tudo<R: Read + Send + 'static>(fonte: Option<R>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut saida = Vec::new();
        if let Some(mut fonte) = fonte {
            let _ = fonte.read_to_end(&mut saida);
        }
        saida
    })
}

/// Espera o filho até o prazo; passado, mata e espera.
pub(crate) fn esperar(filho: &mut Child, prazo: Instant) -> Result<ExitStatus, FalhaDeFerramenta> {
    loop {
        if let Some(status) = filho.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= prazo {
            let _ = filho.kill();
            let _ = filho.wait();
            return Err(FalhaDeFerramenta::TempoEsgotado);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Roda o programa com `entrada` pela entrada padrão e devolve a saída.
pub(crate) fn rodar_com_entrada(
    programa: &Path,
    args: &[&str],
    entrada: Zeroizing<Vec<u8>>,
    limite: Duration,
) -> Result<Saida, FalhaDeFerramenta> {
    let prazo = Instant::now() + limite;
    let mut filho = comando(programa, args).spawn()?;
    let escrita = filho
        .stdin
        .take()
        .map(|stdin| escrever_e_fechar(stdin, entrada));
    let stdout = ler_tudo(filho.stdout.take());
    let stderr = ler_tudo(filho.stderr.take());
    let status = esperar(&mut filho, prazo);
    // Com o filho morto, os pipes fecham e as threads terminam.
    let escrita = escrita.map(|t| t.join());
    let stdout = stdout.join().unwrap_or_default();
    // O erro só é drenado (para o filho não travar com o pipe cheio); não é guardado nem
    // mostrado.
    let _ = stderr.join();
    let status = status?;
    if let Some(Ok(Err(e))) = escrita {
        return Err(e.into());
    }
    Ok(Saida { status, stdout })
}
