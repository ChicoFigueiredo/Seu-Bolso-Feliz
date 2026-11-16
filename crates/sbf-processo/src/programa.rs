//! Qual programa roda: caminho absoluto nas pastas do sistema, nada gravável por outros, e
//! com que ambiente.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Onde procurar, em ordem. Linux: `/usr/bin` primeiro; `sbin` por último (só o root
/// escreve lá).
#[cfg(all(unix, not(target_os = "macos")))]
pub const PASTAS_DO_SISTEMA: &[&str] =
    &["/usr/bin", "/usr/local/bin", "/bin", "/usr/sbin", "/sbin"];
/// Onde procurar, em ordem. macOS: o sistema (`df` está em `/bin`, `diskutil` em
/// `/usr/sbin`), depois o Homebrew e o MacPorts.
#[cfg(target_os = "macos")]
pub const PASTAS_DO_SISTEMA: &[&str] = &[
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
    "/usr/local/bin",
    "/opt/homebrew/bin",
    "/opt/local/bin",
];
/// No Windows as pastas vêm de `%SystemRoot%` e `%ProgramFiles%` (ver [`pastas_do_sistema`]).
#[cfg(windows)]
pub const PASTAS_DO_SISTEMA: &[&str] = &[];

/// `PATH` dos filhos: só o sistema (um script de embrulho ainda acha `sh`, `cat`...).
#[cfg(unix)]
pub(crate) const PATH_MINIMO: &str = "/usr/bin:/bin";

/// Variáveis que passam do app para todo filho, se existirem. Nenhuma é segredo.
#[cfg(not(windows))]
pub const VARIAVEIS_BASICAS: &[&str] = &["LANG", "LC_ALL", "LC_CTYPE"];
/// Variáveis que passam do app para todo filho, se existirem. Nenhuma é segredo.
#[cfg(windows)]
pub const VARIAVEIS_BASICAS: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "SystemRoot",
    "SystemDrive",
    "windir",
    "TEMP",
    "TMP",
];

/// As subpastas de `%ProgramFiles%` onde moram as ferramentas que o app usa no Windows.
#[cfg(windows)]
const SUBPASTAS_DE_PROGRAMAS: &[&str] = &[
    "qpdf\\bin",
    "poppler\\Library\\bin",
    "poppler\\bin",
    "rclone",
];

/// As pastas onde [`localizar`] procura, em ordem.
#[must_use]
pub fn pastas_do_sistema() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut pastas = Vec::new();
        if let Some(raiz) = std::env::var_os("SystemRoot").map(PathBuf::from) {
            pastas.push(raiz.join("System32"));
            pastas.push(raiz.join("System32\\WindowsPowerShell\\v1.0"));
            pastas.push(raiz);
        }
        for raiz in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(raiz) = std::env::var_os(raiz).map(PathBuf::from) {
                for sub in SUBPASTAS_DE_PROGRAMAS {
                    pastas.push(raiz.join(sub));
                }
            }
        }
        pastas
    }
    #[cfg(not(windows))]
    {
        PASTAS_DO_SISTEMA.iter().map(PathBuf::from).collect()
    }
}

/// O nome do executável no sistema (`qpdf.exe` no Windows; um nome que já termina em `.exe`
/// fica como está).
fn nome_do_executavel(nome: &str) -> String {
    if cfg!(windows) && !nome.to_ascii_lowercase().ends_with(".exe") {
        format!("{nome}.exe")
    } else {
        nome.to_owned()
    }
}

/// O caminho absoluto de `nome` na primeira pasta do sistema em que ele existe e é
/// [`confiavel`]. Nunca olha o `PATH`.
#[must_use]
pub fn localizar(nome: &str) -> Option<PathBuf> {
    let executavel = nome_do_executavel(nome);
    pastas_do_sistema()
        .into_iter()
        .map(|pasta| pasta.join(&executavel))
        .find(|caminho| confiavel(caminho))
}

/// O caminho resolvido, ou o próprio nome se não achou (que [`confiavel`] recusa: a chamada
/// dá "programa não encontrado" em vez de cair no `PATH`).
#[must_use]
pub fn localizar_ou_nome(nome: &str) -> PathBuf {
    localizar(nome).unwrap_or_else(|| PathBuf::from(OsStr::new(nome)))
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
    let Ok(dados) = std::fs::metadata(&real) else {
        return false;
    };
    if !dados.is_file() {
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
    // Grupo com escrita só vale se o grupo é o do root ou o grupo próprio do dono (o "user
    // private group" do Ubuntu e do Fedora, em que gid = uid). `/usr/local/bin` do Debian
    // antigo é `root:staff 2775`, e o `staff` não é de confiança.
    let grupo = modo & 0o020 != 0 && dados.gid() != 0 && dados.gid() != dados.uid();
    !outros && !grupo
}

/// No Windows a permissão é por ACL, que a std não lê: vale o caminho absoluto em
/// `%SystemRoot%` e `%ProgramFiles%`, que só o administrador escreve.
#[cfg(windows)]
fn sem_escrita_alheia(_caminho: &Path) -> bool {
    true
}

/// Limpa o ambiente do filho e põe só o necessário: [`VARIAVEIS_BASICAS`], as `herdadas` que
/// existirem no app, e o `PATH` do sistema.
pub fn ambiente_minimo(comando: &mut Command, herdadas: &[&str]) {
    comando.env_clear();
    for nome in VARIAVEIS_BASICAS.iter().chain(herdadas) {
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
