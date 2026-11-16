//! O banco do planejamento: `planejamento.sqlite` (ADR-019).
//!
//! - Migrações por `PRAGMA user_version`, WAL e `foreign_keys`.
//! - **Só cresce**: o histórico não aceita `UPDATE` nem `DELETE` (gatilhos), e recorrentes,
//!   versões, pausas e exceções não aceitam `DELETE`. A exclusão é lógica.
//! - As ocorrências nunca são gravadas; o banco guarda só as regras e as decisões.
//! - Nenhum valor ou nome vai a log: os erros aqui carregam só a mensagem do SQLite.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{
    AlvoDoDocumento, Assinatura, Autor, Cancelamento, Centavos, Competencia, Composicao,
    Conciliacao, Data, DocumentoVinculado, ErroDoPlanejamento, ExcecaoGravada, Exclusao, Grupo,
    IdOcorrencia, Pausa, Recorrente, Resultado, Versao,
};

/// A versão do esquema que este código cria e entende.
pub const VERSAO_DO_ESQUEMA: u32 = 5;

const ESPERA_OCUPADO: Duration = Duration::from_secs(30);

/// A escada de migrações: o item `i` leva o banco de `i` a `i + 1`.
const MIGRACOES: &[&str] = &[
    // 1: o esquema inicial.
    "
    CREATE TABLE grupos (
        nome    TEXT PRIMARY KEY,
        posicao INTEGER NOT NULL
    );

    CREATE TABLE recorrentes (
        id                 TEXT PRIMARY KEY,
        nome               TEXT NOT NULL,
        grupo              TEXT NOT NULL,
        subgrupo           TEXT,
        posicao            INTEGER NOT NULL,
        natureza           TEXT NOT NULL,
        prazo              TEXT NOT NULL,
        arquivado          INTEGER NOT NULL DEFAULT 0,
        excluido           TEXT,
        versao_do_registro INTEGER NOT NULL,
        criado_em          TEXT NOT NULL
    );

    CREATE TABLE versoes (
        id            TEXT PRIMARY KEY,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        desde         TEXT NOT NULL,
        valor         INTEGER,
        certeza       TEXT NOT NULL,
        fonte         TEXT NOT NULL,
        frequencia    TEXT NOT NULL,
        conta         TEXT,
        centro        TEXT,
        fornecedor    TEXT,
        categoria     TEXT,
        nota          TEXT,
        UNIQUE (recorrente_id, desde)
    );

    CREATE TABLE pausas (
        id            TEXT PRIMARY KEY,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        desde         TEXT NOT NULL,
        ate           TEXT,
        motivo        TEXT NOT NULL
    );

    CREATE TABLE cancelamentos (
        recorrente_id TEXT PRIMARY KEY REFERENCES recorrentes(id),
        desde         TEXT NOT NULL,
        motivo        TEXT NOT NULL,
        comprovado    INTEGER NOT NULL
    );

    CREATE TABLE excecoes (
        id            TEXT PRIMARY KEY,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        ocorrencia    TEXT NOT NULL,
        competencia   TEXT NOT NULL,
        parcela       INTEGER,
        tipo          TEXT NOT NULL,
        excecao       TEXT NOT NULL,
        quando        TEXT NOT NULL,
        autor         TEXT NOT NULL,
        desfeita      INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX excecoes_por_recorrente ON excecoes (recorrente_id);

    CREATE TABLE conciliacoes (
        ocorrencia    TEXT NOT NULL,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        competencia   TEXT NOT NULL,
        parcela       INTEGER,
        movimento     TEXT NOT NULL,
        valor         INTEGER NOT NULL,
        data          TEXT NOT NULL,
        confirmada    INTEGER NOT NULL,
        PRIMARY KEY (ocorrencia, movimento)
    );
    CREATE INDEX conciliacoes_por_recorrente ON conciliacoes (recorrente_id);

    CREATE TABLE propostas (
        id        TEXT PRIMARY KEY,
        alteracao TEXT NOT NULL,
        origem    TEXT NOT NULL,
        chave     TEXT NOT NULL UNIQUE,
        criada_em TEXT NOT NULL,
        expira_em TEXT NOT NULL,
        estado    TEXT NOT NULL,
        motivo    TEXT
    );

    CREATE TABLE origens_importadas (
        sha256        TEXT NOT NULL,
        aba           TEXT NOT NULL,
        linha         INTEGER NOT NULL,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        UNIQUE (sha256, aba, linha)
    );

    CREATE TABLE historico (
        sequencia     INTEGER PRIMARY KEY AUTOINCREMENT,
        id            TEXT NOT NULL UNIQUE,
        quando        TEXT NOT NULL,
        autor         TEXT NOT NULL,
        alvo          TEXT NOT NULL,
        recorrente_id TEXT,
        resumo        TEXT NOT NULL,
        alteracao     TEXT NOT NULL
    );
    CREATE INDEX historico_por_recorrente ON historico (recorrente_id);

    -- O histórico só cresce; o que é regra ou decisão nunca é apagado de fato.
    CREATE TRIGGER historico_sem_update BEFORE UPDATE ON historico
    BEGIN SELECT RAISE(ABORT, 'o histórico só cresce'); END;
    CREATE TRIGGER historico_sem_delete BEFORE DELETE ON historico
    BEGIN SELECT RAISE(ABORT, 'o histórico só cresce'); END;
    CREATE TRIGGER recorrentes_sem_delete BEFORE DELETE ON recorrentes
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    CREATE TRIGGER versoes_sem_delete BEFORE DELETE ON versoes
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    CREATE TRIGGER pausas_sem_delete BEFORE DELETE ON pausas
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    CREATE TRIGGER excecoes_sem_delete BEFORE DELETE ON excecoes
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    ",
    // 2: configuração (início do acompanhamento).
    "
    CREATE TABLE configuracao (
        chave TEXT PRIMARY KEY,
        valor TEXT NOT NULL
    );
    ",
    // 3: o início do acompanhamento passa de competência (AAAA-MM) para data (AAAA-MM-DD);
    // o valor antigo vira o dia 1 do mês.
    "
    UPDATE configuracao SET valor = valor || '-01'
    WHERE chave = 'inicio_do_acompanhamento' AND length(valor) = 7;
    ",
    // 4: composição do salário (rubricas) e documentos vinculados. Sem DELETE: a composição
    // de mesmo `desde` é substituída no lugar e o documento é desvinculado por marca.
    "
    CREATE TABLE composicoes (
        id            TEXT PRIMARY KEY,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        desde         TEXT NOT NULL,
        rubricas      TEXT NOT NULL,
        documento     TEXT,
        origem        TEXT NOT NULL,
        quando        TEXT NOT NULL,
        autor         TEXT NOT NULL,
        UNIQUE (recorrente_id, desde)
    );

    CREATE TABLE documentos_vinculados (
        id            TEXT PRIMARY KEY,
        recorrente_id TEXT NOT NULL REFERENCES recorrentes(id),
        ocorrencia    TEXT,
        sha256        TEXT NOT NULL,
        tipo          TEXT NOT NULL,
        nome          TEXT NOT NULL,
        data          TEXT,
        quando        TEXT NOT NULL,
        autor         TEXT NOT NULL,
        desvinculado  INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX documentos_por_recorrente ON documentos_vinculados (recorrente_id);

    CREATE TRIGGER composicoes_sem_delete BEFORE DELETE ON composicoes
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    CREATE TRIGGER documentos_vinculados_sem_delete BEFORE DELETE ON documentos_vinculados
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    ",
    // 5: contas (apelido do planejamento -> conta do extrato) e assinaturas dos recorrentes.
    // Sem DELETE: desfazer um par ou tirar uma assinatura deixa a linha com o campo vazio, e o
    // que mudou fica no histórico.
    "
    CREATE TABLE contas_do_planejamento (
        apelido          TEXT PRIMARY KEY,
        conta_do_extrato TEXT,
        quando           TEXT NOT NULL,
        autor            TEXT NOT NULL
    );

    CREATE TABLE assinaturas (
        recorrente_id TEXT PRIMARY KEY REFERENCES recorrentes(id),
        texto         TEXT,
        janela_dias   INTEGER,
        quando        TEXT NOT NULL,
        autor         TEXT NOT NULL
    );

    CREATE TRIGGER contas_do_planejamento_sem_delete BEFORE DELETE ON contas_do_planejamento
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    CREATE TRIGGER assinaturas_sem_delete BEFORE DELETE ON assinaturas
    BEGIN SELECT RAISE(ABORT, 'a exclusão é lógica'); END;
    ",
];

impl From<rusqlite::Error> for ErroDoPlanejamento {
    fn from(e: rusqlite::Error) -> Self {
        ErroDoPlanejamento::Banco(e.to_string())
    }
}

impl From<serde_json::Error> for ErroDoPlanejamento {
    fn from(e: serde_json::Error) -> Self {
        ErroDoPlanejamento::Banco(format!("json: {e}"))
    }
}

pub(crate) fn json<T: Serialize>(valor: &T) -> Resultado<String> {
    Ok(serde_json::to_string(valor)?)
}

pub(crate) fn de_json<T: DeserializeOwned>(texto: &str) -> Resultado<T> {
    Ok(serde_json::from_str(texto)?)
}

pub(crate) fn ler_data(texto: &str) -> Resultado<Data> {
    Data::de_iso(texto).ok_or_else(|| ErroDoPlanejamento::Banco("data inválida no banco".into()))
}

pub(crate) fn ler_competencia(texto: &str) -> Resultado<Competencia> {
    Competencia::de_texto(texto)
        .ok_or_else(|| ErroDoPlanejamento::Banco("competência inválida no banco".into()))
}

/// O planejamento no SQLite. Quem escreve e quem lê passam por aqui.
#[derive(Debug)]
pub struct Banco {
    conexao: Connection,
}

impl Banco {
    /// Abre (ou cria, com permissão 0600 em Unix) o arquivo e migra até a versão atual.
    ///
    /// # Errors
    /// Falha do SQLite, ou banco criado por uma versão mais nova do app.
    pub fn abrir(caminho: impl AsRef<Path>) -> Resultado<Banco> {
        let caminho = caminho.as_ref();
        criar_privado(caminho)?;
        let conexao = Connection::open(caminho)?;
        Banco::preparar(conexao)
    }

    /// Um banco só na memória, para testes.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn em_memoria() -> Resultado<Banco> {
        Banco::preparar(Connection::open_in_memory()?)
    }

    fn preparar(conexao: Connection) -> Resultado<Banco> {
        conexao.busy_timeout(ESPERA_OCUPADO)?;
        // `journal_mode` devolve uma linha; em memória ele fica em `memory`, e tudo bem.
        conexao.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conexao.pragma_update(None, "foreign_keys", true)?;
        conexao.pragma_update(None, "synchronous", "NORMAL")?;
        let mut banco = Banco { conexao };
        banco.migrar()?;
        Ok(banco)
    }

    fn migrar(&mut self) -> Resultado<()> {
        let atual = self.versao_do_esquema()?;
        if atual > VERSAO_DO_ESQUEMA {
            return Err(ErroDoPlanejamento::Banco(format!(
                "o banco é da versão {atual}; este app só entende até a {VERSAO_DO_ESQUEMA}"
            )));
        }
        for (indice, sql) in MIGRACOES.iter().enumerate().skip(atual as usize) {
            let tx = self.conexao.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(
                None,
                "user_version",
                i64::try_from(indice + 1).unwrap_or(i64::MAX),
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    /// `PRAGMA user_version` agora.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn versao_do_esquema(&self) -> Resultado<u32> {
        Ok(self
            .conexao
            .query_row("PRAGMA user_version", [], |l| l.get(0))?)
    }

    /// A conexão, para leitura e para as transações do serviço.
    pub fn conexao(&self) -> &Connection {
        &self.conexao
    }

    pub(crate) fn conexao_mut(&mut self) -> &mut Connection {
        &mut self.conexao
    }
}

/// Cria o arquivo vazio com 0600 antes de o SQLite abri-lo (os arquivos `-wal` e `-shm`
/// herdam a permissão do principal).
fn criar_privado(caminho: &Path) -> Resultado<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(caminho)
            .map(|_| ())
            .map_err(|e| ErroDoPlanejamento::Banco(format!("não consegui criar o arquivo: {e}")))
    }
    #[cfg(not(unix))]
    {
        let _ = caminho;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------------------

/// Tudo o que a projeção precisa, lido de uma vez.
#[derive(Debug, Default)]
pub(crate) struct Dados {
    pub recorrentes: Vec<Recorrente>,
    pub excecoes: HashMap<String, Vec<ExcecaoGravada>>,
    pub conciliacoes: HashMap<String, Vec<Conciliacao>>,
    pub grupos: Vec<Grupo>,
    /// O início do acompanhamento, se definido.
    pub inicio: Option<Data>,
}

impl Dados {
    pub fn excecoes_de(&self, id: &str) -> &[ExcecaoGravada] {
        self.excecoes.get(id).map_or(&[], Vec::as_slice)
    }

    pub fn conciliacoes_de(&self, id: &str) -> &[Conciliacao] {
        self.conciliacoes.get(id).map_or(&[], Vec::as_slice)
    }
}

struct LinhaDeRecorrente {
    id: String,
    nome: String,
    grupo: String,
    subgrupo: Option<String>,
    posicao: i64,
    natureza: String,
    prazo: String,
    arquivado: bool,
    excluido: Option<String>,
    versao_do_registro: i64,
}

fn linha_de_recorrente(l: &Row<'_>) -> rusqlite::Result<LinhaDeRecorrente> {
    Ok(LinhaDeRecorrente {
        id: l.get(0)?,
        nome: l.get(1)?,
        grupo: l.get(2)?,
        subgrupo: l.get(3)?,
        posicao: l.get(4)?,
        natureza: l.get(5)?,
        prazo: l.get(6)?,
        arquivado: l.get(7)?,
        excluido: l.get(8)?,
        versao_do_registro: l.get(9)?,
    })
}

const COLUNAS_DE_RECORRENTE: &str =
    "id, nome, grupo, subgrupo, posicao, natureza, prazo, arquivado, excluido, versao_do_registro";

struct LinhaDeVersao {
    recorrente_id: String,
    versao: Versao,
}

fn ler_versoes(conexao: &Connection, so_o: Option<&str>) -> Resultado<Vec<LinhaDeVersao>> {
    let mut stmt = conexao.prepare(
        "SELECT recorrente_id, id, desde, valor, certeza, fonte, frequencia,
                conta, centro, fornecedor, categoria, nota
         FROM versoes WHERE (?1 IS NULL OR recorrente_id = ?1)
         ORDER BY recorrente_id, desde",
    )?;
    let brutas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<i64>>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, String>(6)?,
                [
                    l.get::<_, Option<String>>(7)?,
                    l.get::<_, Option<String>>(8)?,
                    l.get::<_, Option<String>>(9)?,
                    l.get::<_, Option<String>>(10)?,
                    l.get::<_, Option<String>>(11)?,
                ],
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    brutas
        .into_iter()
        .map(
            |(
                recorrente_id,
                id,
                desde,
                valor,
                certeza,
                fonte,
                frequencia,
                [conta, centro, fornecedor, categoria, nota],
            )| {
                Ok(LinhaDeVersao {
                    recorrente_id,
                    versao: Versao {
                        id,
                        desde: ler_competencia(&desde)?,
                        valor: valor.map(Centavos),
                        certeza: de_json(&certeza)?,
                        fonte: de_json(&fonte)?,
                        frequencia: de_json(&frequencia)?,
                        conta,
                        centro,
                        fornecedor,
                        categoria,
                        nota,
                    },
                })
            },
        )
        .collect()
}

fn ler_pausas(conexao: &Connection, so_o: Option<&str>) -> Resultado<HashMap<String, Vec<Pausa>>> {
    let mut stmt = conexao.prepare(
        "SELECT recorrente_id, id, desde, ate, motivo FROM pausas
         WHERE (?1 IS NULL OR recorrente_id = ?1) ORDER BY desde, rowid",
    )?;
    let brutas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut mapa: HashMap<String, Vec<Pausa>> = HashMap::new();
    for (recorrente_id, id, desde, ate, motivo) in brutas {
        mapa.entry(recorrente_id).or_default().push(Pausa {
            id,
            desde: ler_competencia(&desde)?,
            ate: ate.as_deref().map(ler_competencia).transpose()?,
            motivo,
        });
    }
    Ok(mapa)
}

fn ler_cancelamentos(
    conexao: &Connection,
    so_o: Option<&str>,
) -> Resultado<HashMap<String, Cancelamento>> {
    let mut stmt = conexao.prepare(
        "SELECT recorrente_id, desde, motivo, comprovado FROM cancelamentos
         WHERE (?1 IS NULL OR recorrente_id = ?1)",
    )?;
    let brutas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, bool>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut mapa = HashMap::new();
    for (recorrente_id, desde, motivo, comprovado) in brutas {
        mapa.insert(
            recorrente_id,
            Cancelamento {
                desde: ler_competencia(&desde)?,
                motivo,
                comprovado,
            },
        );
    }
    Ok(mapa)
}

/// Os recorrentes (com versões, pausas e cancelamento), na ordem de criação.
/// `id` restringe a um; `incluir_excluidos = false` tira os de exclusão lógica.
pub(crate) fn ler_recorrentes(
    conexao: &Connection,
    id: Option<&str>,
    incluir_excluidos: bool,
) -> Resultado<Vec<Recorrente>> {
    let mut stmt = conexao.prepare(&format!(
        "SELECT {COLUNAS_DE_RECORRENTE} FROM recorrentes
         WHERE (?1 IS NULL OR id = ?1) AND (?2 OR excluido IS NULL)
         ORDER BY rowid"
    ))?;
    let linhas = stmt
        .query_map(params![id, incluir_excluidos], linha_de_recorrente)?
        .collect::<Result<Vec<_>, _>>()?;
    let mut versoes: HashMap<String, Vec<Versao>> = HashMap::new();
    for l in ler_versoes(conexao, id)? {
        versoes.entry(l.recorrente_id).or_default().push(l.versao);
    }
    let mut pausas = ler_pausas(conexao, id)?;
    let mut cancelamentos = ler_cancelamentos(conexao, id)?;
    linhas
        .into_iter()
        .map(|l| {
            Ok(Recorrente {
                versoes: versoes.remove(&l.id).unwrap_or_default(),
                pausas: pausas.remove(&l.id).unwrap_or_default(),
                cancelamento: cancelamentos.remove(&l.id),
                natureza: de_json(&l.natureza)?,
                prazo: de_json(&l.prazo)?,
                excluido: l.excluido.as_deref().map(de_json::<Exclusao>).transpose()?,
                id: l.id,
                nome: l.nome,
                grupo: l.grupo,
                subgrupo: l.subgrupo,
                posicao: l.posicao,
                arquivado: l.arquivado,
                versao_do_registro: l.versao_do_registro,
            })
        })
        .collect()
}

pub(crate) fn ler_excecoes(
    conexao: &Connection,
    so_o: Option<&str>,
) -> Resultado<HashMap<String, Vec<ExcecaoGravada>>> {
    let mut stmt = conexao.prepare(
        "SELECT id, recorrente_id, competencia, parcela, excecao, quando, autor, desfeita
         FROM excecoes WHERE (?1 IS NULL OR recorrente_id = ?1) ORDER BY rowid",
    )?;
    let brutas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<u32>>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, String>(6)?,
                l.get::<_, bool>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut mapa: HashMap<String, Vec<ExcecaoGravada>> = HashMap::new();
    for (id, recorrente_id, competencia, parcela, excecao, quando, autor, desfeita) in brutas {
        mapa.entry(recorrente_id.clone())
            .or_default()
            .push(ExcecaoGravada {
                id,
                ocorrencia: IdOcorrencia {
                    recorrente: recorrente_id,
                    competencia: ler_competencia(&competencia)?,
                    parcela,
                },
                excecao: de_json(&excecao)?,
                quando,
                autor: de_json::<Autor>(&autor)?,
                desfeita,
            });
    }
    Ok(mapa)
}

pub(crate) fn ler_conciliacoes(
    conexao: &Connection,
    so_o: Option<&str>,
) -> Resultado<HashMap<String, Vec<Conciliacao>>> {
    let mut stmt = conexao.prepare(
        "SELECT recorrente_id, competencia, parcela, movimento, valor, data, confirmada
         FROM conciliacoes WHERE (?1 IS NULL OR recorrente_id = ?1) ORDER BY rowid",
    )?;
    let brutas = stmt
        .query_map(params![so_o], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<u32>>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, i64>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, bool>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut mapa: HashMap<String, Vec<Conciliacao>> = HashMap::new();
    for (recorrente_id, competencia, parcela, movimento, valor, data, confirmada) in brutas {
        mapa.entry(recorrente_id.clone())
            .or_default()
            .push(Conciliacao {
                ocorrencia: IdOcorrencia {
                    recorrente: recorrente_id,
                    competencia: ler_competencia(&competencia)?,
                    parcela,
                },
                movimento,
                valor: Centavos(valor),
                data: ler_data(&data)?,
                confirmada,
            });
    }
    Ok(mapa)
}

pub(crate) fn ler_grupos(conexao: &Connection) -> Resultado<Vec<Grupo>> {
    let mut stmt = conexao.prepare("SELECT nome, posicao FROM grupos ORDER BY posicao, nome")?;
    let grupos = stmt
        .query_map([], |l| {
            Ok(Grupo {
                nome: l.get(0)?,
                posicao: l.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(grupos)
}

/// Tudo de uma vez: o controle e o calendário projetam a partir disto.
pub(crate) fn carregar_dados(conexao: &Connection) -> Resultado<Dados> {
    Ok(Dados {
        recorrentes: ler_recorrentes(conexao, None, false)?,
        excecoes: ler_excecoes(conexao, None)?,
        conciliacoes: ler_conciliacoes(conexao, None)?,
        grupos: ler_grupos(conexao)?,
        inicio: ler_inicio(conexao)?,
    })
}

pub(crate) const CHAVE_INICIO: &str = "inicio_do_acompanhamento";

/// O início do acompanhamento gravado na configuração.
pub(crate) fn ler_inicio(conexao: &Connection) -> Resultado<Option<Data>> {
    conexao
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            params![CHAVE_INICIO],
            |l| l.get::<_, String>(0),
        )
        .optional()?
        .as_deref()
        // Tolera o valor antigo (competência): vira o dia 1.
        .map(|t| ler_data(t).or_else(|_| ler_competencia(t).map(|c| c.dia(1))))
        .transpose()
}

/// Um recorrente com as exceções e as conciliações dele.
pub(crate) type UmRecorrente = (Recorrente, Vec<ExcecaoGravada>, Vec<Conciliacao>);

/// Um recorrente só, com as exceções e conciliações dele.
pub(crate) fn carregar_um(conexao: &Connection, id: &str) -> Resultado<Option<UmRecorrente>> {
    let Some(rec) = ler_recorrentes(conexao, Some(id), true)?.into_iter().next() else {
        return Ok(None);
    };
    let excecoes = ler_excecoes(conexao, Some(id))?
        .remove(id)
        .unwrap_or_default();
    let conciliacoes = ler_conciliacoes(conexao, Some(id))?
        .remove(id)
        .unwrap_or_default();
    Ok(Some((rec, excecoes, conciliacoes)))
}

/// Um id novo, aleatório (do próprio SQLite), com prefixo legível.
pub(crate) fn novo_id(conexao: &Connection, prefixo: &str) -> Resultado<String> {
    let sufixo: String = conexao.query_row("SELECT lower(hex(randomblob(8)))", [], |l| l.get(0))?;
    Ok(format!("{prefixo}_{sufixo}"))
}

pub(crate) fn existe(
    conexao: &Connection,
    sql: &str,
    parametros: impl rusqlite::Params,
) -> Resultado<bool> {
    Ok(conexao
        .query_row(sql, parametros, |_| Ok(()))
        .optional()?
        .is_some())
}

// ---------------------------------------------------------------------------------------
// Relógio
// ---------------------------------------------------------------------------------------

/// Segundos desde 1970-01-01 UTC, pelo relógio do sistema.
pub(crate) fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// `AAAA-MM-DDTHH:MM:SSZ` (UTC).
pub(crate) fn rfc3339(segundos_unix: i64) -> String {
    let dias = segundos_unix.div_euclid(86_400);
    let resto = segundos_unix.rem_euclid(86_400);
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        Data::de_dias_desde_1970(dias),
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    )
}

/// O inverso de [`rfc3339`] (ignora a fração de segundo; só aceita `Z`).
pub(crate) fn de_rfc3339(texto: &str) -> Option<i64> {
    let (dia, hora) = texto.split_once('T')?;
    let data = Data::de_iso(dia)?;
    let hora = hora.get(0..8)?;
    let h: i64 = hora.get(0..2)?.parse().ok()?;
    let m: i64 = hora.get(3..5)?.parse().ok()?;
    let s: i64 = hora.get(6..8)?.parse().ok()?;
    Some(data.dias_desde_1970() * 86_400 + h * 3600 + m * 60 + s)
}

// ---------------------------------------------------------------------------------------
// Composições e documentos vinculados
// ---------------------------------------------------------------------------------------

/// A composição de maior `desde <= competencia` do recorrente.
pub(crate) fn ler_composicao(
    c: &Connection,
    recorrente: &str,
    competencia: Competencia,
) -> Resultado<Option<Composicao>> {
    let linhas: Vec<(String, String, Option<String>, String)> = {
        let mut stmt = c.prepare(
            "SELECT desde, rubricas, documento, origem FROM composicoes
             WHERE recorrente_id = ?1",
        )?;
        stmt.query_map(params![recorrente], |l| {
            Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?))
        })?
        .collect::<Result<_, _>>()?
    };
    let mut melhor: Option<Composicao> = None;
    for (desde, rubricas, documento, origem) in linhas {
        let desde = ler_competencia(&desde)?;
        if desde > competencia || melhor.as_ref().is_some_and(|m| m.desde >= desde) {
            continue;
        }
        melhor = Some(Composicao {
            recorrente: recorrente.to_owned(),
            desde,
            rubricas: de_json(&rubricas)?,
            documento,
            origem,
        });
    }
    Ok(melhor)
}

/// Os documentos ativos de um recorrente: só os da regra, ou só os de uma ocorrência.
pub(crate) fn ler_documentos(
    c: &Connection,
    recorrente: &str,
    ocorrencia: Option<&str>,
) -> Resultado<Vec<DocumentoVinculado>> {
    let mut stmt = c.prepare(
        "SELECT id, recorrente_id, ocorrencia, sha256, tipo, nome, data, quando, autor, desvinculado
         FROM documentos_vinculados
         WHERE recorrente_id = ?1 AND desvinculado = 0
           AND (ocorrencia IS NULL OR ocorrencia = ?2)
         ORDER BY quando DESC, rowid DESC",
    )?;
    let linhas = stmt
        .query_map(params![recorrente, ocorrencia], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<String>>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, Option<String>>(6)?,
                l.get::<_, String>(7)?,
                l.get::<_, String>(8)?,
                l.get::<_, bool>(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    linhas
        .into_iter()
        .map(
            |(id, rec, oc, sha256, tipo, nome, data, quando, autor, desvinculado)| {
                let alvo = match oc {
                    Some(t) => AlvoDoDocumento::Ocorrencia {
                        id: IdOcorrencia::de_texto(&t).ok_or_else(|| {
                            ErroDoPlanejamento::Banco("ocorrência inválida no banco".into())
                        })?,
                    },
                    None => AlvoDoDocumento::Recorrente { id: rec },
                };
                Ok(DocumentoVinculado {
                    id,
                    alvo,
                    sha256,
                    tipo: de_json(&tipo)?,
                    nome,
                    data: data.as_deref().map(ler_data).transpose()?,
                    quando,
                    autor: de_json(&autor)?,
                    desvinculado,
                })
            },
        )
        .collect()
}

// ---------------------------------------------------------------------------------------
// Contas e assinaturas
// ---------------------------------------------------------------------------------------

/// Os pares confirmados: apelido -> conta do extrato (só os que têm conta).
pub(crate) fn ler_pares_de_conta(c: &Connection) -> Resultado<HashMap<String, String>> {
    let mut stmt = c.prepare(
        "SELECT apelido, conta_do_extrato FROM contas_do_planejamento
         WHERE conta_do_extrato IS NOT NULL",
    )?;
    let pares = stmt
        .query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)))?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(pares)
}

/// As assinaturas ativas: recorrente -> assinatura.
pub(crate) fn ler_assinaturas(c: &Connection) -> Resultado<HashMap<String, Assinatura>> {
    let mut stmt = c.prepare(
        "SELECT recorrente_id, texto, janela_dias FROM assinaturas
         WHERE texto IS NOT NULL AND janela_dias IS NOT NULL",
    )?;
    let linhas = stmt
        .query_map([], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(linhas
        .into_iter()
        .map(|(rec, texto, janela)| {
            (
                rec,
                Assinatura {
                    texto,
                    janela_dias: u8::try_from(janela).unwrap_or(u8::MAX),
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn rfc3339_vai_e_volta() {
        let t = 1_794_000_000;
        assert_eq!(de_rfc3339(&rfc3339(t)), Some(t));
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn migra_do_zero_e_nao_repete() {
        let banco = Banco::em_memoria().expect("banco");
        assert_eq!(
            banco.versao_do_esquema().expect("versão"),
            VERSAO_DO_ESQUEMA
        );
    }
}
