//! O Chrome visível dos roteiros. O motor (CDP, perfil dedicado, ponte, página simplificada)
//! vive em `bancada-navegador`; aqui ficam as duas peças que são do SBF:
//!
//! - o [`Lancador`] sobre a regra única de processo externo (`sbf-processo`, ADR-012):
//!   caminho absoluto de confiança, ambiente mínimo, nada herdado do shell;
//! - a [`Politica`] injetada: a lista de bloqueio do SBF ([`ListaDeBloqueio`]).
//!
//! [`abrir`] junta as duas. Os tipos do motor são reexportados daqui para o resto do SBF.

pub use bancada_navegador::chrome::{
    Chrome, Lancador, OpcoesDoChrome, TravaDoPerfil, VARIAVEIS_DO_CHROME,
    apagar_dados_de_formulario, argumentos_do_chrome, preparar_preferencias,
};

use crate::bloqueio::{ListaDeBloqueio, Politica};
use crate::erro::Resultado;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;

/// O [`Lancador`] do SBF: acha o Chrome e o sobe pela regra única de processo externo.
#[derive(Debug, Clone, Copy, Default)]
pub struct LancadorDoSbf;

impl Lancador for LancadorDoSbf {
    fn localizar_chrome(&self) -> Option<PathBuf> {
        localizar_chrome()
    }

    fn lancar(&self, programa: &Path, args: &[String]) -> Result<Child, String> {
        let mut cmd = sbf_processo::comando(programa, args, VARIAVEIS_DO_CHROME)
            .map_err(|_| "o Chrome não está num lugar de confiança".to_owned())?;
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd.spawn().map_err(|e| format!("não abriu o Chrome: {e}"))
    }

    fn sem_sandbox(&self) -> bool {
        std::env::var_os("SBF_CHROME_SEM_SANDBOX").is_some()
    }
}

/// Procura o Chrome ou Chromium instalado, só em pastas de confiança.
#[must_use]
pub fn localizar_chrome() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SBF_CHROME").map(PathBuf::from)
        && sbf_processo::confiavel(&p)
    {
        return Some(p);
    }
    for nome in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
    ] {
        if let Some(p) = sbf_processo::localizar(nome) {
            return Some(p);
        }
    }
    #[cfg(target_os = "macos")]
    for p in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ] {
        let p = PathBuf::from(p);
        if sbf_processo::confiavel(&p) {
            return Some(p);
        }
    }
    #[cfg(windows)]
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(var) {
            let p = PathBuf::from(base).join("Google/Chrome/Application/chrome.exe");
            if sbf_processo::confiavel(&p) {
                return Some(p);
            }
        }
    }
    None
}

/// Abre o Chrome com a lista de bloqueio do SBF e o lançador do SBF.
///
/// # Errors
/// Chrome não encontrado, não subiu a tempo, ou falha do CDP.
pub fn abrir(opcoes: &OpcoesDoChrome) -> Resultado<Chrome> {
    let politica: Arc<dyn Politica + Send + Sync> = Arc::new(ListaDeBloqueio);
    Ok(Chrome::abrir(opcoes, &LancadorDoSbf, politica)?)
}
