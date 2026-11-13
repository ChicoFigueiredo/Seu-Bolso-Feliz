//! Chamada ao `rclone`, sempre por `Command` com lista de argumentos (sem shell) e com
//! segredos só no ambiente do processo filho ou na entrada padrão, nunca em argumento.
//!
//! # Qual rclone e com que ambiente (achados A6 e A7 da revisão de segurança da fatia 1)
//!
//! - [`Rclone::novo`] resolve o caminho **uma vez**, nas pastas do sistema
//!   ([`PASTAS_DO_SISTEMA`]), nunca pelo `PATH`: um `rclone` plantado em `~/.bun/bin` ou num
//!   shim receberia a senha ofuscada da cópia. [`Rclone::com_binario`] é a configuração
//!   explícita. Um caminho relativo, ausente ou em pasta gravável por outros não roda
//!   ([`Erro::RcloneAusente`]). [`Rclone::binario`] é o caminho a mostrar em Opções.
//! - O filho roda com `env_clear`: só um `PATH` mínimo, o idioma, o que o rclone precisa para
//!   achar a configuração do usuário (`HOME`, `XDG_CONFIG_HOME`, `RCLONE_CONFIG`,
//!   `RCLONE_CONFIG_PASS`), o cache, a pasta temporária, os proxies e os certificados, e as
//!   variáveis que o próprio app põe. Nenhum outro `RCLONE_*` do shell (por exemplo
//!   `RCLONE_DRY_RUN` ou um `PASSWORD2` alheio) muda o comportamento da cópia.
//!
//! A regra é a mesma do `sbf-pdf/src/ferramenta.rs` (os dois crates não dependem um do outro).

use crate::erro::{Erro, Resultado};
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Variável de ambiente para o filho: nome e valor (zerado ao descartar).
pub type VariavelDeAmbiente = (String, Zeroizing<String>);

/// Onde procurar o rclone, em ordem.
#[cfg(all(unix, not(target_os = "macos")))]
pub const PASTAS_DO_SISTEMA: &[&str] = &["/usr/bin", "/usr/local/bin", "/bin"];
/// Onde procurar o rclone, em ordem.
#[cfg(target_os = "macos")]
pub const PASTAS_DO_SISTEMA: &[&str] = &[
    "/usr/bin",
    "/usr/local/bin",
    "/opt/homebrew/bin",
    "/opt/local/bin",
];
/// No Windows a pasta vem de `%ProgramFiles%\rclone`.
#[cfg(windows)]
pub const PASTAS_DO_SISTEMA: &[&str] = &[];

#[cfg(unix)]
const PATH_MINIMO: &str = "/usr/bin:/bin";

/// O que passa do app para o rclone, se existir.
const VARIAVEIS_PERMITIDAS: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "TMPDIR",
    "RCLONE_CONFIG",
    "RCLONE_CONFIG_PASS",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    #[cfg(windows)]
    "SystemRoot",
    #[cfg(windows)]
    "SystemDrive",
    #[cfg(windows)]
    "windir",
    #[cfg(windows)]
    "TEMP",
    #[cfg(windows)]
    "TMP",
    #[cfg(windows)]
    "APPDATA",
    #[cfg(windows)]
    "LOCALAPPDATA",
    #[cfg(windows)]
    "USERPROFILE",
    #[cfg(windows)]
    "HOMEDRIVE",
    #[cfg(windows)]
    "HOMEPATH",
];

fn pastas_do_sistema() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(|raiz| PathBuf::from(raiz).join("rclone"))
            .collect()
    }
    #[cfg(not(windows))]
    {
        PASTAS_DO_SISTEMA.iter().map(PathBuf::from).collect()
    }
}

/// O caminho absoluto do `rclone` na primeira pasta do sistema em que ele existe e é
/// [`confiavel`]. Nunca olha o `PATH`.
#[must_use]
pub fn localizar_rclone() -> Option<PathBuf> {
    let nome = if cfg!(windows) {
        "rclone.exe"
    } else {
        "rclone"
    };
    pastas_do_sistema()
        .into_iter()
        .map(|pasta| pasta.join(nome))
        .find(|caminho| confiavel(caminho))
}

