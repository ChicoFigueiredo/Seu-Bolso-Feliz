//! Chamada ao `rclone`, sempre com lista de argumentos (sem shell) e com segredos só no
//! ambiente do processo filho ou na entrada padrão, nunca em argumento.
//!
//! # Qual rclone e com que ambiente (achados A6 e A7 da revisão de segurança da fatia 1)
//!
//! A regra é a do `sbf-processo` (uma implementação por regra, ADR-012):
//!
//! - [`Rclone::novo`] resolve o caminho **uma vez**, nas pastas do sistema
//!   ([`sbf_processo::localizar`]), nunca pelo `PATH`: um `rclone` plantado em `~/.bun/bin` ou
//!   num shim receberia a senha ofuscada da cópia. [`Rclone::com_binario`] é a configuração
//!   explícita. Um caminho relativo, ausente ou em pasta gravável por outros não roda
//!   ([`Erro::RcloneAusente`]). [`Rclone::binario`] é o caminho a mostrar em Opções.
//! - O filho roda com o ambiente mínimo: o `PATH` do sistema, o idioma, o que o rclone precisa
//!   para achar a configuração do usuário (`HOME`, `XDG_CONFIG_HOME`, `RCLONE_CONFIG`,
//!   `RCLONE_CONFIG_PASS`), o cache, a pasta temporária, os proxies e os certificados
//!   ([`VARIAVEIS_DO_RCLONE`]), e as variáveis que o próprio app põe. Nenhum outro `RCLONE_*`
//!   do shell (por exemplo `RCLONE_DRY_RUN` ou um `PASSWORD2` alheio) muda a cópia.
//! - A saída guardada tem teto ([`SAIDA_MAXIMA_DO_RCLONE`]), e o fim do stderr também.

use crate::erro::{Erro, Resultado};
use sbf_processo::{Chamada, FalhaDeProcesso};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeroize::Zeroizing;

/// Variável de ambiente para o filho: nome e valor (zerado ao descartar).
pub type VariavelDeAmbiente = (String, Zeroizing<String>);

