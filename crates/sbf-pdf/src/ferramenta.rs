//! Onde estão o `pdftotext` e o `qpdf`, e com que ambiente rodam (achados A6 e A7 da
//! revisão de segurança da fatia 1).
//!
//! - **Caminho absoluto, resolvido uma vez**, nas pastas do sistema ([`PASTAS_DO_SISTEMA`]),
//!   nunca pelo `PATH` do usuário: um executável com o mesmo nome plantado em `~/.bun/bin`,
//!   num shim ou em `/usr/local/sbin` receberia a senha candidata ou o PDF decifrado.
//! - **Nada em pasta gravável por outros**: o arquivo e a pasta dele (depois de seguir links)
//!   não podem ter escrita para "outros", nem para o grupo se o grupo não for o do root nem o
//!   grupo próprio do dono.
//!   Vale para o caminho encontrado e para o configurado à mão em [`crate::Ferramentas`].
//! - **Ambiente limpo**: o filho não herda tokens nem chaves de API do shell; recebe só um
//!   `PATH` mínimo e o idioma (`LANG`, `LC_ALL`, `LC_CTYPE`), e no Windows as variáveis sem
//!   as quais um processo não sobe.
//!
//! O `sbf-estante` tem a mesma regra para o `rclone` (cópia em `rclone.rs`; os dois crates
//! não dependem um do outro).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Onde procurar, em ordem. Linux: `/usr/bin` primeiro.
#[cfg(all(unix, not(target_os = "macos")))]
pub const PASTAS_DO_SISTEMA: &[&str] = &["/usr/bin", "/usr/local/bin", "/bin"];
/// Onde procurar, em ordem. macOS: o sistema, depois o Homebrew e o MacPorts.
#[cfg(target_os = "macos")]
pub const PASTAS_DO_SISTEMA: &[&str] = &[
    "/usr/bin",
    "/usr/local/bin",
    "/opt/homebrew/bin",
    "/opt/local/bin",
];
/// No Windows as pastas vêm de `%ProgramFiles%` (ver [`pastas_do_sistema`]).
#[cfg(windows)]
pub const PASTAS_DO_SISTEMA: &[&str] = &[];

/// `PATH` dos filhos: só o sistema (um script de embrulho ainda acha `sh`, `cat`...).
#[cfg(unix)]
const PATH_MINIMO: &str = "/usr/bin:/bin";

/// Variáveis que passam do app para o filho, se existirem. Nenhuma é segredo.
#[cfg(unix)]
const VARIAVEIS_PERMITIDAS: &[&str] = &["LANG", "LC_ALL", "LC_CTYPE"];
#[cfg(windows)]
const VARIAVEIS_PERMITIDAS: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "SystemRoot",
    "SystemDrive",
    "windir",
    "TEMP",
    "TMP",
];

/// As pastas onde [`localizar`] procura, em ordem.
#[must_use]
pub fn pastas_do_sistema() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut pastas = Vec::new();
        for raiz in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(raiz) = std::env::var_os(raiz).map(PathBuf::from) {
                for sub in [
                    "qpdf\\bin",
                    "poppler\\Library\\bin",
                    "poppler\\bin",
                    "rclone",
                ] {
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

/// O nome do executável no sistema (`qpdf.exe` no Windows).
fn nome_do_executavel(nome: &str) -> String {
    if cfg!(windows) {
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
    let pasta_ok = real.parent().is_some_and(sem_escrita_alheia);
    pasta_ok && sem_escrita_alheia(&real)
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
/// `%ProgramFiles%`, que só o administrador escreve.
#[cfg(windows)]
fn sem_escrita_alheia(_caminho: &Path) -> bool {
    true
}

/// Limpa o ambiente do filho e põe só o necessário.
pub(crate) fn ambiente_minimo(comando: &mut Command) {
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

/// O caminho resolvido, ou o próprio nome se não achou (que [`confiavel`] recusa: a chamada
/// dá "ferramenta não encontrada" em vez de cair no `PATH`).
pub(crate) fn localizar_ou_nome(nome: &str) -> PathBuf {
    localizar(nome).unwrap_or_else(|| PathBuf::from(OsStr::new(nome)))
}
