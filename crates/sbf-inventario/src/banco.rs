//! Dono: agente do núcleo (spec 06, "O banco"). Ver `lib.rs`.
//!
//! [`Inventario`] é o dono do `inventario.sqlite` a partir da fatia 1:
//!
//! 1. [`Inventario::abrir`] reconhece o banco (vazio, do Bun "2", do Rust numa versão N) e
//!    **recusa** banco mais novo que o app. Abrir não muda nada no arquivo (INV-01).
//! 2. [`Inventario::plano`] diz o que a migração vai mudar, para a janela mostrar.
//! 3. [`Inventario::migrar`] faz primeiro a cópia pela API de backup do SQLite e só depois
//!    migra. Sem essa chamada explícita, nada muda até a primeira decisão.
//! 4. As escritas são as de [`Comandos`]: cada uma numa transação IMMEDIATE curta, que
//!    grava a decisão e a projeção juntas.
//!
//! A leitura da janela abre a sua própria conexão com [`Inventario::caminho`] (WAL: não
//! espera o escritor). Concorrência com os scripts Bun: vale o `LICOES.md`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bancada_armazem::{Armazem, Pragmas, Sincronizacao, verificar_integridade_de_arquivo};
use rusqlite::{Connection, OptionalExtension};

use crate::comando::{Comandos, ErroDeComando, IdDecisao, PedidoDeDecisao, ResultadoDeComando};
use crate::migracoes::{self, VERSAO_ATUAL, VERSAO_DO_BUN_ADOTADA};
use crate::projecao::{self, ResumoDaReprojecao};
use crate::referencia::Ref;
use crate::{decisao, lote};

/// Espera por um lock antes de `SQLITE_BUSY` (LICOES.md: vários scripts gravam juntos).
pub const ESPERA_OCUPADO: Duration = Duration::from_secs(30);

/// Os PRAGMAs do inventário: WAL, `busy_timeout` de 30 s, chaves estrangeiras e
/// `synchronous=NORMAL` (o mesmo do Bun; seguro com WAL).
pub fn pragmas() -> Pragmas {
    Pragmas {
        espera_ocupado: ESPERA_OCUPADO,
        sincronizacao: Sincronizacao::Normal,
        ..Pragmas::default()
    }
}

/// O que [`Inventario::abrir`] encontrou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDoBanco {
    /// Sem tabelas: a escada roda inteira.
    Vazio,
    /// Banco do Bun (`meta.schema_versao='2'`, `user_version=0`): a migração 1 só carimba.
    DoBun,
    /// Banco do Rust em `user_version = N` (de 1 a [`VERSAO_ATUAL`]).
    Rust(u32),
}

/// Um passo do plano de migração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassoDeMigracao {
    /// A versão a que o passo leva.
    pub versao: u32,
    pub descricao: String,
}

/// O que a migração vai mudar, para a janela mostrar antes de migrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanoDeMigracao {
    pub estado: EstadoDoBanco,
    /// `user_version` agora.
    pub versao_atual: u32,
    /// `user_version` depois de migrar.
    pub versao_final: u32,
    /// Os passos, em ordem (vazio quando o banco já está em dia).
    pub passos: Vec<PassoDeMigracao>,
    /// Se [`Inventario::migrar`] vai copiar o banco antes (todo banco com dados).
    pub faz_copia: bool,
}

impl PlanoDeMigracao {
    /// `true` quando há o que migrar.
    pub fn precisa_migrar(&self) -> bool {
        !self.passos.is_empty()
    }
}

/// O que [`Inventario::migrar`] fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigracaoFeita {
    pub versao_inicial: u32,
    pub versao_final: u32,
    /// A cópia feita antes de migrar (`None` em banco vazio ou já em dia).
    pub copia: Option<PathBuf>,
    /// `true` se o banco do Bun foi adotado (carimbo da versão 1).
    pub adotou_o_bun: bool,
}

/// Por que o inventário não abre ou não migra.
#[derive(Debug, thiserror::Error)]
pub enum ErroDoBanco {
    #[error(
        "o banco está na versão {encontrada}, mas este app só entende até a {suportada}: atualize o app"
    )]
    MaisNovo { encontrada: u32, suportada: u32 },
    #[error(
        "o banco tem tabelas mas não é do inventário do Bun na versão {VERSAO_DO_BUN_ADOTADA} (schema_versao = {encontrado:?})"
    )]
    NaoEhDoBun { encontrado: Option<String> },
    #[error("o banco do Bun não bate com a migração 1: faltam {}", faltando.join(", "))]
    NaoBate { faltando: Vec<String> },
    #[error("o banco precisa ser migrado antes da primeira decisão")]
    NaoMigrado,
    #[error("{0}")]
    Armazem(#[from] bancada_armazem::Erro),
    #[error("erro do banco: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// O inventário aberto para escrita. Uma instância por processo; a leitura usa outra
/// conexão (aberta por [`caminho`](Self::caminho)).
#[derive(Debug)]
pub struct Inventario {
    armazem: Armazem,
    caminho: PathBuf,
    estado: EstadoDoBanco,
}

fn tem_tabelas(conexao: &Connection) -> rusqlite::Result<bool> {
    conexao.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master
                        WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\')",
        [],
        |l| l.get(0),
    )
}

