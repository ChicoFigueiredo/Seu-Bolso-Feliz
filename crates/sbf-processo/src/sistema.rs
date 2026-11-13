//! Abrir um arquivo (ou a pasta dele) com o programa padrão do sistema: `xdg-open` no Linux,
//! `open` no macOS, `explorer` no Windows, sob a mesma regra das outras chamadas (caminho
//! absoluto nas pastas do sistema, nada gravável por outros, ambiente mínimo).

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;

use crate::FalhaDeProcesso;
use crate::chamada::comando;
use crate::programa::localizar_ou_nome;

/// O que o abridor herda do app, além das básicas: só o necessário para achar a sessão
/// gráfica e o programa padrão do usuário. Nenhuma é segredo.
#[cfg(not(windows))]
const VARIAVEIS_DA_SESSAO: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "XDG_SESSION_DESKTOP",
    "XDG_DATA_HOME",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_HOME",
    "XDG_CONFIG_DIRS",
    "DESKTOP_SESSION",
    "KDE_FULL_SESSION",
];
#[cfg(windows)]
const VARIAVEIS_DA_SESSAO: &[&str] = &[
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "HOMEDRIVE",
    "HOMEPATH",
    "USERNAME",
];

/// O nome do abridor do sistema.
fn abridor() -> &'static str {
    if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Os argumentos do abridor: o arquivo, ou a pasta dele com o arquivo selecionado quando o
/// sistema sabe selecionar (`open -R`, `explorer /select,`); o `xdg-open` abre a pasta.
fn argumentos(caminho: &Path, so_a_pasta: bool) -> Vec<OsString> {
    if !so_a_pasta {
        return vec![caminho.as_os_str().to_owned()];
    }
    if cfg!(windows) {
        let mut selecionar = OsString::from("/select,");
        selecionar.push(caminho.as_os_str());
        vec![selecionar]
    } else if cfg!(target_os = "macos") {
        vec![OsString::from("-R"), caminho.as_os_str().to_owned()]
    } else {
        let pasta = caminho.parent().unwrap_or(caminho);
        vec![pasta.as_os_str().to_owned()]
    }
}

/// Abre `caminho` com o programa padrão do sistema (ou a pasta dele, com `so_a_pasta`), sem
/// esperar: uma thread recolhe o abridor quando ele sai, para não deixar processo zumbi.
///
/// # Errors
/// [`FalhaDeProcesso::CaminhoRelativo`] se o caminho não é absoluto (um relativo começando
/// com `-` viraria opção do abridor); [`FalhaDeProcesso::NaoEncontrado`] se o abridor não está
/// numa pasta do sistema ou não é confiável.
pub fn abrir_no_sistema(caminho: &Path, so_a_pasta: bool) -> Result<(), FalhaDeProcesso> {
    abrir_com(&localizar_ou_nome(abridor()), caminho, so_a_pasta)
}

pub(crate) fn abrir_com(
    programa: &Path,
    caminho: &Path,
    so_a_pasta: bool,
) -> Result<(), FalhaDeProcesso> {
    if !caminho.is_absolute() {
        return Err(FalhaDeProcesso::CaminhoRelativo);
    }
    let mut c = comando(
        programa,
        argumentos(caminho, so_a_pasta),
        VARIAVEIS_DA_SESSAO,
    )?;
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut filho = c.spawn()?;
    std::thread::spawn(move || {
        let _ = filho.wait();
    });
    Ok(())
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod testes {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    fn esperar_arquivo(caminho: &Path) -> String {
        let fim = Instant::now() + Duration::from_secs(10);
        while Instant::now() < fim {
            if let Ok(t) = std::fs::read_to_string(caminho)
                && t.ends_with("FIM\n")
            {
                return t;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("o abridor falso não rodou");
    }

    #[test]
    fn abre_o_arquivo_ou_a_pasta_com_ambiente_minimo() {
        let pasta = tempfile::tempdir().expect("pasta");
        let registro = pasta.path().join("registro.txt");
        let falso = pasta.path().join("abridor");
        std::fs::write(
            &falso,
            format!(
                "#!/bin/sh\n{{ printf 'ARG:%s\\n' \"$@\"; /usr/bin/env; echo FIM; }} > '{}'\n",
                registro.display()
            ),
        )
        .expect("script");
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let doc = pasta.path().join("sub").join("fatura.pdf");

        abrir_com(&falso, &doc, false).expect("abrir");
        let t = esperar_arquivo(&registro);
        assert!(t.contains(&format!("ARG:{}\n", doc.display())), "{t}");
        assert!(t.contains("PATH=/usr/bin:/bin\n"), "{t}");
        for linha in t.lines().filter(|l| !l.starts_with("ARG:") && *l != "FIM") {
            let nome = linha.split_once('=').map_or(linha, |(n, _)| n);
            assert!(
                ["PATH", "PWD", "SHLVL", "_", "OLDPWD"].contains(&nome)
                    || VARIAVEIS_DA_SESSAO.contains(&nome)
                    || crate::VARIAVEIS_BASICAS.contains(&nome),
                "variável a mais: {nome}"
            );
        }

        std::fs::remove_file(&registro).expect("limpar");
        abrir_com(&falso, &doc, true).expect("abrir a pasta");
        let t = esperar_arquivo(&registro);
        assert!(
            t.contains(&format!("ARG:{}\n", doc.parent().expect("pai").display())),
            "{t}"
        );
    }

    #[test]
    fn caminho_relativo_e_abridor_inseguro_sao_recusados() {
        assert_eq!(
            abrir_no_sistema(Path::new("-x.pdf"), false),
            Err(FalhaDeProcesso::CaminhoRelativo)
        );
        assert_eq!(
            abrir_com(Path::new("xdg-open"), Path::new("/tmp/x.pdf"), false),
            Err(FalhaDeProcesso::NaoEncontrado)
        );
    }
}
