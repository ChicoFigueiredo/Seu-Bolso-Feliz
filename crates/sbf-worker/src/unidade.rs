//! O serviço do usuário. No Linux, uma unit systemd `--user` com reinício; no Windows e no
//! macOS o comando responde **"ainda não suportado"** (launchd e serviço do Windows são
//! trabalho futuro, ADR-014): lá, `sbf worker rodar` à mão funciona igual.
//!
//! Gerar a unit é função pura e testada em arquivo temporário. Ligar de verdade
//! (`systemctl --user enable --now`) só acontece quando o usuário roda
//! `sbf worker instalar`; nenhum teste chama o `systemctl` real: o [`Systemctl`] é injetado.

use std::path::{Path, PathBuf};

use crate::{Erro, Resultado};

/// O nome da unit.
pub const NOME: &str = "sbf-worker.service";

/// Quem executa `systemctl --user ...`.
pub trait Systemctl {
    /// Roda `systemctl --user <args>` e devolve a saída padrão, ou o motivo da falha (sem
    /// caminho nem conteúdo).
    ///
    /// # Errors
    /// Texto curto da falha.
    fn rodar(&self, args: &[&str]) -> Result<String, String>;
}

/// O `systemctl` do sistema, por caminho absoluto de confiança (`sbf-processo`).
#[derive(Debug, Default)]
pub struct SystemctlDoSistema;

impl Systemctl for SystemctlDoSistema {
    fn rodar(&self, args: &[&str]) -> Result<String, String> {
        use sbf_processo::{Chamada, localizar};
        let Some(programa) = localizar("systemctl") else {
            return Err("systemctl não encontrado".to_owned());
        };
        let mut todos = vec!["--user"];
        todos.extend_from_slice(args);
        let saida = Chamada::nova(programa)
            .argumentos(todos)
            .herdando(&["HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"])
            .rodar()
            .map_err(|e| e.to_string())?;
        let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
        if saida.status.success() {
            Ok(texto)
        } else {
            // `is-active` e `is-enabled` saem com código != 0 para "inativo": quem chama decide.
            Err(texto.trim().to_owned())
        }
    }
}

fn texto_seguro(c: &Path) -> Result<&str, Erro> {
    let s = c.to_str().ok_or(Erro::CaminhoInvalido)?;
    if !c.is_absolute() || s.contains(['\n', '\r', '\0']) {
        return Err(Erro::CaminhoInvalido);
    }
    Ok(s)
}

/// Um argumento no formato da unit: entre aspas, com `\`, `"` e `%` escapados.
fn quotar(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '%' => o.push_str("%%"),
            '$' => o.push_str("$$"),
            _ => o.push(c),
        }
    }
    o.push('"');
    o
}

/// O texto da unit para o binário `exe` e o cofre `cofre`.
///
/// # Errors
/// [`Erro::CaminhoInvalido`] se algum caminho não é absoluto ou tem quebra de linha.
pub fn gerar(exe: &Path, cofre: &Path) -> Resultado<String> {
    let exe = texto_seguro(exe)?;
    let cofre = texto_seguro(cofre)?;
    Ok(format!(
        "[Unit]\n\
Description=Seu Bolso Feliz: worker local (coleta, vigia de pastas e triagem)\n\
After=default.target\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart={} worker rodar --inventario {}\n\
Restart=on-failure\n\
RestartSec=15\n\
Nice=10\n\
NoNewPrivileges=true\n\
\n\
[Install]\n\
WantedBy=default.target\n",
        quotar(exe),
        quotar(cofre),
    ))
}

/// A pasta das units do usuário: `<config>/systemd/user`.
#[must_use]
pub fn pasta_das_units(config_home: &Path) -> PathBuf {
    config_home.join("systemd").join("user")
}

/// O `~/.config`, ou `XDG_CONFIG_HOME`.
#[must_use]
pub fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|h| h.join(".config"))
        })
}

fn so_linux() -> Result<(), Erro> {
    if cfg!(target_os = "linux") {
        Ok(())
    } else {
        Err(Erro::NaoSuportado)
    }
}

/// Escreve a unit em `config_home` e, com `ligar`, recarrega e liga (`enable --now`).
///
/// # Errors
/// [`Erro::NaoSuportado`] fora do Linux; caminho inválido; falha de disco ou do `systemctl`.
pub fn instalar(
    exe: &Path,
    cofre: &Path,
    config_home: &Path,
    ligar: bool,
    systemctl: &dyn Systemctl,
) -> Resultado<PathBuf> {
    so_linux()?;
    let texto = gerar(exe, cofre)?;
    let pasta = pasta_das_units(config_home);
    std::fs::create_dir_all(&pasta)?;
    let arquivo = pasta.join(NOME);
    std::fs::write(&arquivo, texto)?;
    if ligar {
        systemctl
            .rodar(&["daemon-reload"])
            .map_err(Erro::Systemctl)?;
        systemctl
            .rodar(&["enable", "--now", NOME])
            .map_err(Erro::Systemctl)?;
    }
    Ok(arquivo)
}

