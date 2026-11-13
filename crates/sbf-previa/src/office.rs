//! Office (xls, xlsx, xlsm, doc, docx, rtf, odt, ods): convertido em PDF pelo LibreOffice
//! numa pasta 0700 dentro de `$XDG_RUNTIME_DIR` (tmpfs, em memória), com perfil próprio na
//! mesma pasta, prazo de 30 s e uma conversão por vez. A pasta é apagada na hora (guarda
//! RAII), dê certo ou não; [`faxinar_sobras`] apaga a de um app que morreu no meio.
//!
//! Sem `$XDG_RUNTIME_DIR` (ou se ela não for uma pasta só do usuário), a prévia não converte:
//! nunca cai para `/tmp`, que pode estar em disco.

use crate::{Conteudo, ErroDaPrevia};
use sbf_processo::{Chamada, localizar_ou_nome};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use zeroize::Zeroizing;

/// O nome da pasta da prévia dentro de `$XDG_RUNTIME_DIR`.
const PASTA_DA_PREVIA: &str = "sbf-previa";
/// O prefixo de cada conversão: `conversao-<pid>-<n>`.
const PREFIXO: &str = "conversao-";
/// O prazo de uma conversão.
const PRAZO: Duration = Duration::from_secs(30);

/// O LibreOffice não gosta de instâncias em paralelo: uma conversão por vez no processo.
static UMA_POR_VEZ: Mutex<()> = Mutex::new(());
static SEQUENCIA: AtomicU64 = AtomicU64::new(0);

/// Apaga a pasta ao sair do escopo, dê certo ou não.
struct PastaTemporaria(PathBuf);

impl Drop for PastaTemporaria {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sem_previa(motivo: impl Into<String>) -> Conteudo {
    Conteudo::SemPrevia {
        motivo: motivo.into(),
    }
}

/// `$XDG_RUNTIME_DIR/sbf-previa`, criada 0700 se faltar. `Err` com o motivo se não há uma
/// pasta em memória só do usuário.
#[cfg(unix)]
fn raiz() -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("sem $XDG_RUNTIME_DIR, a conversão do Office não tem pasta em memória")?;
    let dados = std::fs::metadata(&xdg).map_err(|_| "$XDG_RUNTIME_DIR não existe")?;
    // O dono do processo, sem libc: o dono do /proc/self.
    let eu = std::fs::metadata("/proc/self").map(|m| m.uid()).ok();
    if !dados.is_dir() || dados.mode() & 0o077 != 0 || eu.is_some_and(|u| u != dados.uid()) {
        return Err("$XDG_RUNTIME_DIR não é uma pasta só do usuário".to_owned());
    }
    let raiz = xdg.join(PASTA_DA_PREVIA);
    match std::fs::DirBuilder::new().mode(0o700).create(&raiz) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("não criou a pasta da prévia: {e}")),
    }
    let dados = std::fs::symlink_metadata(&raiz).map_err(|e| e.to_string())?;
    if !dados.is_dir() {
        return Err("a pasta da prévia não é uma pasta".to_owned());
    }
    if dados.mode() & 0o077 != 0 {
        std::fs::set_permissions(&raiz, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(raiz)
}

#[cfg(not(unix))]
fn raiz() -> Result<PathBuf, String> {
    Err("a prévia do Office só converte no Linux por enquanto".to_owned())
}

#[cfg(unix)]
fn criar_pasta(caminho: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(caminho)
}

#[cfg(not(unix))]
fn criar_pasta(caminho: &Path) -> std::io::Result<()> {
    std::fs::create_dir(caminho)
}

/// O processo `pid` está vivo (no Linux, pelo `/proc`; fora dele, supõe que sim).
fn vivo(pid: u32) -> bool {
    if Path::new("/proc/self").exists() {
        Path::new("/proc").join(pid.to_string()).exists()
    } else {
        true
    }
}

/// Apaga as pastas de conversão que um app morto deixou (a do processo atual e as de
/// processos vivos ficam). Chame na abertura do app. Devolve quantas apagou.
pub fn faxinar_sobras() -> usize {
    let Ok(raiz) = raiz() else {
        return 0;
    };
    faxinar_em(&raiz)
}

fn faxinar_em(raiz: &Path) -> usize {
    let Ok(entradas) = std::fs::read_dir(raiz) else {
        return 0;
    };
    let eu = std::process::id();
    let mut apagadas = 0;
    for entrada in entradas.flatten() {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        let Some(resto) = nome.strip_prefix(PREFIXO) else {
            continue;
        };
        let pid = resto.split('-').next().and_then(|p| p.parse::<u32>().ok());
        if pid.is_some_and(|p| p == eu || vivo(p)) {
            continue;
        }
        if std::fs::remove_dir_all(entrada.path()).is_ok() {
            apagadas += 1;
        }
    }
    apagadas
}

/// Mata (`kill -9`) os processos cujo argv cita `pasta`: o que sobrou de uma conversão que
/// passou do prazo. Só no Linux (pelo `/proc`).
fn matar_restos(pasta: &Path) {
    let agulha = pasta.as_os_str().as_encoded_bytes();
    let Ok(entradas) = std::fs::read_dir("/proc") else {
        return;
    };
    let eu = std::process::id().to_string();
    let pids: Vec<String> = entradas
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.bytes().all(|b| b.is_ascii_digit()) && *n != eu)
        .filter(|n| {
            std::fs::read(Path::new("/proc").join(n).join("cmdline"))
                .is_ok_and(|c| c.windows(agulha.len()).any(|j| j == agulha))
        })
        .collect();
    if pids.is_empty() {
        return;
    }
    let _ = Chamada::nova(localizar_ou_nome("kill"))
        .argumentos(std::iter::once("-9".to_owned()).chain(pids))
        .com_prazo(Duration::from_secs(5))
        .rodar();
}

