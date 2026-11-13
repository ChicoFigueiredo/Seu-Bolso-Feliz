//! Onde o chaveiro mora: a pasta de dados do app, **nunca** dentro do cofre (spec 06,
//! "Chaveiro"; o glossário manda o chaveiro não ficar junto dos documentos).

use crate::erro::ErroDoChaveiro;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// O sistema operacional, para escolher a pasta de dados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sistema {
    Linux,
    Windows,
    MacOs,
}

impl Sistema {
    /// O sistema em que o app está rodando (outro Unix conta como Linux: segue o XDG).
    pub fn atual() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// O nome da subpasta do chaveiro dentro da pasta de dados do app.
pub const SUBPASTA_DO_CHAVEIRO: &str = "chaveiro";

/// A pasta de dados do app, dado o sistema e um leitor de variáveis de ambiente:
///
/// - Linux: `$XDG_DATA_HOME/seu-bolso-feliz` (só se absoluto) ou
///   `$HOME/.local/share/seu-bolso-feliz`;
/// - Windows: `%APPDATA%\Seu Bolso Feliz`;
/// - macOS: `$HOME/Library/Application Support/Seu Bolso Feliz`.
pub fn pasta_de_dados_de(
    sistema: Sistema,
    variavel: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, ErroDoChaveiro> {
    let absoluta = |nome: &str| {
        variavel(nome)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    match sistema {
        Sistema::Linux => absoluta("XDG_DATA_HOME")
            .or_else(|| absoluta("HOME").map(|h| h.join(".local").join("share")))
            .map(|base| base.join("seu-bolso-feliz")),
        Sistema::Windows => absoluta("APPDATA").map(|base| base.join("Seu Bolso Feliz")),
        Sistema::MacOs => absoluta("HOME").map(|h| {
            h.join("Library")
                .join("Application Support")
                .join("Seu Bolso Feliz")
        }),
    }
    .ok_or(ErroDoChaveiro::SemPastaDeDados)
}

/// A pasta de dados do app neste sistema.
pub fn pasta_de_dados_do_app() -> Result<PathBuf, ErroDoChaveiro> {
    pasta_de_dados_de(Sistema::atual(), |nome| std::env::var_os(nome))
}

/// A pasta padrão do chaveiro: `<pasta de dados do app>/chaveiro`.
pub fn pasta_padrao_do_chaveiro() -> Result<PathBuf, ErroDoChaveiro> {
    Ok(pasta_de_dados_do_app()?.join(SUBPASTA_DO_CHAVEIRO))
}

/// O caminho absoluto e sem `..`, resolvendo links no maior prefixo que já existe (a pasta do
/// chaveiro pode ainda não existir).
fn normalizar(caminho: &Path) -> PathBuf {
    let absoluto = if caminho.is_absolute() {
        caminho.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|d| d.join(caminho))
            .unwrap_or_else(|_| caminho.to_path_buf())
    };
    let mut existente = absoluto.as_path();
    let mut resto = Vec::new();
    loop {
        if let Ok(real) = existente.canonicalize() {
            let mut saida = real;
            for parte in resto.iter().rev() {
                saida.push(parte);
            }
            return sem_ponto_ponto(&saida);
        }
        match (existente.parent(), existente.components().next_back()) {
            (Some(pai), Some(parte)) => {
                resto.push(parte.as_os_str().to_os_string());
                existente = pai;
            }
            _ => return sem_ponto_ponto(&absoluto),
        }
    }
}

/// Tira `.` e `..` lexicamente (o que sobrou depois do maior prefixo que existe).
fn sem_ponto_ponto(caminho: &Path) -> PathBuf {
    use std::path::Component;
    let mut saida = PathBuf::new();
    for parte in caminho.components() {
        match parte {
            Component::CurDir => {}
            Component::ParentDir => {
                saida.pop();
            }
            outra => saida.push(outra.as_os_str()),
        }
    }
    saida
}

/// Recusa uma pasta do chaveiro que fique dentro do cofre (ou que seja o próprio cofre).
pub fn garantir_fora_do_cofre(
    pasta_do_chaveiro: &Path,
    raiz_do_cofre: &Path,
) -> Result<(), ErroDoChaveiro> {
    if normalizar(pasta_do_chaveiro).starts_with(normalizar(raiz_do_cofre)) {
        Err(ErroDoChaveiro::PastaDentroDoCofre)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ambiente(pares: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pares: Vec<(String, String)> = pares
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect();
        move |nome| {
            pares
                .iter()
                .find(|(a, _)| a == nome)
                .map(|(_, b)| OsString::from(b))
        }
    }

    #[test]
    fn linux_segue_o_xdg_e_cai_para_o_home() {
        assert_eq!(
            pasta_de_dados_de(
                Sistema::Linux,
                ambiente(&[("XDG_DATA_HOME", "/dados"), ("HOME", "/h")])
            ),
            Ok(PathBuf::from("/dados/seu-bolso-feliz"))
        );
        assert_eq!(
            pasta_de_dados_de(
                Sistema::Linux,
                ambiente(&[("XDG_DATA_HOME", "relativo"), ("HOME", "/h")])
            ),
            Ok(PathBuf::from("/h/.local/share/seu-bolso-feliz"))
        );
        assert_eq!(
            pasta_de_dados_de(Sistema::Linux, ambiente(&[])),
            Err(ErroDoChaveiro::SemPastaDeDados)
        );
    }

    #[test]
    fn macos_usa_application_support() {
        assert_eq!(
            pasta_de_dados_de(Sistema::MacOs, ambiente(&[("HOME", "/Users/x")])),
            Ok(PathBuf::from(
                "/Users/x/Library/Application Support/Seu Bolso Feliz"
            ))
        );
    }

    #[cfg(unix)]
    #[test]
    fn recusa_pasta_dentro_do_cofre_mesmo_por_link_e_ponto_ponto() {
        let base = tempfile::tempdir().expect("pasta temporária");
        let cofre = base.path().join("cofre");
        std::fs::create_dir_all(cofre.join("segredos")).expect("cofre");
        let fora = base.path().join("dados").join("chaveiro");
        assert_eq!(garantir_fora_do_cofre(&fora, &cofre), Ok(()));
        assert_eq!(
            garantir_fora_do_cofre(&cofre.join("segredos").join("chaveiro"), &cofre),
            Err(ErroDoChaveiro::PastaDentroDoCofre)
        );
        assert_eq!(
            garantir_fora_do_cofre(&base.path().join("dados/../cofre/x"), &cofre),
            Err(ErroDoChaveiro::PastaDentroDoCofre)
        );
        let link = base.path().join("atalho");
        std::os::unix::fs::symlink(&cofre, &link).expect("link");
        assert_eq!(
            garantir_fora_do_cofre(&link.join("chaveiro"), &cofre),
            Err(ErroDoChaveiro::PastaDentroDoCofre)
        );
    }
}
