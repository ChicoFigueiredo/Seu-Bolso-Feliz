//! Uma chamada a um programa externo: entrada, saída e erro em pipe, prazo, e teto do que é
//! guardado da saída.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

use crate::programa::{ambiente_minimo, confiavel};

/// Por que um programa externo não rodou até o fim. Sem saída nem argumento dentro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum FalhaDeProcesso {
    /// O programa não foi encontrado, não pôde ser executado, ou não é [`confiavel`].
    #[error("programa não encontrado")]
    NaoEncontrado,
    /// Passou do prazo e foi morto.
    #[error("o programa passou do limite de tempo")]
    TempoEsgotado,
    /// O caminho a abrir não é absoluto (um relativo começando com `-` viraria opção).
    #[error("o caminho precisa ser absoluto")]
    CaminhoRelativo,
    /// Falha de entrada e saída com o processo.
    #[error("falha de entrada e saída com o programa: {0}")]
    Io(std::io::ErrorKind),
}

impl From<std::io::Error> for FalhaDeProcesso {
    fn from(erro: std::io::Error) -> Self {
        match erro.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => {
                Self::NaoEncontrado
            }
            outro => Self::Io(outro),
        }
    }
}

/// O máximo guardado da saída padrão, se a chamada não disser outro. O resto é drenado e
/// descartado (achado A10 da revisão de segurança da fatia 1).
pub const SAIDA_MAXIMA_PADRAO: usize = 8 * 1024 * 1024;
/// O máximo guardado do erro padrão (o **fim** dele, onde fica a mensagem que interessa).
pub const ERRO_MAXIMO_PADRAO: usize = 64 * 1024;
/// O prazo de uma chamada que não diz outro.
pub const PRAZO_PADRAO: Duration = Duration::from_secs(30);

/// Um comando com a entrada, a saída e o erro em pipe e o ambiente mínimo (mais as
/// `herdadas`). Um programa que não é [`confiavel`] (relativo, ausente, ou em pasta gravável
/// por outros) dá [`FalhaDeProcesso::NaoEncontrado`] sem rodar.
///
/// # Errors
/// [`FalhaDeProcesso::NaoEncontrado`] se o programa não é confiável.
pub fn comando<I, S>(
    programa: &Path,
    argumentos: I,
    herdadas: &[&str],
) -> Result<Command, FalhaDeProcesso>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if !confiavel(programa) {
        return Err(FalhaDeProcesso::NaoEncontrado);
    }
    let mut c = Command::new(programa);
    ambiente_minimo(&mut c, herdadas);
    c.args(argumentos)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(c)
}

/// Escreve `bytes` na entrada do filho numa thread (para não travar com a saída cheia) e
/// fecha a entrada no fim. O buffer é zerado quando a thread termina. Um filho que fecha a
/// entrada antes (pipe quebrado) não é erro: quem decide é o código de saída.
pub fn escrever_e_fechar(
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

/// O que [`ler_com_limite`] guardou.
#[derive(Default)]
pub struct Lido {
    /// Os primeiros bytes, até o limite, num buffer zerado ao descartar.
    pub bytes: Zeroizing<Vec<u8>>,
    /// Havia mais do que o limite (o resto foi drenado e descartado).
    pub cortado: bool,
}

impl std::fmt::Debug for Lido {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lido")
            .field("bytes", &self.bytes.len())
            .field("cortado", &self.cortado)
            .finish()
    }
}

/// Lê de um pipe numa thread, guardando no máximo `limite` bytes num buffer zerável com
/// `capacidade` reservada (para uma saída sensível pequena não deixar cópias ao crescer). O
/// resto é lido e jogado fora, para o filho não travar com o pipe cheio.
pub fn ler_com_limite<R: Read + Send + 'static>(
    fonte: Option<R>,
    limite: usize,
    capacidade: usize,
) -> JoinHandle<Lido> {
    std::thread::spawn(move || {
        let mut lido = Lido {
            bytes: Zeroizing::new(Vec::with_capacity(capacidade.min(limite))),
            cortado: false,
        };
        if let Some(mut fonte) = fonte {
            let teto = u64::try_from(limite).unwrap_or(u64::MAX);
            let _ = (&mut fonte).take(teto).read_to_end(&mut lido.bytes);
            let resto = std::io::copy(&mut fonte, &mut std::io::sink()).unwrap_or(0);
            lido.cortado = resto > 0;
        }
        lido
    })
}

