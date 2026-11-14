//! Conectar e desconectar Google Drive e OneDrive pelo app, num arquivo de configuração do
//! rclone **próprio do SBF** (nunca o do usuário).
//!
//! # Como o token fica fora do app
//!
//! `rclone config create <nome> drive|onedrive` abre o navegador, o usuário autoriza no site
//! do provedor e o rclone (com o client OAuth dele) grava o token **direto no arquivo de
//! configuração**. O app só passa o nome e o tipo: nenhum token, código ou segredo passa por
//! argumento, entrada, saída ou log do app.
//!
//! - O arquivo e a pasta dele são criados com permissão `0600`/`0700` (Unix) antes do rclone
//!   escrever, e a permissão é reaplicada depois de cada operação.
//! - Sem as perguntas interativas: na v1.60 o `config create` sem `--non-interactive` toma o
//!   valor padrão de cada pergunta (usar o navegador deste computador, `scope=drive` aqui,
//!   tipo de conta pessoal no OneDrive).
//! - **Cifra do arquivo.** A v1.60 não tem comando não interativo para cifrar a configuração
//!   (`rclone config encryption set` só existe a partir da 1.64). Por isso a proteção é a
//!   permissão `0600`. Se o usuário cifrar o arquivo por conta própria, a senha vem do
//!   chaveiro por [`Rclone::com_senha_da_configuracao`], que a põe só em `RCLONE_CONFIG_PASS`.
//! - Desconectar apaga a conexão do arquivo (e o token com ela). Não revoga o acesso no
//!   provedor; quem quiser revoga na conta Google ou Microsoft.

use crate::erro::{Erro, Resultado};
use crate::rclone::Rclone;
use std::path::Path;
use std::time::Duration;

/// Quanto tempo o usuário tem para autorizar no navegador.
pub const PRAZO_DA_AUTORIZACAO: Duration = Duration::from_secs(10 * 60);

/// Variáveis que o rclone precisa para abrir o navegador do usuário.
const VARIAVEIS_DO_NAVEGADOR: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "XDG_CURRENT_DESKTOP",
    "DBUS_SESSION_BUS_ADDRESS",
    "BROWSER",
];

/// Que nuvem conectar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nuvem {
    /// Google Drive (remoto rclone `drive`).
    Drive,
    /// Microsoft OneDrive (remoto rclone `onedrive`).
    OneDrive,
}

impl Nuvem {
    /// O tipo do backend no rclone.
    #[must_use]
    pub const fn tipo_do_rclone(self) -> &'static str {
        match self {
            Self::Drive => "drive",
            Self::OneDrive => "onedrive",
        }
    }

    /// Nome sugerido da conexão.
    #[must_use]
    pub const fn nome_sugerido(self) -> &'static str {
        match self {
            Self::Drive => "gdrive",
            Self::OneDrive => "onedrive",
        }
    }

    /// Opções de `key=value` do `config create`. Só escolhas, nunca segredo.
    #[must_use]
    pub const fn opcoes(self) -> &'static [&'static str] {
        match self {
            // Acesso completo: a Estante cria `SBF/Estante` e a coleta lê o resto.
            Self::Drive => &["scope=drive"],
            Self::OneDrive => &["drive_type=personal"],
        }
    }

    /// A nuvem de um tipo do rclone, se for uma das suportadas.
    #[must_use]
    pub fn do_tipo(tipo: &str) -> Option<Self> {
        match tipo {
            "drive" => Some(Self::Drive),
            "onedrive" => Some(Self::OneDrive),
            _ => None,
        }
    }
}

/// Uma conexão que existe no arquivo do SBF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conexao {
    /// Nome do remoto.
    pub nome: String,
    /// Que nuvem é.
    pub nuvem: Nuvem,
}

/// Lê `nome: tipo` (uma conexão por linha) e fica com Drive e OneDrive. Puro.
#[must_use]
pub fn conexoes_da_listagem(saida: &str) -> Vec<Conexao> {
    saida
        .lines()
        .filter_map(|linha| {
            let (nome, tipo) = linha.split_once(':')?;
            Some(Conexao {
                nome: nome.trim().to_owned(),
                nuvem: Nuvem::do_tipo(tipo.trim())?,
            })
        })
        .collect()
}

/// Aceita só letras, números, `_` e `-` (até 32), sem começar por `-`.
///
/// # Errors
/// [`Erro::NomeDeConexaoInvalido`].
pub fn validar_nome(nome: &str) -> Resultado<()> {
    let bom = !nome.is_empty()
        && nome.len() <= 32
        && !nome.starts_with('-')
        && nome
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bom {
        Ok(())
    } else {
        Err(Erro::NomeDeConexaoInvalido(nome.to_owned()))
    }
}

/// Cria o arquivo (e a pasta) com permissão restrita, se ainda não existirem, e reaplica a
/// permissão se já existirem.
///
/// # Errors
/// Falha de disco.
pub fn preparar_arquivo(arquivo: &Path) -> Resultado<()> {
    if let Some(pasta) = arquivo.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(pasta)?;
        restringir(pasta, 0o700)?;
    }
    if !arquivo.exists() {
        std::fs::write(arquivo, b"")?;
    }
    restringir(arquivo, 0o600)
}

#[cfg(unix)]
fn restringir(caminho: &Path, modo: u32) -> Resultado<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(modo))?;
    Ok(())
}

