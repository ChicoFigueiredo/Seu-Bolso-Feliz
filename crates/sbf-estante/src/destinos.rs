//! Vários destinos para a estante (decisão do CEO, 2026-11-13): Google Drive, OneDrive e uma
//! pasta local, todos com a mesma organização `<ciclo>/<tipo>/<nome>`.
//!
//! # Decisões
//!
//! - **A pasta local também passa pelo rclone** (backend `local`, caminho absoluto sem nome de
//!   remoto). Assim há um só planejador incremental (rename em vez de reenvio, só nomes no
//!   nosso padrão, o resto fica em paz) e o resultado é idêntico em todos os destinos. O custo
//!   é exigir o rclone mesmo para a pasta local; ele já é requisito do app.
//! - **A lista vive nas preferências do app**, fora do cofre ([`ListaDeDestinos::carregar`] e
//!   [`ListaDeDestinos::salvar`] recebem o arquivo; quem escolhe a pasta é a janela). Não há
//!   segredo nela: só nome, tipo, alvo e se está ativo.
//! - **Atualizar percorre os ativos em sequência**; a falha de um (remoto inexistente, pasta
//!   sem permissão, alvo inválido) fica no relatório dele e não impede os outros.
//! - **Detectar remotos** usa `rclone listremotes --long`, que só lê o arquivo de configuração:
//!   não abre nem lista o conteúdo de nenhum remoto.

use crate::erro::{Erro, Resultado};
use crate::estante::{Andamento, Estante, Plano, RelatorioDaEstante};
use crate::rclone::{Rclone, validar_destino};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Pasta da estante dentro de um remoto em nuvem, quando se sugere um destino.
pub const PASTA_NA_NUVEM: &str = "SBF/Estante";

/// Que tipo de lugar é o destino.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoDeDestino {
    /// Remoto rclone do tipo `drive`.
    Drive,
    /// Remoto rclone do tipo `onedrive`.
    OneDrive,
    /// Pasta no disco, por caminho absoluto.
    PastaLocal,
}

/// Um lugar onde a estante é mantida.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destino {
    /// Nome mostrado em Opções e no relatório; único na lista.
    pub nome: String,
    /// Tipo do destino.
    pub tipo: TipoDeDestino,
    /// Remoto e pasta (`gdrive:SBF/Estante`) ou caminho absoluto da pasta local.
    pub alvo: String,
    /// Só os ativos entram em "Atualizar estante".
    pub ativo: bool,
}

impl Destino {
    /// Destino em nuvem. `alvo` é `<remoto>:<pasta>`.
    ///
    /// # Errors
    /// [`Erro::DestinoInvalido`] se o alvo for vazio, começar com `-` ou não tiver `:`.
    pub fn nuvem(
        nome: impl Into<String>,
        tipo: TipoDeDestino,
        alvo: impl Into<String>,
    ) -> Resultado<Self> {
        let d = Self {
            nome: nome.into(),
            tipo,
            alvo: alvo.into(),
            ativo: true,
        };
        d.validar()?;
        Ok(d)
    }

    /// Pasta local. `caminho` precisa ser absoluto (assim o rclone nunca o lê como remoto).
    ///
    /// # Errors
    /// [`Erro::DestinoInvalido`] se o caminho for vazio ou relativo.
    pub fn pasta_local(nome: impl Into<String>, caminho: &Path) -> Resultado<Self> {
        let d = Self {
            nome: nome.into(),
            tipo: TipoDeDestino::PastaLocal,
            alvo: caminho.to_string_lossy().into_owned(),
            ativo: true,
        };
        d.validar()?;
        Ok(d)
    }

    /// Confere o que dá para conferir sem rede.
    ///
    /// # Errors
    /// [`Erro::DestinoInvalido`].
    pub fn validar(&self) -> Resultado<()> {
        validar_destino(&self.alvo)?;
        if self.nome.trim().is_empty() {
            return Err(Erro::DestinoInvalido(self.nome.clone()));
        }
        let bom = match self.tipo {
            TipoDeDestino::PastaLocal => Path::new(&self.alvo).is_absolute(),
            TipoDeDestino::Drive | TipoDeDestino::OneDrive => self
                .alvo
                .split_once(':')
                .is_some_and(|(r, _)| !r.is_empty()),
        };
        if bom {
            Ok(())
        } else {
            Err(Erro::DestinoInvalido(self.alvo.clone()))
        }
    }