fn reconhecer(conexao: &Connection) -> Result<EstadoDoBanco, ErroDoBanco> {
    let versao: u32 = conexao.query_row("PRAGMA user_version", [], |l| l.get(0))?;
    if versao > VERSAO_ATUAL {
        return Err(ErroDoBanco::MaisNovo {
            encontrada: versao,
            suportada: VERSAO_ATUAL,
        });
    }
    if versao > 0 {
        return Ok(EstadoDoBanco::Rust(versao));
    }
    if !tem_tabelas(conexao)? {
        return Ok(EstadoDoBanco::Vazio);
    }
    let tem_meta = migracoes::tem_coluna(conexao, "meta", "valor")?;
    let marcador: Option<String> = if tem_meta {
        conexao
            .query_row(
                "SELECT valor FROM meta WHERE chave = 'schema_versao'",
                [],
                |l| l.get(0),
            )
            .optional()?
    } else {
        None
    };
    if marcador.as_deref() != Some(VERSAO_DO_BUN_ADOTADA) {
        return Err(ErroDoBanco::NaoEhDoBun {
            encontrado: marcador,
        });
    }
    let faltando = migracoes::colunas_que_faltam_na_base(conexao)?;
    if !faltando.is_empty() {
        return Err(ErroDoBanco::NaoBate { faltando });
    }
    Ok(EstadoDoBanco::DoBun)
}

impl Inventario {
    /// Abre o banco em `caminho` (criando o arquivo, se não existe) e reconhece o estado
    /// dele. Não migra e não grava nada.
    ///
    /// # Errors
    /// [`ErroDoBanco::MaisNovo`] (banco de um app mais novo), [`ErroDoBanco::NaoEhDoBun`],
    /// [`ErroDoBanco::NaoBate`] ou falha do SQLite.
    pub fn abrir(caminho: impl AsRef<Path>) -> Result<Self, ErroDoBanco> {
        let caminho = caminho.as_ref().to_path_buf();
        let armazem = Armazem::abrir(&caminho, &pragmas())?;
        let estado = reconhecer(armazem.conexao())?;
        Ok(Self {
            armazem,
            caminho,
            estado,
        })
    }

    /// O caminho do banco, para a leitura abrir a sua própria conexão.
    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// O estado reconhecido ao abrir (ou depois da última migração).
    pub fn estado(&self) -> EstadoDoBanco {
        self.estado
    }