#[cfg(not(unix))]
fn restringir(_caminho: &Path, _modo: u32) -> Resultado<()> {
    Ok(())
}

/// As conexões de nuvem do SBF, no arquivo de configuração próprio.
#[derive(Debug, Clone)]
pub struct Conexoes {
    rclone: Rclone,
}

impl Conexoes {
    /// `rclone` precisa ter [`Rclone::com_configuracao`]: jamais se conecta no arquivo do
    /// usuário. Prepara o arquivo (0600).
    ///
    /// # Errors
    /// [`Erro::SemConfiguracaoPropria`] ou falha de disco.
    pub fn nova(rclone: Rclone) -> Resultado<Self> {
        let arquivo = rclone.configuracao().ok_or(Erro::SemConfiguracaoPropria)?;
        preparar_arquivo(arquivo)?;
        Ok(Self { rclone })
    }

    /// O rclone com o arquivo do SBF, para a Estante, a coleta e a cópia usarem as conexões.
    #[must_use]
    pub fn rclone(&self) -> &Rclone {
        &self.rclone
    }

    /// As conexões Drive e OneDrive do arquivo. Só lê a configuração; não abre nenhum remoto.
    ///
    /// # Errors
    /// Falha ao rodar o rclone.
    pub fn listar(&self) -> Resultado<Vec<Conexao>> {
        let saida = self.rclone.executar(["listremotes", "--long"], &[], None)?;
        Ok(conexoes_da_listagem(&saida))
    }

    /// "Conectar Google Drive" / "Conectar OneDrive": o rclone abre o navegador, o usuário
    /// autoriza e o token vai direto ao arquivo do SBF. Bloqueia até o fim (até
    /// [`PRAZO_DA_AUTORIZACAO`]); chame de uma thread.
    ///
    /// # Errors
    /// Nome inválido ou repetido, falha do rclone ou autorização não concluída.
    pub fn conectar(&self, nome: &str, nuvem: Nuvem) -> Resultado<()> {
        self.criar(nome, nuvem.tipo_do_rclone(), nuvem.opcoes(), true)
    }

    /// Cria um remoto de qualquer tipo sem navegador (`opcoes` em `chave=valor`). Serve a
    /// testes com remotos `local`/`alias`/`crypt`; as nuvens passam por [`Conexoes::conectar`].
    ///
    /// # Errors
    /// Como [`Conexoes::conectar`].
    pub fn criar_remoto(&self, nome: &str, tipo: &str, opcoes: &[&str]) -> Resultado<()> {
        self.criar(nome, tipo, opcoes, false)
    }

    fn criar(&self, nome: &str, tipo: &str, opcoes: &[&str], com_navegador: bool) -> Resultado<()> {
        validar_nome(nome)?;
        let existentes = self.rclone.executar(["listremotes"], &[], None)?;
        if existentes
            .lines()
            .any(|l| l.trim().trim_end_matches(':') == nome)
        {
            return Err(Erro::ConexaoRepetida(nome.to_owned()));
        }
        let mut argumentos = vec!["config".to_owned(), "create".to_owned()];
        argumentos.push(nome.to_owned());
        argumentos.push(tipo.to_owned());
        argumentos.extend(opcoes.iter().map(|o| (*o).to_owned()));
        let herdar: &[&str] = if com_navegador {
            VARIAVEIS_DO_NAVEGADOR
        } else {
            &[]
        };
        let resultado =
            self.rclone
                .executar_com(&argumentos, &[], None, herdar, Some(PRAZO_DA_AUTORIZACAO));
        if let Some(arquivo) = self.rclone.configuracao() {
            restringir(arquivo, 0o600)?;
        }
        resultado.map(|_| ())
    }

    /// Desconecta: tira a conexão (e o token) do arquivo do SBF. Não toca na nuvem.
    ///
    /// # Errors
    /// [`Erro::ConexaoInexistente`] ou falha do rclone.
    pub fn desconectar(&self, nome: &str) -> Resultado<()> {
        validar_nome(nome)?;
        let existentes = self.rclone.executar(["listremotes"], &[], None)?;
        if !existentes
            .lines()
            .any(|l| l.trim().trim_end_matches(':') == nome)
        {
            return Err(Erro::ConexaoInexistente(nome.to_owned()));
        }
        self.rclone
            .executar(["config", "delete", nome], &[], None)?;
        if let Some(arquivo) = self.rclone.configuracao() {
            restringir(arquivo, 0o600)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nomes_validos_e_invalidos() {
        for ok in ["gdrive", "one-drive", "A_1"] {
            assert!(validar_nome(ok).is_ok(), "{ok}");
        }
        for ruim in ["", "-x", "a:b", "a b", "a/b", "ç", &"x".repeat(33)] {
            assert!(validar_nome(ruim).is_err(), "{ruim}");
        }
    }

    #[test]
    fn listagem_fica_com_drive_e_onedrive() {
        let c = conexoes_da_listagem("a: drive\nb: local\nc: onedrive\nlixo\n");
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].nuvem, Nuvem::OneDrive);
    }

    #[test]
    fn opcoes_nao_tem_segredo() {
        for n in [Nuvem::Drive, Nuvem::OneDrive] {
            assert!(n.opcoes().iter().all(|o| !o.contains("token")));
        }
    }
}