/// Lê de um pipe numa thread, guardando só os **últimos** `limite` bytes (a mensagem de erro
/// costuma estar no fim). Lê tudo, para o filho não travar.
pub fn ler_o_fim<R: Read + Send + 'static>(fonte: Option<R>, limite: usize) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut fim = Vec::new();
        let Some(mut fonte) = fonte else {
            return fim;
        };
        let mut pedaco = [0_u8; 8192];
        loop {
            match fonte.read(&mut pedaco) {
                Ok(0) => break,
                Ok(n) => {
                    fim.extend_from_slice(&pedaco[..n]);
                    if fim.len() > limite.saturating_mul(2).max(pedaco.len()) {
                        let sobra = fim.len() - limite;
                        fim.drain(..sobra);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        if fim.len() > limite {
            let sobra = fim.len() - limite;
            fim.drain(..sobra);
        }
        fim
    })
}

/// Espera o filho até o prazo (`None`: sem prazo); passado, mata e espera.
///
/// # Errors
/// [`FalhaDeProcesso::TempoEsgotado`] se passou do prazo; `Io` se a espera falhou.
pub fn esperar(filho: &mut Child, prazo: Option<Instant>) -> Result<ExitStatus, FalhaDeProcesso> {
    loop {
        if let Some(status) = filho.try_wait()? {
            return Ok(status);
        }
        if prazo.is_some_and(|p| Instant::now() >= p) {
            let _ = filho.kill();
            let _ = filho.wait();
            return Err(FalhaDeProcesso::TempoEsgotado);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// O que um filho devolveu.
pub struct Saida {
    pub status: ExitStatus,
    /// A saída padrão até o teto, num buffer zerado ao descartar.
    pub stdout: Zeroizing<Vec<u8>>,
    /// A saída padrão passou do teto (e foi cortada).
    pub stdout_cortado: bool,
    /// O fim do erro padrão, até o teto.
    pub stderr: Vec<u8>,
}

impl std::fmt::Debug for Saida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saida")
            .field("status", &self.status)
            .field("stdout", &self.stdout.len())
            .field("stdout_cortado", &self.stdout_cortado)
            .field("stderr", &self.stderr.len())
            .finish()
    }
}

/// Uma chamada a um programa externo, montada aos poucos e rodada com [`Chamada::rodar`].
///
/// O padrão: sem entrada (`/dev/null`), só [`crate::VARIAVEIS_BASICAS`] no ambiente,
/// [`PRAZO_PADRAO`], [`SAIDA_MAXIMA_PADRAO`] e [`ERRO_MAXIMO_PADRAO`].
pub struct Chamada {
    programa: PathBuf,
    argumentos: Vec<OsString>,
    herdadas: Vec<String>,
    variaveis: Vec<(String, Zeroizing<String>)>,
    entrada: Option<Zeroizing<Vec<u8>>>,
    prazo: Option<Duration>,
    saida_maxima: usize,
    capacidade_da_saida: usize,
    erro_maximo: usize,
}

/// Sem os valores: nem os argumentos, nem as variáveis, nem a entrada aparecem num log.
impl std::fmt::Debug for Chamada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nomes: Vec<&str> = self.variaveis.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("Chamada")
            .field("programa", &self.programa)
            .field("argumentos", &self.argumentos.len())
            .field("herdadas", &self.herdadas)
            .field("variaveis", &nomes)
            .field("entrada", &self.entrada.as_ref().map(|e| e.len()))
            .field("prazo", &self.prazo)
            .finish_non_exhaustive()
    }
}

impl Chamada {
    /// Uma chamada a `programa` (de preferência resolvido por [`crate::localizar`]).
    pub fn nova(programa: impl Into<PathBuf>) -> Self {
        Self {
            programa: programa.into(),
            argumentos: Vec::new(),
            herdadas: Vec::new(),
            variaveis: Vec::new(),
            entrada: None,
            prazo: Some(PRAZO_PADRAO),
            saida_maxima: SAIDA_MAXIMA_PADRAO,
            capacidade_da_saida: 0,
            erro_maximo: ERRO_MAXIMO_PADRAO,
        }
    }