    /// `PRAGMA user_version` agora.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn versao(&self) -> Result<u32, ErroDoBanco> {
        Ok(self.armazem.versao()?)
    }

    /// A conexão de escrita, para testes e ferramentas de manutenção. Não guarde
    /// transação aberta nela.
    pub fn conexao(&self) -> &Connection {
        self.armazem.conexao()
    }

    /// O que [`migrar`](Self::migrar) vai mudar. Não grava nada.
    ///
    /// # Errors
    /// [`ErroDoBanco::MaisNovo`] ou falha do SQLite.
    pub fn plano(&self) -> Result<PlanoDeMigracao, ErroDoBanco> {
        let escada = migracoes::escada();
        let versao_atual = self.armazem.versao()?;
        let mut passos = Vec::new();
        if self.estado == EstadoDoBanco::DoBun {
            passos.push(PassoDeMigracao {
                versao: 1,
                descricao: format!(
                    "adota o banco do Bun (versão {VERSAO_DO_BUN_ADOTADA}): confere as tabelas e só carimba a versão 1"
                ),
            });
        }
        let inicio = match self.estado {
            EstadoDoBanco::DoBun => 2,
            _ => versao_atual + 1,
        };
        for (i, migracao) in self.armazem.pendentes(&escada)?.iter().enumerate() {
            passos.push(PassoDeMigracao {
                versao: inicio + i as u32,
                descricao: migracao.descricao.to_owned(),
            });
        }
        Ok(PlanoDeMigracao {
            estado: self.estado,
            versao_atual,
            versao_final: escada.versao_mais_recente(),
            faz_copia: self.estado != EstadoDoBanco::Vazio && !passos.is_empty(),
            passos,
        })
    }

    /// Um nome para a cópia de antes da migração, ao lado do banco:
    /// `<banco>.antes-da-v<N>-<segundos>.sqlite`.
    pub fn caminho_sugerido_da_copia(&self) -> PathBuf {
        let segundos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let mut nome = self.caminho.as_os_str().to_owned();
        nome.push(format!(".antes-da-v{VERSAO_ATUAL}-{segundos}.sqlite"));
        PathBuf::from(nome)
    }

    /// Migra até [`VERSAO_ATUAL`]. Antes, copia o banco para `copia` pela API de backup do
    /// SQLite e confere a integridade da cópia; se a cópia falha, nada é migrado.
    /// Banco vazio não é copiado. Banco em dia: não faz nada.
    ///
    /// # Errors
    /// Falha da cópia (inclusive `copia` já existir), da linha de base ou de uma migração
    /// (desfeita inteira; as anteriores ficam).
    pub fn migrar(&mut self, copia: impl AsRef<Path>) -> Result<MigracaoFeita, ErroDoBanco> {
        let plano = self.plano()?;
        if !plano.precisa_migrar() {
            return Ok(MigracaoFeita {
                versao_inicial: plano.versao_atual,
                versao_final: plano.versao_atual,
                copia: None,
                adotou_o_bun: false,
            });
        }
        let copia = if plano.faz_copia {
            let destino = copia.as_ref().to_path_buf();
            self.armazem.copiar_para(&destino)?;
            verificar_integridade_de_arquivo(&destino)?;
            Some(destino)
        } else {
            None
        };
        let relatorio = self.armazem.migrar(&migracoes::escada())?;
        self.estado = reconhecer(self.armazem.conexao())?;
        Ok(MigracaoFeita {
            versao_inicial: relatorio.versao_inicial,
            versao_final: relatorio.versao_final,
            copia,
            adotou_o_bun: relatorio.linha_de_base_carimbada,
        })
    }

    fn exigir_migrado(&self) -> ResultadoDeComando<()> {
        if self.estado == EstadoDoBanco::Rust(VERSAO_ATUAL) {
            Ok(())
        } else {
            Err(ErroDeComando::Banco(ErroDoBanco::NaoMigrado.to_string()))
        }
    }

    fn escrever<T>(
        &mut self,
        corpo: impl FnOnce(&rusqlite::Transaction<'_>) -> ResultadoDeComando<T>,
    ) -> ResultadoDeComando<T> {
        self.exigir_migrado()?;
        self.armazem.transacao_imediata(corpo)
    }

    /// Comando de manutenção: refaz a projeção a partir do log de decisões, numa
    /// transação só.
    ///
    /// # Errors
    /// Banco não migrado, decisão ilegível ou falha do SQLite (nada muda).
    pub fn reprojetar(&mut self) -> ResultadoDeComando<ResumoDaReprojecao> {
        self.escrever(projecao::reprojetar)
    }

    /// Enviar ao servidor: desabilitado nesta fatia; o `Err` é o motivo (INV-10).
    ///
    /// # Errors
    /// Sempre, na fatia 1.
    pub fn enviar_ao_servidor(&self, lote: &Ref) -> Result<(), &'static str> {
        lote::enviar_ao_servidor(lote)
    }
}

impl Comandos for Inventario {
    fn executar(&mut self, pedido: &PedidoDeDecisao) -> ResultadoDeComando<Vec<IdDecisao>> {
        self.escrever(|tx| decisao::executar(tx, pedido))
    }

    fn desfazer(&mut self, id: IdDecisao) -> ResultadoDeComando<IdDecisao> {
        self.escrever(|tx| decisao::desfazer(tx, id))
    }

    fn desfazer_ultima(&mut self) -> ResultadoDeComando<Option<IdDecisao>> {
        self.escrever(|tx| match decisao::ultima_desfazivel(tx)? {
            Some(id) => decisao::desfazer(tx, id).map(Some),
            None => Ok(None),
        })
    }

    fn criar_lote(&mut self, nome: &str) -> ResultadoDeComando<Ref> {
        self.escrever(|tx| lote::criar(tx, nome))
    }

    fn adicionar_ao_lote(&mut self, alvo: &Ref, itens: &[Ref]) -> ResultadoDeComando<()> {
        self.escrever(|tx| lote::adicionar(tx, alvo, itens))
    }

    fn remover_do_lote(&mut self, alvo: &Ref, itens: &[Ref]) -> ResultadoDeComando<()> {
        self.escrever(|tx| lote::remover(tx, alvo, itens))
    }

    fn aprovar_lote(&mut self, alvo: &Ref) -> ResultadoDeComando<()> {
        self.escrever(|tx| {
            let em = decisao::agora(tx)?;
            lote::aprovar(tx, alvo, &em, None).map(|_| ())
        })
    }

    fn desaprovar_lote(&mut self, alvo: &Ref) -> ResultadoDeComando<()> {
        self.escrever(|tx| {
            let em = decisao::agora(tx)?;
            lote::desaprovar(tx, alvo, &em, None).map(|_| ())
        })
    }
}
