//! Chamada ao `rclone`, sempre por `Command` com lista de argumentos (sem shell) e com
//! segredos só no ambiente do processo filho ou na entrada padrão, nunca em argumento.

use crate::erro::{Erro, Resultado};
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use zeroize::Zeroizing;

/// Variável de ambiente para o filho: nome e valor (zerado ao descartar).
pub type VariavelDeAmbiente = (String, Zeroizing<String>);

/// O rclone instalado.
#[derive(Debug, Clone)]
pub struct Rclone {
    binario: PathBuf,
}

impl Default for Rclone {
    fn default() -> Self {
        Self::novo()
    }
}

impl Rclone {
    /// Usa o `rclone` do PATH.
    #[must_use]
    pub fn novo() -> Self {
        Self {
            binario: PathBuf::from("rclone"),
        }
    }

    /// Usa outro executável (testes, ou instalação fora do PATH).
    #[must_use]
    pub fn com_binario(binario: impl Into<PathBuf>) -> Self {
        Self {
            binario: binario.into(),
        }
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
        let mut processo = Command::new(&self.binario);
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
        // stderr em outra thread, para nenhum dos dois pipes encher e travar o filho.
        let mut stderr = filho.stderr.take();
        let leitor = std::thread::spawn(move || {
            let mut texto = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut texto);
            }
            texto
        });
        let mut saida = String::new();
        if let Some(mut stdout) = filho.stdout.take() {
            let _ = stdout.read_to_string(&mut saida);
        }
        let estado = filho.wait()?;
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