/// `true` se `caminho` é absoluto, é um arquivo e nem ele nem a pasta dele (depois de seguir
/// links) podem ser escritos por outros usuários.
#[must_use]
pub fn confiavel(caminho: &Path) -> bool {
    if !caminho.is_absolute() {
        return false;
    }
    let Ok(real) = std::fs::canonicalize(caminho) else {
        return false;
    };
    if !real.is_file() {
        return false;
    }
    real.parent().is_some_and(sem_escrita_alheia) && sem_escrita_alheia(&real)
}

#[cfg(unix)]
fn sem_escrita_alheia(caminho: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(dados) = std::fs::metadata(caminho) else {
        return false;
    };
    let modo = dados.mode();
    let outros = modo & 0o002 != 0;
    // Grupo com escrita só vale para o grupo do root ou o grupo próprio do dono (gid = uid).
    let grupo = modo & 0o020 != 0 && dados.gid() != 0 && dados.gid() != dados.uid();
    !outros && !grupo
}

/// No Windows a permissão é por ACL, que a std não lê: vale o caminho absoluto.
#[cfg(windows)]
fn sem_escrita_alheia(_caminho: &Path) -> bool {
    true
}

fn ambiente_minimo(comando: &mut Command) {
    comando.env_clear();
    for nome in VARIAVEIS_PERMITIDAS {
        if let Some(valor) = std::env::var_os(nome) {
            comando.env(nome, valor);
        }
    }
    #[cfg(unix)]
    comando.env("PATH", PATH_MINIMO);
    #[cfg(windows)]
    if let Some(raiz) = std::env::var_os("SystemRoot") {
        comando.env("PATH", PathBuf::from(raiz).join("System32"));
    }
}

/// Prazo padrão das chamadas curtas (listar, ler o sal, ofuscar, renomear, apagar): um remoto
/// travado não trava a ação (achado A9 da revisão de segurança da fatia 1). As transferências
/// (`copy`, `copyto`) não têm prazo de relógio, porque o acervo pode levar horas; nelas vale
/// o `--timeout` do próprio rclone (5 min sem tráfego).
pub const PRAZO_PADRAO: Duration = Duration::from_secs(5 * 60);
/// Subcomandos que transferem dados e ficam sem prazo de relógio.
const TRANSFERENCIAS: &[&str] = &["copy", "copyto"];

/// O rclone instalado.
#[derive(Debug, Clone)]
pub struct Rclone {
    binario: PathBuf,
    prazo: Duration,
}

impl Default for Rclone {
    fn default() -> Self {
        Self::novo()
    }
}

impl Rclone {
    /// Usa o `rclone` das pastas do sistema ([`localizar_rclone`]). Se não houver, as chamadas
    /// dão [`Erro::RcloneAusente`].
    #[must_use]
    pub fn novo() -> Self {
        Self {
            binario: localizar_rclone().unwrap_or_default(),
            prazo: PRAZO_PADRAO,
        }
    }

    /// O executável que será chamado (para Opções e Bastidores). Vazio se não foi achado.
    #[must_use]
    pub fn binario(&self) -> &Path {
        &self.binario
    }

    /// Usa outro executável (configuração explícita, ou testes). Precisa ser absoluto e fora
    /// de pasta gravável por outros.
    #[must_use]
    pub fn com_binario(binario: impl Into<PathBuf>) -> Self {
        Self {
            binario: binario.into(),
            prazo: PRAZO_PADRAO,
        }
    }

    /// Troca o prazo das chamadas curtas ([`PRAZO_PADRAO`]).
    #[must_use]
    pub fn com_prazo(mut self, prazo: Duration) -> Self {
        self.prazo = prazo;
        self
    }