/// Converte o arquivo em PDF e devolve as páginas.
pub(crate) fn converter(caminho: &Path, extensao: &str) -> Result<Conteudo, ErroDaPrevia> {
    let bytes = crate::ler(caminho)?;
    let raiz = match raiz() {
        Ok(r) => r,
        Err(motivo) => return Ok(sem_previa(motivo)),
    };
    let soffice = localizar_ou_nome("soffice");
    if !sbf_processo::confiavel(&soffice) {
        return Err(ErroDaPrevia::FaltaPrograma("soffice".to_owned()));
    }
    let _vez = UMA_POR_VEZ.lock().unwrap_or_else(|e| e.into_inner());
    let n = SEQUENCIA.fetch_add(1, Ordering::Relaxed);
    let pasta = raiz.join(format!("{PREFIXO}{}-{n}", std::process::id()));
    criar_pasta(&pasta).map_err(|e| ErroDaPrevia::Ilegivel(format!("pasta da conversão: {e}")))?;
    let guarda = PastaTemporaria(pasta);
    let pasta = guarda.0.as_path();
    let saida = pasta.join("saida");
    criar_pasta(&saida).map_err(|e| ErroDaPrevia::Ilegivel(e.to_string()))?;
    // O LibreOffice lê uma cópia em memória (tmpfs): nada de arquivo de trava ao lado do
    // original no cofre. O nome é fixo: nenhum nome do usuário vai para o argv.
    let extensao: String = extensao
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let entrada = pasta.join(format!("entrada.{extensao}"));
    std::fs::write(&entrada, &*bytes).map_err(|e| ErroDaPrevia::Ilegivel(e.to_string()))?;
    drop(bytes);

    let mut perfil = std::ffi::OsString::from("-env:UserInstallation=file://");
    perfil.push(pasta.join("perfil"));
    let pasta_texto = pasta.to_string_lossy();
    let resultado = Chamada::nova(soffice)
        .argumentos([
            perfil.as_os_str(),
            "--headless".as_ref(),
            "--norestore".as_ref(),
            "--nolockcheck".as_ref(),
            "--nodefault".as_ref(),
            "--convert-to".as_ref(),
            "pdf".as_ref(),
            "--outdir".as_ref(),
            saida.as_os_str(),
            entrada.as_os_str(),
        ])
        // O HOME (cache de fontes, etc.) e os temporários do LibreOffice também ficam na
        // pasta em memória: nada em `~/.config` nem em `/tmp`.
        .com_variavel("HOME", Zeroizing::new(pasta_texto.to_string()))
        .com_variavel("TMPDIR", Zeroizing::new(pasta_texto.to_string()))
        .com_prazo(PRAZO)
        .com_saida_maxima(64 * 1024)
        .rodar();
    let resultado = match resultado {
        Ok(r) => r,
        Err(falha) => {
            // Passou do prazo: o `oosplash` foi morto, mas o `soffice.bin` filho dele
            // ficaria órfão, segurando a pasta.
            matar_restos(pasta);
            return Err(ErroDaPrevia::de_processo("soffice", falha));
        }
    };
    let pdf = saida.join("entrada.pdf");
    if !pdf.exists() {
        let erro = String::from_utf8_lossy(&resultado.stderr);
        let ultima = erro.lines().last().unwrap_or("sem saída").trim().to_owned();
        return Err(ErroDaPrevia::Ilegivel(format!(
            "o LibreOffice não converteu: {ultima}"
        )));
    }
    let bytes = crate::ler(&pdf)?;
    drop(guarda);
    crate::pdf::abrir(bytes, &pdf, &[])
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_faxina_apaga_so_as_pastas_de_processos_mortos() {
        let raiz = tempfile::tempdir().expect("pasta");
        let eu = std::process::id();
        // Um pid que não existe: maior que o pid_max do Linux (4194304).
        let morto = 4_194_305_u32;
        for nome in [
            format!("{PREFIXO}{eu}-0"),
            format!("{PREFIXO}{morto}-3"),
            "outra-coisa".to_owned(),
        ] {
            std::fs::create_dir_all(raiz.path().join(&nome).join("perfil")).expect("criar");
        }
        assert_eq!(faxinar_em(raiz.path()), 1);
        let mut sobrou: Vec<String> = std::fs::read_dir(raiz.path())
            .expect("ler")
            .map(|e| e.expect("e").file_name().to_string_lossy().into_owned())
            .collect();
        sobrou.sort();
        assert_eq!(
            sobrou,
            [format!("{PREFIXO}{eu}-0"), "outra-coisa".to_owned()]
        );
    }
}