    /// Acrescenta argumentos. **Nada sensível aqui**: o argv aparece no `ps`.
    #[must_use]
    pub fn argumentos<I, S>(mut self, argumentos: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.argumentos
            .extend(argumentos.into_iter().map(|a| a.as_ref().to_owned()));
        self
    }

    /// Variáveis do app que o filho herda, se existirem, além das básicas.
    #[must_use]
    pub fn herdando(mut self, nomes: &[&str]) -> Self {
        self.herdadas.extend(nomes.iter().map(|n| (*n).to_owned()));
        self
    }

    /// Uma variável só no ambiente do filho (o lugar de um segredo que o programa só aceita
    /// assim, como a senha ofuscada do rclone).
    #[must_use]
    pub fn com_variavel(mut self, nome: impl Into<String>, valor: Zeroizing<String>) -> Self {
        self.variaveis.push((nome.into(), valor));
        self
    }

    /// O que vai pela entrada padrão (que fecha no fim).
    #[must_use]
    pub fn com_entrada(mut self, bytes: Zeroizing<Vec<u8>>) -> Self {
        self.entrada = Some(bytes);
        self
    }

    /// Troca o prazo ([`PRAZO_PADRAO`]).
    #[must_use]
    pub fn com_prazo(mut self, prazo: Duration) -> Self {
        self.prazo = Some(prazo);
        self
    }

    /// Sem prazo de relógio (uma transferência que pode levar horas e tem o próprio
    /// `--timeout`).
    #[must_use]
    pub fn sem_prazo(mut self) -> Self {
        self.prazo = None;
        self
    }

    /// Troca o teto da saída padrão ([`SAIDA_MAXIMA_PADRAO`]).
    #[must_use]
    pub fn com_saida_maxima(mut self, bytes: usize) -> Self {
        self.saida_maxima = bytes;
        self
    }

    /// Reserva a capacidade do buffer da saída, para uma saída sensível pequena não deixar
    /// cópias ao crescer.
    #[must_use]
    pub fn com_capacidade_da_saida(mut self, bytes: usize) -> Self {
        self.capacidade_da_saida = bytes;
        self
    }

    /// Troca o teto do fim do erro padrão ([`ERRO_MAXIMO_PADRAO`]).
    #[must_use]
    pub fn com_erro_maximo(mut self, bytes: usize) -> Self {
        self.erro_maximo = bytes;
        self
    }

    /// Roda e devolve a saída, seja qual for o código de saída (quem chama decide).
    ///
    /// # Errors
    /// [`FalhaDeProcesso::NaoEncontrado`] (ausente ou não confiável),
    /// [`FalhaDeProcesso::TempoEsgotado`] (passou do prazo e foi morto) ou `Io`.
    pub fn rodar(self) -> Result<Saida, FalhaDeProcesso> {
        let prazo = self.prazo.map(|d| Instant::now() + d);
        let herdadas: Vec<&str> = self.herdadas.iter().map(String::as_str).collect();
        let mut c = comando(&self.programa, &self.argumentos, &herdadas)?;
        if self.entrada.is_none() {
            c.stdin(Stdio::null());
        }
        for (nome, valor) in &self.variaveis {
            c.env(nome, valor.as_str());
        }
        let mut filho = c.spawn()?;
        let escrita = match (self.entrada, filho.stdin.take()) {
            (Some(bytes), Some(stdin)) => Some(escrever_e_fechar(stdin, bytes)),
            _ => None,
        };
        let stdout = ler_com_limite(
            filho.stdout.take(),
            self.saida_maxima,
            self.capacidade_da_saida,
        );
        let stderr = ler_o_fim(filho.stderr.take(), self.erro_maximo);
        // Passou do prazo: o filho já foi morto. As threads de leitura não são esperadas,
        // porque um neto que herdou os pipes os manteria abertos e o prazo não valeria; elas
        // terminam sozinhas quando o último pipe fechar.
        let status = esperar(&mut filho, prazo)?;
        let escrita = escrita.map(JoinHandle::join);
        let lido = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        if let Some(Ok(Err(e))) = escrita {
            return Err(e.into());
        }
        Ok(Saida {
            status,
            stdout: lido.bytes,
            stdout_cortado: lido.cortado,
            stderr,
        })
    }
}
