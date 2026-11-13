//! Dono: agente do núcleo (spec 06, "O banco"). Ver `lib.rs`.
//!
//! [`Inventario`] é o dono do `inventario.sqlite` a partir da fatia 1:
//!
//! 1. [`Inventario::abrir`] reconhece o banco (vazio, do Bun "2", do Rust numa versão N) e
//!    **recusa** banco mais novo que o app. Abrir não muda nada no arquivo (INV-01): o
//!    reconhecimento usa uma conexão só de leitura, e um arquivo que não existe nem é
//!    criado. A conexão de escrita, cujos PRAGMAs (WAL) gravam no cabeçalho, só abre na
//!    primeira escrita: [`Inventario::migrar`] ou a primeira decisão.
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

use bancada_armazem::{
    Armazem, Pragmas, Sincronizacao, pendentes, verificar_integridade_de_arquivo,
};
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

/// Abre uma conexão **só de leitura** ao banco em `caminho`, com a mesma espera por lock,
/// para a leitura da janela (WAL: não bloqueia o escritor). Não guarde um `SELECT` aberto
/// entre quadros: ele fixa um snapshot.
///
/// # Errors
/// Falha do SQLite ao abrir.
pub fn abrir_leitura(caminho: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conexao = Connection::open_with_flags(
        caminho,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conexao.busy_timeout(ESPERA_OCUPADO)?;
    Ok(conexao)
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

/// Uma etapa de [`Inventario::migrar_com_andamento`], avisada **antes** de começar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EtapaDaMigracao {
    /// Cópia do banco pela API de backup.
    Copia,
    /// `PRAGMA integrity_check` da cópia.
    ConferenciaDaCopia,
    /// Adoção do banco do Bun: confere as tabelas e carimba a versão 1.
    Adocao,
    /// Uma migração da escada (no banco vazio, a 1 também).
    Migracao { versao: u32, descricao: String },
    /// Reconhece o banco migrado.
    Conferencia,
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

/// O inventário. Uma instância por processo; a leitura da janela usa outra conexão
/// (aberta por [`caminho`](Self::caminho)).
#[derive(Debug)]
pub struct Inventario {
    caminho: PathBuf,
    estado: EstadoDoBanco,
    /// Só leitura (em memória, vazia, enquanto o arquivo não existe): reconhece e planeja.
    leitura: Connection,
    /// A conexão de escrita, aberta na primeira escrita.
    armazem: Option<Armazem>,
}

/// Copia o banco para `destino` pela API de backup, num passo só.
///
/// Difere de `bancada_armazem::copiar_banco` (128 páginas por passo, 2 ms entre passos)
/// porque, com os scripts Bun gravando, cada escrita entre dois passos recomeça o backup;
/// num passo só, a cópia é um retrato de uma transação de leitura, e em WAL ela não
/// bloqueia quem escreve. Num inventário sintético de 400 MB, as pausas eram 70% do tempo
/// da cópia. Mesmas garantias: grava em `<destino>.parcial-<pid>`, renomeia no fim, nunca
/// sobrescreve e sai um arquivo só (sem `-wal`).
fn copiar_de_uma_vez(origem: &Connection, destino: &Path) -> Result<(), ErroDoBanco> {
    use rusqlite::backup::{Backup, StepResult};

    if destino.exists() {
        return Err(bancada_armazem::Erro::DestinoJaExiste(destino.to_path_buf()).into());
    }
    let mut nome = destino.as_os_str().to_owned();
    nome.push(format!(".parcial-{}", std::process::id()));
    let parcial = PathBuf::from(nome);
    let copiar = || -> Result<(), ErroDoBanco> {
        let mut copia = Connection::open(&parcial)?;
        {
            let backup = Backup::new(origem, &mut copia)?;
            loop {
                match backup.step(-1)? {
                    StepResult::Done => break,
                    StepResult::More => {}
                    _ => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        }
        copia.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))?;
        copia.close().map_err(|(_, e)| ErroDoBanco::Sqlite(e))?;
        std::fs::rename(&parcial, destino)
            .map_err(|e| ErroDoBanco::Armazem(bancada_armazem::Erro::Io(e)))
    };
    let resultado = copiar();
    if resultado.is_err() {
        for sufixo in ["", "-wal", "-shm", "-journal"] {
            let mut nome = parcial.as_os_str().to_owned();
            nome.push(sufixo);
            let _ = std::fs::remove_file(PathBuf::from(nome));
        }
    }
    resultado
}

fn abrir_reconhecimento(caminho: &Path) -> rusqlite::Result<Connection> {
    if caminho.exists() {
        abrir_leitura(caminho)
    } else {
        Connection::open_in_memory()
    }
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
    /// Reconhece o banco em `caminho` por uma conexão só de leitura. Não migra, não grava
    /// e não cria o arquivo (um caminho que não existe é um banco vazio).
    ///
    /// # Errors
    /// [`ErroDoBanco::MaisNovo`] (banco de um app mais novo), [`ErroDoBanco::NaoEhDoBun`],
    /// [`ErroDoBanco::NaoBate`] ou falha do SQLite.
    pub fn abrir(caminho: impl AsRef<Path>) -> Result<Self, ErroDoBanco> {
        let caminho = caminho.as_ref().to_path_buf();
        let leitura = abrir_reconhecimento(&caminho)?;
        let estado = reconhecer(&leitura)?;
        Ok(Self {
            caminho,
            estado,
            leitura,
            armazem: None,
        })
    }

    /// A conexão de escrita, aberta agora se ainda não estava. Abrir aplica os PRAGMAs
    /// (WAL), que gravam no cabeçalho de um arquivo novo ou vazio.
    ///
    /// # Errors
    /// Falha do SQLite ao abrir ou configurar.
    pub fn conexao_de_escrita(&mut self) -> Result<&mut Armazem, ErroDoBanco> {
        let armazem = match self.armazem.take() {
            Some(a) => a,
            None => Armazem::abrir(&self.caminho, &pragmas())?,
        };
        Ok(self.armazem.insert(armazem))
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
        Ok(self
            .conexao()
            .query_row("PRAGMA user_version", [], |l| l.get(0))?)
    }

    /// Para testes e ferramentas de manutenção: a conexão de escrita, se já foi aberta
    /// (por [`migrar`](Self::migrar), uma decisão ou
    /// [`conexao_de_escrita`](Self::conexao_de_escrita)); senão a de leitura. Não guarde
    /// transação aberta nela.
    pub fn conexao(&self) -> &Connection {
        match &self.armazem {
            Some(armazem) => armazem.conexao(),
            None => &self.leitura,
        }
    }

    /// O que [`migrar`](Self::migrar) vai mudar. Não grava nada.
    ///
    /// # Errors
    /// [`ErroDoBanco::MaisNovo`] ou falha do SQLite.
    pub fn plano(&self) -> Result<PlanoDeMigracao, ErroDoBanco> {
        let escada = migracoes::escada();
        let versao_atual = self.versao()?;
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
        for (i, migracao) in pendentes(self.conexao(), &escada)?.iter().enumerate() {
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
        self.migrar_com_andamento(copia, &|_| {})
    }

    /// Como [`migrar`](Self::migrar), avisando `andamento` antes de cada etapa, para a
    /// janela mostrar onde está. O aviso roda na thread que migra.
    ///
    /// # Errors
    /// Os de [`migrar`](Self::migrar).
    pub fn migrar_com_andamento(
        &mut self,
        copia: impl AsRef<Path>,
        andamento: &dyn Fn(EtapaDaMigracao),
    ) -> Result<MigracaoFeita, ErroDoBanco> {
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
            andamento(EtapaDaMigracao::Copia);
            copiar_de_uma_vez(&self.leitura, &destino)?;
            andamento(EtapaDaMigracao::ConferenciaDaCopia);
            verificar_integridade_de_arquivo(&destino)?;
            Some(destino)
        } else {
            None
        };
        let escada = migracoes::escada();
        let do_bun = self.estado == EstadoDoBanco::DoBun;
        let armazem = self.conexao_de_escrita()?;
        let mut adotou_o_bun = false;
        let mut versao = plano.versao_atual;
        if do_bun {
            andamento(EtapaDaMigracao::Adocao);
            let relatorio = armazem.migrar_ate(&escada, 1)?;
            adotou_o_bun = relatorio.linha_de_base_carimbada;
            versao = relatorio.versao_final;
        }
        for passo in plano.passos.iter().filter(|p| p.versao > versao) {
            andamento(EtapaDaMigracao::Migracao {
                versao: passo.versao,
                descricao: passo.descricao.clone(),
            });
            armazem.migrar_ate(&escada, passo.versao)?;
        }
        andamento(EtapaDaMigracao::Conferencia);
        let versao_final = armazem.versao()?;
        self.estado = reconhecer(self.conexao())?;
        // O arquivo pode ter acabado de nascer: a leitura passa a olhar para ele.
        self.leitura = abrir_leitura(&self.caminho)?;
        Ok(MigracaoFeita {
            versao_inicial: plano.versao_atual,
            versao_final,
            copia,
            adotou_o_bun,
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
        self.conexao_de_escrita()
            .map_err(|e| ErroDeComando::Banco(e.to_string()))?
            .transacao_imediata(corpo)
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