    /// A estante deste destino.
    ///
    /// # Errors
    /// [`Erro::DestinoInvalido`].
    pub fn estante(&self, rclone: &Rclone) -> Resultado<Estante> {
        self.validar()?;
        Estante::nova(rclone.clone(), self.alvo.clone())
    }
}

/// A lista de destinos guardada nas preferências do app.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListaDeDestinos {
    destinos: Vec<Destino>,
}

impl ListaDeDestinos {
    /// Lista vazia (primeira abertura: a janela pergunta a pasta local).
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Os destinos, na ordem em que foram adicionados.
    #[must_use]
    pub fn todos(&self) -> &[Destino] {
        &self.destinos
    }

    /// Só os ativos.
    pub fn ativos(&self) -> impl Iterator<Item = &Destino> {
        self.destinos.iter().filter(|d| d.ativo)
    }

    /// Nenhum destino ainda: é a deixa para perguntar a pasta local.
    #[must_use]
    pub fn vazia(&self) -> bool {
        self.destinos.is_empty()
    }

    /// Acrescenta um destino.
    ///
    /// # Errors
    /// [`Erro::DestinoInvalido`] ou [`Erro::DestinoRepetido`] (nome já usado).
    pub fn adicionar(&mut self, destino: Destino) -> Resultado<()> {
        destino.validar()?;
        if self.destinos.iter().any(|d| d.nome == destino.nome) {
            return Err(Erro::DestinoRepetido(destino.nome));
        }
        self.destinos.push(destino);
        Ok(())
    }

    /// Liga ou desliga um destino. Devolve `false` se não existe. Desligar nunca apaga nada
    /// do destino: ele só deixa de ser atualizado.
    pub fn definir_ativo(&mut self, nome: &str, ativo: bool) -> bool {
        self.destinos
            .iter_mut()
            .find(|d| d.nome == nome)
            .map(|d| d.ativo = ativo)
            .is_some()
    }

    /// Tira o destino da lista (o conteúdo dele fica intacto). Devolve `false` se não existe.
    pub fn remover(&mut self, nome: &str) -> bool {
        let antes = self.destinos.len();
        self.destinos.retain(|d| d.nome != nome);
        self.destinos.len() != antes
    }

    /// Lê as preferências. Arquivo ausente dá lista vazia.
    ///
    /// # Errors
    /// [`Erro::Io`] ou [`Erro::PreferenciasIlegiveis`].
    pub fn carregar(arquivo: &Path) -> Resultado<Self> {
        match std::fs::read(arquivo) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| Erro::PreferenciasIlegiveis(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::nova()),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava as preferências (arquivo temporário ao lado e troca, sem deixar meio arquivo).
    ///
    /// # Errors
    /// [`Erro::Io`].
    pub fn salvar(&self, arquivo: &Path) -> Resultado<()> {
        use std::io::Write as _;
        let pasta = arquivo
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(pasta)?;
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| Erro::PreferenciasIlegiveis(e.to_string()))?;
        let mut temporario = tempfile::NamedTempFile::new_in(pasta)?;
        temporario.write_all(&json)?;
        temporario.as_file().sync_all()?;
        temporario.persist(arquivo).map_err(|e| e.error)?;
        Ok(())
    }
}

/// Lê a saída de `rclone listremotes --long` (`nome: tipo` por linha) e sugere destinos para
/// os remotos `drive` e `onedrive`, inativos até o usuário aceitar. Puro.
#[must_use]
pub fn sugerir_de_listagem(saida: &str) -> Vec<Destino> {
    saida
        .lines()
        .filter_map(|linha| {
            let (remoto, tipo) = linha.split_once(':')?;
            let remoto = remoto.trim();
            let tipo = match tipo.trim() {
                "drive" => TipoDeDestino::Drive,
                "onedrive" => TipoDeDestino::OneDrive,
                _ => return None,
            };
            let destino = Destino {
                nome: remoto.to_owned(),
                tipo,
                alvo: format!("{remoto}:{PASTA_NA_NUVEM}"),
                ativo: false,
            };
            (!remoto.is_empty() && destino.validar().is_ok()).then_some(destino)
        })
        .collect()
}