/// O que passa do app para o rclone, se existir, além de [`sbf_processo::VARIAVEIS_BASICAS`].
pub const VARIAVEIS_DO_RCLONE: &[&str] = &[
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

/// O caminho absoluto do `rclone` na primeira pasta do sistema em que ele existe e é
/// confiável ([`sbf_processo::localizar`]). Nunca olha o `PATH`.
#[must_use]
pub fn localizar_rclone() -> Option<PathBuf> {
    sbf_processo::localizar("rclone")
}

/// Prazo padrão das chamadas curtas (listar, ler o sal, ofuscar, renomear, apagar): um remoto
/// travado não trava a ação (achado A9 da revisão de segurança da fatia 1). As transferências
/// (`copy`, `copyto`) não têm prazo de relógio, porque o acervo pode levar horas; nelas vale
/// o `--timeout` do próprio rclone (5 min sem tráfego).
pub const PRAZO_PADRAO: Duration = Duration::from_secs(5 * 60);
/// Capacidade inicial do buffer da saída: cabe a senha ofuscada sem realocar.
const CAPACIDADE_DA_SAIDA: usize = 4096;
/// O máximo guardado da saída do rclone: a listagem do acervo inteiro (`lsf`, `lsjson`) cabe
/// com folga; passar disto é erro, nunca uma listagem cortada em silêncio.
pub const SAIDA_MAXIMA_DO_RCLONE: usize = 256 * 1024 * 1024;
/// Subcomandos que transferem dados e ficam sem prazo de relógio.
const TRANSFERENCIAS: &[&str] = &["copy", "copyto"];

/// O rclone instalado.
#[derive(Debug, Clone)]
pub struct Rclone {
    binario: PathBuf,
    prazo: Duration,
    /// Arquivo de configuração próprio do SBF (`RCLONE_CONFIG`). `None`: o rclone decide.
    configuracao: Option<PathBuf>,
    /// Senha de um arquivo de configuração cifrado (`RCLONE_CONFIG_PASS`), só no ambiente.
    senha_da_configuracao: Option<std::sync::Arc<Zeroizing<String>>>,
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
            configuracao: None,
            senha_da_configuracao: None,
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
            configuracao: None,
            senha_da_configuracao: None,
        }
    }

    /// Usa o arquivo de configuração próprio do SBF em vez do do usuário (`RCLONE_CONFIG`).
    #[must_use]
    pub fn com_configuracao(mut self, arquivo: impl Into<PathBuf>) -> Self {
        self.configuracao = Some(arquivo.into());
        self
    }

    /// O arquivo de configuração em uso, se foi definido.
    #[must_use]
    pub fn configuracao(&self) -> Option<&Path> {
        self.configuracao.as_deref()
    }

    /// Senha do arquivo de configuração, se ele for cifrado (vem do chaveiro). Vai só no
    /// ambiente do filho (`RCLONE_CONFIG_PASS`), nunca em argumento.
    #[must_use]
    pub fn com_senha_da_configuracao(mut self, senha: &bancada_segredos::Segredo) -> Self {
        self.senha_da_configuracao = Some(std::sync::Arc::new(Zeroizing::new(
            senha.expor().to_owned(),
        )));
        self
    }

    /// Troca o prazo das chamadas curtas ([`PRAZO_PADRAO`]).
    #[must_use]
    pub fn com_prazo(mut self, prazo: Duration) -> Self {
        self.prazo = prazo;
        self
    }

    /// Roda o rclone e devolve o stdout. `entrada` vai na entrada padrão; `ambiente` só no
    /// ambiente do filho.
    pub fn executar<I, S>(
        &self,
        argumentos: I,
        ambiente: &[VariavelDeAmbiente],
        entrada: Option<&str>,
    ) -> Resultado<Zeroizing<String>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.executar_com(argumentos, ambiente, entrada, &[], None)
    }

    /// Como [`Rclone::executar`], com variáveis a mais herdadas do ambiente (por exemplo
    /// `DISPLAY`, para o rclone abrir o navegador) e um prazo próprio.
    pub fn executar_com<I, S>(
        &self,
        argumentos: I,
        ambiente: &[VariavelDeAmbiente],
        entrada: Option<&str>,
        herdar_alem: &[&str],
        prazo: Option<Duration>,
    ) -> Resultado<Zeroizing<String>>
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
        let mut chamada = Chamada::nova(&self.binario)
            .argumentos(&argumentos)
            .herdando(VARIAVEIS_DO_RCLONE)
            .herdando(herdar_alem)
            .com_saida_maxima(SAIDA_MAXIMA_DO_RCLONE)
            .com_capacidade_da_saida(CAPACIDADE_DA_SAIDA);
        // As transferências não têm prazo de relógio (vale o `--timeout` do rclone).
        chamada = if TRANSFERENCIAS.contains(&comando.as_str()) {
            chamada.sem_prazo()
        } else {
            chamada.com_prazo(prazo.unwrap_or(self.prazo))
        };
        if let Some(arquivo) = &self.configuracao {
            chamada = chamada.com_variavel(
                "RCLONE_CONFIG",
                Zeroizing::new(arquivo.to_string_lossy().into_owned()),
            );
        }
        if let Some(senha) = &self.senha_da_configuracao {
            chamada =
                chamada.com_variavel("RCLONE_CONFIG_PASS", Zeroizing::new((***senha).clone()));
        }
        for (nome, valor) in ambiente {
            chamada = chamada.com_variavel(nome.clone(), valor.clone());
        }
        if let Some(texto) = entrada {
            chamada = chamada.com_entrada(Zeroizing::new(texto.as_bytes().to_vec()));
        }
        let saida = chamada.rodar().map_err(|falha| match falha {
            FalhaDeProcesso::TempoEsgotado => Erro::TempoEsgotado {
                comando: comando.clone(),
            },
            FalhaDeProcesso::Io(tipo) => Erro::Io(std::io::Error::from(tipo)),
            _ => Erro::RcloneAusente,
        })?;
        if saida.stdout_cortado {
            return Err(Erro::Rclone {
                comando,
                codigo: saida.status.code(),
                mensagem: format!(
                    "a saída passou de {} MiB",
                    SAIDA_MAXIMA_DO_RCLONE / (1024 * 1024)
                ),
            });
        }
        if saida.status.success() {
            return Ok(texto_zeravel(saida.stdout));
        }
        Err(Erro::Rclone {
            comando,
            codigo: saida.status.code(),
            mensagem: fim_do_stderr(&String::from_utf8_lossy(&saida.stderr)),
        })
    }

    /// `rclone obscure -`: a senha vai pela entrada padrão e volta no formato que o rclone
    /// aceita em `RCLONE_CONFIG_<NOME>_PASSWORD`. Não é cifra, só ofuscação reversível.
    pub(crate) fn ofuscar(&self, senha: &str) -> Resultado<Zeroizing<String>> {
        let saida = self.executar(["obscure", "-"], &[], Some(senha))?;
        Ok(Zeroizing::new(saida.trim().to_owned()))
    }
}

/// Os bytes da saída como texto, sem cópia quando são UTF-8 (a senha ofuscada não fica
/// espalhada pela memória).
fn texto_zeravel(mut bytes: Zeroizing<Vec<u8>>) -> Zeroizing<String> {
    match String::from_utf8(std::mem::take(&mut *bytes)) {
        Ok(texto) => Zeroizing::new(texto),
        Err(erro) => {
            let bytes = Zeroizing::new(erro.into_bytes());
            Zeroizing::new(String::from_utf8_lossy(&bytes).into_owned())
        }
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