    /// Roda o rclone e devolve o stdout. `entrada` vai na entrada padrão; `ambiente` só no
    /// ambiente do filho.
    pub(crate) fn executar<I, S>(
        &self,
        argumentos: I,
        ambiente: &[VariavelDeAmbiente],
        entrada: Option<&str>,
    ) -> Resultado<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let argumentos: Vec<OsString> = argumentos
            .into_iter()
            .map(|a| a.as_ref().to_owned())
            .collect();
        let comando = argumentos
            .first()
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !confiavel(&self.binario) {
            return Err(Erro::RcloneAusente);
        }
        let mut processo = Command::new(&self.binario);
        ambiente_minimo(&mut processo);
        processo
            .args(&argumentos)
            .stdin(if entrada.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (nome, valor) in ambiente {
            processo.env(nome, valor.as_str());
        }
        let mut filho = processo.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Erro::RcloneAusente
            } else {
                Erro::Io(e)
            }
        })?;
        if let (Some(texto), Some(mut stdin)) = (entrada, filho.stdin.take()) {
            stdin.write_all(texto.as_bytes())?;
            // Fecha a entrada para o rclone ver o fim.
            drop(stdin);
        }
        // stdout e stderr em threads, para nenhum pipe encher e travar o filho, e para o prazo
        // valer mesmo com o filho calado.
        let mut stderr = filho.stderr.take();
        let leitor = std::thread::spawn(move || {
            let mut texto = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut texto);
            }
            texto
        });
        let mut stdout = filho.stdout.take();
        let leitor_da_saida = std::thread::spawn(move || {
            let mut texto = String::new();
            if let Some(s) = stdout.as_mut() {
                let _ = s.read_to_string(&mut texto);
            }
            texto
        });
        let prazo =
            (!TRANSFERENCIAS.contains(&comando.as_str())).then(|| Instant::now() + self.prazo);
        let estado = loop {
            if let Some(estado) = filho.try_wait()? {
                break estado;
            }
            if prazo.is_some_and(|p| Instant::now() >= p) {
                let _ = filho.kill();
                let _ = filho.wait();
                let _ = leitor.join();
                let _ = leitor_da_saida.join();
                return Err(Erro::TempoEsgotado { comando });
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let saida = leitor_da_saida.join().unwrap_or_default();
        let erro = leitor.join().unwrap_or_default();
        if estado.success() {
            return Ok(saida);
        }
        Err(Erro::Rclone {
            comando,
            codigo: estado.code(),
            mensagem: fim_do_stderr(&erro),
        })
    }

    /// `rclone obscure -`: a senha vai pela entrada padrão e volta no formato que o rclone
    /// aceita em `RCLONE_CONFIG_<NOME>_PASSWORD`. Não é cifra, só ofuscação reversível.
    pub(crate) fn ofuscar(&self, senha: &str) -> Resultado<Zeroizing<String>> {
        let saida = self.executar(["obscure", "-"], &[], Some(senha))?;
        Ok(Zeroizing::new(saida.trim().to_owned()))
    }
}

fn fim_do_stderr(texto: &str) -> String {
    let linhas: Vec<&str> = texto.lines().filter(|l| !l.trim().is_empty()).collect();
    let inicio = linhas.len().saturating_sub(3);
    let junto = linhas[inicio..].join(" | ");
    junto.chars().take(400).collect()
}

/// Junta o remoto/pasta base e um caminho relativo.
pub(crate) fn juntar(base: &str, relativo: &str) -> String {
    if base.ends_with(':') || base.ends_with('/') {
        format!("{base}{relativo}")
    } else {
        format!("{base}/{relativo}")
    }
}

/// Rejeita destino vazio ou que o rclone leria como opção.
pub(crate) fn validar_destino(destino: &str) -> Resultado<()> {
    if destino.trim().is_empty() || destino.starts_with('-') {
        return Err(Erro::DestinoInvalido(destino.to_owned()));
    }
    Ok(())
}