/// Os remotos `drive` e `onedrive` já configurados no rclone, como destinos sugeridos.
/// Só lê a configuração; não abre nem lista o conteúdo de nenhum remoto.
///
/// # Errors
/// Falha ao rodar o rclone.
pub fn detectar_remotos(rclone: &Rclone) -> Resultado<Vec<Destino>> {
    let saida = rclone.executar(["listremotes", "--long"], &[], None)?;
    Ok(sugerir_de_listagem(&saida))
}

/// O que aconteceu num destino.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioDoDestino {
    /// Nome do destino.
    pub destino: String,
    /// O relatório da atualização, ou a mensagem da falha que impediu o destino inteiro.
    pub resultado: Result<RelatorioDaEstante, String>,
}

/// Atualiza todos os destinos ativos, um depois do outro. A falha de um fica no relatório
/// dele. `progresso` recebe o nome do destino e o andamento.
pub fn atualizar_destinos(
    rclone: &Rclone,
    lista: &ListaDeDestinos,
    conexao: &Connection,
    cofre: &Path,
    progresso: &(dyn Fn(&str, Andamento) + Sync),
) -> Vec<RelatorioDoDestino> {
    lista
        .ativos()
        .map(|d| RelatorioDoDestino {
            destino: d.nome.clone(),
            resultado: d
                .estante(rclone)
                .and_then(|e| e.atualizar(conexao, cofre, &|a| progresso(&d.nome, a)))
                .map_err(|e| e.to_string()),
        })
        .collect()
}

/// O que cada destino ativo faria, sem mexer em nada ("N a enviar" por destino).
pub fn planejar_destinos(
    rclone: &Rclone,
    lista: &ListaDeDestinos,
    conexao: &Connection,
    cofre: &Path,
) -> Vec<(String, Result<Plano, String>)> {
    lista
        .ativos()
        .map(|d| {
            let plano = d
                .estante(rclone)
                .and_then(|e| e.planejar(conexao, cofre))
                .map_err(|e| e.to_string());
            (d.nome.clone(), plano)
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sugere_so_drive_e_onedrive() {
        let saida = "gdrive: drive\nOneDrive: onedrive\nlocal: local\ncripto: crypt\n\nlixo\n";
        let s = sugerir_de_listagem(saida);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].tipo, TipoDeDestino::Drive);
        assert_eq!(s[0].alvo, "gdrive:SBF/Estante");
        assert_eq!(s[1].tipo, TipoDeDestino::OneDrive);
        assert!(s.iter().all(|d| !d.ativo));
    }

    #[test]
    fn validacao_dos_alvos() {
        assert!(Destino::pasta_local("casa", Path::new("relativa/x")).is_err());
        assert!(Destino::pasta_local("casa", Path::new("/tmp/x")).is_ok());
        assert!(Destino::nuvem("d", TipoDeDestino::Drive, "-x:y").is_err());
        assert!(Destino::nuvem("d", TipoDeDestino::Drive, "semremoto").is_err());
        assert!(Destino::nuvem("", TipoDeDestino::Drive, "a:b").is_err());
    }

    #[test]
    fn lista_nao_repete_nome_e_persiste() {
        let mut l = ListaDeDestinos::nova();
        l.adicionar(Destino::pasta_local("casa", Path::new("/tmp/a")).expect("d"))
            .expect("1");
        assert!(matches!(
            l.adicionar(Destino::pasta_local("casa", Path::new("/tmp/b")).expect("d")),
            Err(Erro::DestinoRepetido(_))
        ));
        assert!(l.definir_ativo("casa", false));
        assert_eq!(l.ativos().count(), 0);
        let pasta = tempfile::tempdir().expect("pasta");
        let arq = pasta.path().join("sub/destinos.json");
        l.salvar(&arq).expect("salvar");
        assert_eq!(ListaDeDestinos::carregar(&arq).expect("ler"), l);
        assert!(
            ListaDeDestinos::carregar(&pasta.path().join("nao.json"))
                .expect("ausente")
                .vazia()
        );
        std::fs::write(&arq, "{ruim").expect("estragar");
        assert!(matches!(
            ListaDeDestinos::carregar(&arq),
            Err(Erro::PreferenciasIlegiveis(_))
        ));
        assert!(l.remover("casa") && !l.remover("casa"));
    }
}