/// Desliga e apaga a unit.
///
/// # Errors
/// [`Erro::NaoSuportado`] fora do Linux; falha de disco.
pub fn desinstalar(
    config_home: &Path,
    desligar: bool,
    systemctl: &dyn Systemctl,
) -> Resultado<bool> {
    so_linux()?;
    let arquivo = pasta_das_units(config_home).join(NOME);
    if desligar {
        // Já parada ou nunca ligada não é erro.
        let _ = systemctl.rodar(&["disable", "--now", NOME]);
    }
    let existia = arquivo.exists();
    if existia {
        std::fs::remove_file(&arquivo)?;
    }
    if desligar {
        let _ = systemctl.rodar(&["daemon-reload"]);
    }
    Ok(existia)
}

/// O que o `estado` mostra do serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDoServico {
    /// A unit existe na pasta do usuário.
    pub instalada: bool,
    /// `enabled`, `disabled` ou o que o systemd respondeu.
    pub habilitada: String,
    /// `active`, `inactive`, `failed`...
    pub ativa: String,
}

/// Consulta a unit.
///
/// # Errors
/// [`Erro::NaoSuportado`] fora do Linux.
pub fn estado(config_home: &Path, systemctl: &dyn Systemctl) -> Resultado<EstadoDoServico> {
    so_linux()?;
    let instalada = pasta_das_units(config_home).join(NOME).exists();
    let resposta = |args: &[&str]| match systemctl.rodar(args) {
        Ok(t) | Err(t) => {
            let t = t.trim();
            if t.is_empty() {
                "desconhecido".to_owned()
            } else {
                t.lines().next().unwrap_or("").to_owned()
            }
        }
    };
    Ok(EstadoDoServico {
        instalada,
        habilitada: resposta(&["is-enabled", NOME]),
        ativa: resposta(&["is-active", NOME]),
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Falso {
        chamadas: RefCell<Vec<String>>,
    }
    impl Systemctl for Falso {
        fn rodar(&self, args: &[&str]) -> Result<String, String> {
            self.chamadas.borrow_mut().push(args.join(" "));
            Ok("enabled\n".to_owned())
        }
    }

    #[test]
    fn a_unit_tem_reinicio_e_exec_com_aspas() {
        let u = gerar(Path::new("/opt/Meu App/sbf"), Path::new("/dados/cofre 1")).unwrap();
        assert!(
            u.contains(
                "ExecStart=\"/opt/Meu App/sbf\" worker rodar --inventario \"/dados/cofre 1\"\n"
            ),
            "{u}"
        );
        assert!(u.contains("Restart=on-failure"));
        assert!(u.contains("WantedBy=default.target"));
        assert!(u.contains("NoNewPrivileges=true"));
    }

    #[test]
    fn escapa_percentual_aspas_e_recusa_caminho_ruim() {
        let u = gerar(Path::new("/o/s%bf\"x"), Path::new("/c")).unwrap();
        assert!(u.contains("\"/o/s%%bf\\\"x\""), "{u}");
        assert!(gerar(Path::new("relativo/sbf"), Path::new("/c")).is_err());
        assert!(gerar(Path::new("/o/sbf"), Path::new("/c\nExecStart=/bin/sh")).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn instalar_so_gera_o_arquivo_sem_ligar_e_ligar_chama_o_systemctl_injetado() {
        let d = tempfile::tempdir().unwrap();
        let f = Falso::default();
        let arq = instalar(Path::new("/o/sbf"), Path::new("/c"), d.path(), false, &f).unwrap();
        assert_eq!(arq, d.path().join("systemd/user/sbf-worker.service"));
        assert!(
            std::fs::read_to_string(&arq)
                .unwrap()
                .contains("worker rodar")
        );
        assert!(
            f.chamadas.borrow().is_empty(),
            "sem ligar, nenhum systemctl"
        );
        instalar(Path::new("/o/sbf"), Path::new("/c"), d.path(), true, &f).unwrap();
        assert_eq!(
            f.chamadas.borrow().as_slice(),
            ["daemon-reload", "enable --now sbf-worker.service"]
        );
        let e = estado(d.path(), &f).unwrap();
        assert!(e.instalada);
        assert_eq!(e.habilitada, "enabled");
        assert!(desinstalar(d.path(), true, &f).unwrap());
        assert!(!arq.exists());
        assert!(!desinstalar(d.path(), false, &f).unwrap());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn fora_do_linux_diz_ainda_nao_suportado() {
        let d = tempfile::tempdir().unwrap();
        let e = instalar(
            Path::new("/o/sbf"),
            Path::new("/c"),
            d.path(),
            false,
            &Falso::default(),
        );
        assert!(matches!(e, Err(Erro::NaoSuportado)));
    }
}
