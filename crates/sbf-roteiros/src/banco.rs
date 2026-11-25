//! O `roteiros.sqlite`, na pasta de dados do app (fora do cofre): roteiros, versões,
//! execuções e eventos. **Nunca guarda senha**: o passo de senha só tem o rótulo do chaveiro.
//!
//! A janela e o worker abrem o mesmo arquivo (WAL, espera de 5 s). A pausa para o usuário
//! passa por aqui: o executor marca a execução como `aguardando_usuario` e fica olhando o
//! campo `continuar`; a janela grava `1` (Continuar) ou `2` (Cancelar).

use crate::erro::{ErroDeRoteiro, Resultado};
use crate::manifesto::{LIMITE_DE_BYTES, Manifesto, tem_caractere_perigoso};
use crate::passo::{Passo, Roteiro, validar_nome};
use rusqlite::{Connection, OptionalExtension as _, params};
use std::path::Path;

const SQL: &str = "
CREATE TABLE IF NOT EXISTS roteiro (
  id            INTEGER PRIMARY KEY,
  nome          TEXT NOT NULL UNIQUE,
  descricao     TEXT NOT NULL DEFAULT '',
  url_inicial   TEXT NOT NULL DEFAULT '',
  rotulo_senha  TEXT,
  agenda_horas  INTEGER,
  ativo         INTEGER NOT NULL DEFAULT 1,
  criado_em     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE IF NOT EXISTS versao (
  id          INTEGER PRIMARY KEY,
  roteiro_id  INTEGER NOT NULL REFERENCES roteiro(id) ON DELETE CASCADE,
  numero      INTEGER NOT NULL,
  passos      TEXT NOT NULL,
  estado      TEXT NOT NULL CHECK (estado IN ('ativa','proposta','anterior','rejeitada')),
  origem      TEXT NOT NULL,
  motivo      TEXT,
  criada_em   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  UNIQUE (roteiro_id, numero)
);
CREATE TABLE IF NOT EXISTS execucao (
  id           INTEGER PRIMARY KEY,
  roteiro_id   INTEGER NOT NULL REFERENCES roteiro(id) ON DELETE CASCADE,
  versao_id    INTEGER NOT NULL REFERENCES versao(id),
  gatilho      TEXT NOT NULL,
  iniciada_em  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  iniciada_unix INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  concluida_em TEXT,
  estado       TEXT NOT NULL DEFAULT 'rodando' CHECK (estado IN
                 ('rodando','aguardando_usuario','concluida','falhou','bloqueada','cancelada')),
  passo_atual  INTEGER NOT NULL DEFAULT 0,
  motivo       TEXT,
  baixados     INTEGER NOT NULL DEFAULT 0,
  novos        INTEGER NOT NULL DEFAULT 0,
  continuar    INTEGER NOT NULL DEFAULT 0,
  pid          INTEGER
);
CREATE INDEX IF NOT EXISTS execucao_roteiro ON execucao(roteiro_id, id);
CREATE TABLE IF NOT EXISTS evento (
  id          INTEGER PRIMARY KEY,
  execucao_id INTEGER NOT NULL REFERENCES execucao(id) ON DELETE CASCADE,
  quando      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  passo       INTEGER,
  tipo        TEXT NOT NULL,
  mensagem    TEXT NOT NULL
);
";

/// O roteiro escrito (Luau): o nome, as versões **imutáveis** e o histórico de estados. Os
/// gatilhos impedem editar ou apagar o que uma versão já guardou; só `estado` e
/// `resultado_do_teste` mudam, e há no máximo uma versão `ativo` por roteiro.
const SQL_ESCRITO: &str = "
CREATE TABLE IF NOT EXISTS roteiro_escrito (
  id        INTEGER PRIMARY KEY,
  nome      TEXT NOT NULL UNIQUE,
  criado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE IF NOT EXISTS roteiro_escrito_versao (
  id          INTEGER PRIMARY KEY,
  roteiro_id  INTEGER NOT NULL REFERENCES roteiro_escrito(id),
  numero      INTEGER NOT NULL,
  manifesto   TEXT NOT NULL,
  fonte       TEXT NOT NULL,
  checksum    TEXT NOT NULL,
  estado      TEXT NOT NULL CHECK (estado IN ('rascunho','testado','ativo','aposentado')),
  autor       TEXT NOT NULL CHECK (autor IN ('usuario','mcp') OR autor LIKE 'ia:_%'),
  motivo      TEXT,
  criada_em   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  resultado_do_teste TEXT,
  UNIQUE (roteiro_id, numero)
);
CREATE UNIQUE INDEX IF NOT EXISTS roteiro_escrito_uma_ativa
  ON roteiro_escrito_versao(roteiro_id) WHERE estado = 'ativo';
CREATE TABLE IF NOT EXISTS roteiro_escrito_evento (
  id        INTEGER PRIMARY KEY,
  versao_id INTEGER NOT NULL REFERENCES roteiro_escrito_versao(id),
  quando    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  de        TEXT,
  para      TEXT NOT NULL,
  quem      TEXT NOT NULL,
  detalhe   TEXT NOT NULL DEFAULT ''
);
CREATE TRIGGER IF NOT EXISTS roteiro_escrito_versao_imutavel
BEFORE UPDATE OF roteiro_id, numero, manifesto, fonte, checksum, autor, motivo, criada_em
ON roteiro_escrito_versao
BEGIN
  SELECT RAISE(ABORT, 'uma versão de roteiro escrito nunca é editada: importe uma versão nova');
END;
CREATE TRIGGER IF NOT EXISTS roteiro_escrito_versao_sem_apagar
BEFORE DELETE ON roteiro_escrito_versao
BEGIN
  SELECT RAISE(ABORT, 'uma versão de roteiro escrito nunca é apagada');
END;
CREATE TRIGGER IF NOT EXISTS roteiro_escrito_estado_sem_voltar
BEFORE UPDATE OF estado ON roteiro_escrito_versao
WHEN NOT (
     (OLD.estado = 'rascunho' AND NEW.estado IN ('rascunho','testado'))
  OR (OLD.estado = 'testado'  AND NEW.estado = 'ativo')
  OR (OLD.estado = 'ativo'    AND NEW.estado = 'aposentado')
  OR (OLD.estado = NEW.estado)
)
BEGIN
  SELECT RAISE(ABORT, 'transição de estado inválida no roteiro escrito');
END;
CREATE TRIGGER IF NOT EXISTS roteiro_escrito_evento_imutavel
BEFORE UPDATE ON roteiro_escrito_evento
BEGIN
  SELECT RAISE(ABORT, 'o histórico do roteiro escrito não se edita');
END;
CREATE TRIGGER IF NOT EXISTS roteiro_escrito_evento_sem_apagar
BEFORE DELETE ON roteiro_escrito_evento
BEGIN
  SELECT RAISE(ABORT, 'o histórico do roteiro escrito não se apaga');
END;
";

/// Como uma versão nasceu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrigemDaVersao {
    Gravacao,
    Conserto,
    Manual,
}

impl OrigemDaVersao {
    fn texto(self) -> &'static str {
        match self {
            Self::Gravacao => "gravacao",
            Self::Conserto => "conserto",
            Self::Manual => "manual",
        }
    }
}

/// O estado de uma execução.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDaExecucao {
    Rodando,
    AguardandoUsuario,
    Concluida,
    Falhou,
    Bloqueada,
    Cancelada,
}

impl EstadoDaExecucao {
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            Self::Rodando => "rodando",
            Self::AguardandoUsuario => "aguardando_usuario",
            Self::Concluida => "concluida",
            Self::Falhou => "falhou",
            Self::Bloqueada => "bloqueada",
            Self::Cancelada => "cancelada",
        }
    }
    pub(crate) fn de(t: &str) -> Self {
        match t {
            "aguardando_usuario" => Self::AguardandoUsuario,
            "concluida" => Self::Concluida,
            "falhou" => Self::Falhou,
            "bloqueada" => Self::Bloqueada,
            "cancelada" => Self::Cancelada,
            _ => Self::Rodando,
        }
    }
    /// O rótulo para a janela.
    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Rodando => "Rodando",
            Self::AguardandoUsuario => "Sua vez no navegador",
            Self::Concluida => "Concluída",
            Self::Falhou => "Falhou",
            Self::Bloqueada => "Bloqueada",
            Self::Cancelada => "Cancelada",
        }
    }
}

/// Uma execução, para o histórico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeExecucao {
    pub id: i64,
    pub roteiro_id: i64,
    pub roteiro: String,
    pub versao: i64,
    pub gatilho: String,
    pub iniciada_em: String,
    pub concluida_em: Option<String>,
    pub estado: EstadoDaExecucao,
    pub passo_atual: i64,
    pub motivo: Option<String>,
    pub baixados: i64,
    pub novos: i64,
}

/// Um roteiro na lista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeRoteiro {
    pub id: i64,
    pub nome: String,
    pub descricao: String,
    pub url_inicial: String,
    pub rotulo_senha: Option<String>,
    pub agenda_horas: Option<i64>,
    pub ativo: bool,
    pub versao: i64,
    pub passos: usize,
    pub ha_proposta: bool,
    pub ultima: Option<LinhaDeExecucao>,
}

/// Um conserto proposto pela IA, à espera do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposta {
    pub versao_id: i64,
    pub roteiro_id: i64,
    pub numero: i64,
    pub passos: Vec<Passo>,
    pub motivo: String,
}

/// Um evento da execução: (quando, passo, tipo, mensagem).
pub type Evento = (String, Option<i64>, String, String);

/// O banco dos roteiros.
#[derive(Debug)]
pub struct BancoDeRoteiros {
    pub(crate) conn: Connection,
}

fn passos_de(json: &str) -> Resultado<Vec<Passo>> {
    serde_json::from_str(json).map_err(|e| ErroDeRoteiro::Invalido(format!("passos: {e}")))
}

impl BancoDeRoteiros {
    /// Abre (e cria) o banco.
    ///
    /// # Errors
    /// Disco ou SQLite.
    pub fn abrir(caminho: &Path) -> Resultado<Self> {
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)?;
        }
        let conn = Connection::open(caminho)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |l| l.get(0))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SQL)?;
        conn.execute_batch(SQL_ESCRITO)?;
        conn.execute_batch(crate::execucao_escrita::SQL)?;
        Ok(Self { conn })
    }

    /// Em memória, para testes.
    ///
    /// # Errors
    /// SQLite.
    pub fn em_memoria() -> Resultado<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SQL)?;
        conn.execute_batch(SQL_ESCRITO)?;
        conn.execute_batch(crate::execucao_escrita::SQL)?;
        Ok(Self { conn })
    }

    /// Cria um roteiro com a primeira versão ativa.
    ///
    /// # Errors
    /// Nome inválido ou repetido, ou falha do banco.
    pub fn criar_roteiro(
        &self,
        nome: &str,
        descricao: &str,
        url_inicial: &str,
        rotulo_senha: Option<&str>,
        passos: &[Passo],
        origem: OrigemDaVersao,
    ) -> Resultado<i64> {
        validar_nome(nome).map_err(ErroDeRoteiro::Invalido)?;
        let r = Roteiro {
            formato: crate::passo::FORMATO,
            nome: nome.to_owned(),
            descricao: descricao.to_owned(),
            passos: passos.to_vec(),
        };
        r.validar().map_err(ErroDeRoteiro::Invalido)?;
        self.conn.execute(
            "INSERT INTO roteiro (nome, descricao, url_inicial, rotulo_senha) VALUES (?1,?2,?3,?4)",
            params![nome, descricao, url_inicial, rotulo_senha],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT INTO versao (roteiro_id, numero, passos, estado, origem) VALUES (?1,1,?2,'ativa',?3)",
            params![id, serde_json::to_string(passos).unwrap_or_default(), origem.texto()],
        )?;
        Ok(id)
    }

    /// Cria um rascunho: o roteiro existe (para a gravação ter onde registrar a execução), mas
    /// fica inativo, sem agenda, até [`Self::concluir_gravacao`].
    ///
    /// # Errors
    /// Nome inválido ou repetido, ou falha do banco.
    pub fn criar_rascunho(
        &self,
        nome: &str,
        descricao: &str,
        url_inicial: &str,
        rotulo_senha: Option<&str>,
    ) -> Resultado<i64> {
        let id = self.criar_roteiro(
            nome,
            descricao,
            url_inicial,
            rotulo_senha,
            &[],
            OrigemDaVersao::Gravacao,
        )?;
        self.conn
            .execute("UPDATE roteiro SET ativo = 0 WHERE id = ?1", [id])?;
        Ok(id)
    }

    /// Termina a gravação: os passos aceitos viram a versão 1 e o roteiro fica ativo.
    ///
    /// # Errors
    /// Passos inválidos ou falha do banco.
    pub fn concluir_gravacao(&self, roteiro_id: i64, passos: &[Passo]) -> Resultado<()> {
        let (nome, descricao): (String, String) = self.conn.query_row(
            "SELECT nome, descricao FROM roteiro WHERE id = ?1",
            [roteiro_id],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )?;
        Roteiro {
            formato: crate::passo::FORMATO,
            nome,
            descricao,
            passos: passos.to_vec(),
        }
        .validar()
        .map_err(ErroDeRoteiro::Invalido)?;
        self.conn.execute(
            "UPDATE versao SET passos = ?2 WHERE roteiro_id = ?1 AND estado = 'ativa'",
            params![
                roteiro_id,
                serde_json::to_string(passos).unwrap_or_default()
            ],
        )?;
        self.conn
            .execute("UPDATE roteiro SET ativo = 1 WHERE id = ?1", [roteiro_id])?;
        Ok(())
    }

    /// O id de um roteiro pelo nome.
    ///
    /// # Errors
    /// Banco.
    pub fn id_do_roteiro(&self, nome: &str) -> Resultado<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT id FROM roteiro WHERE nome = ?1", [nome], |l| {
                l.get(0)
            })
            .optional()?)
    }

    /// A versão ativa: (id da versão, número, roteiro).
    ///
    /// # Errors
    /// Banco, ou roteiro sem versão ativa.
    pub fn versao_ativa(&self, roteiro_id: i64) -> Resultado<(i64, i64, Roteiro)> {
        let (vid, numero, passos, nome, descricao): (i64, i64, String, String, String) = self
            .conn
            .query_row(
                "SELECT v.id, v.numero, v.passos, r.nome, r.descricao FROM versao v
                   JOIN roteiro r ON r.id = v.roteiro_id
                  WHERE v.roteiro_id = ?1 AND v.estado = 'ativa'",
                [roteiro_id],
                |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?, l.get(4)?)),
            )
            .optional()?
            .ok_or_else(|| ErroDeRoteiro::Invalido("roteiro sem versão ativa".into()))?;
        Ok((
            vid,
            numero,
            Roteiro {
                formato: crate::passo::FORMATO,
                nome,
                descricao,
                passos: passos_de(&passos)?,
            },
        ))
    }

    /// A URL inicial e o rótulo de senha do roteiro.
    ///
    /// # Errors
    /// Banco.
    pub fn dados_do_roteiro(&self, roteiro_id: i64) -> Resultado<(String, Option<String>)> {
        Ok(self.conn.query_row(
            "SELECT url_inicial, rotulo_senha FROM roteiro WHERE id = ?1",
            [roteiro_id],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )?)
    }

    /// Guarda um conserto como proposta. Uma proposta anterior ainda aberta vira rejeitada.
    ///
    /// # Errors
    /// Banco.
    pub fn propor_versao(
        &self,
        roteiro_id: i64,
        passos: &[Passo],
        motivo: &str,
        origem: OrigemDaVersao,
    ) -> Resultado<i64> {
        self.conn.execute(
            "UPDATE versao SET estado = 'rejeitada' WHERE roteiro_id = ?1 AND estado = 'proposta'",
            [roteiro_id],
        )?;
        let numero: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(numero),0)+1 FROM versao WHERE roteiro_id = ?1",
            [roteiro_id],
            |l| l.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO versao (roteiro_id, numero, passos, estado, origem, motivo)
             VALUES (?1,?2,?3,'proposta',?4,?5)",
            params![
                roteiro_id,
                numero,
                serde_json::to_string(passos).unwrap_or_default(),
                origem.texto(),
                motivo
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// A proposta aberta do roteiro, se houver.
    ///
    /// # Errors
    /// Banco.
    pub fn proposta(&self, roteiro_id: i64) -> Resultado<Option<Proposta>> {
        let l: Option<(i64, i64, String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT id, numero, passos, motivo FROM versao
                  WHERE roteiro_id = ?1 AND estado = 'proposta'",
                [roteiro_id],
                |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
            )
            .optional()?;
        l.map(|(versao_id, numero, passos, motivo)| {
            Ok(Proposta {
                versao_id,
                roteiro_id,
                numero,
                passos: passos_de(&passos)?,
                motivo: motivo.unwrap_or_default(),
            })
        })
        .transpose()
    }

    /// O usuário aprovou: a proposta vira a versão ativa e a anterior fica no histórico.
    ///
    /// # Errors
    /// Banco, ou não há proposta.
    pub fn aprovar_proposta(&self, roteiro_id: i64) -> Resultado<()> {
        let p = self
            .proposta(roteiro_id)?
            .ok_or_else(|| ErroDeRoteiro::Invalido("não há conserto para aprovar".into()))?;
        self.conn.execute(
            "UPDATE versao SET estado = 'anterior' WHERE roteiro_id = ?1 AND estado = 'ativa'",
            [roteiro_id],
        )?;
        self.conn.execute(
            "UPDATE versao SET estado = 'ativa' WHERE id = ?1",
            [p.versao_id],
        )?;
        Ok(())
    }

    /// O usuário recusou o conserto.
    ///
    /// # Errors
    /// Banco.
    pub fn rejeitar_proposta(&self, roteiro_id: i64) -> Resultado<()> {
        self.conn.execute(
            "UPDATE versao SET estado = 'rejeitada' WHERE roteiro_id = ?1 AND estado = 'proposta'",
            [roteiro_id],
        )?;
        Ok(())
    }

    /// Liga ou desliga o agendamento (a cada `horas`; `None` = só manual).
    ///
    /// # Errors
    /// Banco.
    pub fn definir_agenda(&self, roteiro_id: i64, horas: Option<i64>) -> Resultado<()> {
        self.conn.execute(
            "UPDATE roteiro SET agenda_horas = ?2 WHERE id = ?1",
            params![roteiro_id, horas.filter(|h| *h > 0)],
        )?;
        Ok(())
    }

    /// Apaga o roteiro com versões, execuções e eventos.
    ///
    /// # Errors
    /// Banco.
    pub fn apagar_roteiro(&self, roteiro_id: i64) -> Resultado<()> {
        self.conn
            .execute("DELETE FROM roteiro WHERE id = ?1", [roteiro_id])?;
        Ok(())
    }

    /// Os roteiros, com a última execução.
    ///
    /// # Errors
    /// Banco.
    pub fn listar(&self) -> Resultado<Vec<LinhaDeRoteiro>> {
        let mut st = self.conn.prepare(
            "SELECT r.id, r.nome, r.descricao, r.url_inicial, r.rotulo_senha, r.agenda_horas, r.ativo,
                    COALESCE(v.numero,0), COALESCE(v.passos,'[]'),
                    EXISTS(SELECT 1 FROM versao p WHERE p.roteiro_id = r.id AND p.estado='proposta')
               FROM roteiro r LEFT JOIN versao v ON v.roteiro_id = r.id AND v.estado='ativa'
              ORDER BY r.nome",
        )?;
        let linhas = st
            .query_map([], |l| {
                Ok((
                    l.get::<_, i64>(0)?,
                    l.get::<_, String>(1)?,
                    l.get::<_, String>(2)?,
                    l.get::<_, String>(3)?,
                    l.get::<_, Option<String>>(4)?,
                    l.get::<_, Option<i64>>(5)?,
                    l.get::<_, i64>(6)?,
                    l.get::<_, i64>(7)?,
                    l.get::<_, String>(8)?,
                    l.get::<_, bool>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut saida = Vec::new();
        for (
            id,
            nome,
            descricao,
            url_inicial,
            rotulo_senha,
            agenda_horas,
            ativo,
            versao,
            passos,
            ha_proposta,
        ) in linhas
        {
            let n = passos_de(&passos).map(|p| p.len()).unwrap_or(0);
            saida.push(LinhaDeRoteiro {
                id,
                nome,
                descricao,
                url_inicial,
                rotulo_senha,
                agenda_horas,
                ativo: ativo != 0,
                versao,
                passos: n,
                ha_proposta,
                ultima: self.historico(Some(id), 1)?.into_iter().next(),
            });
        }
        Ok(saida)
    }

    /// O histórico de execuções, da mais nova para a mais antiga.
    ///
    /// # Errors
    /// Banco.
    pub fn historico(
        &self,
        roteiro_id: Option<i64>,
        limite: usize,
    ) -> Resultado<Vec<LinhaDeExecucao>> {
        let mut st = self.conn.prepare(
            "SELECT e.id, e.roteiro_id, r.nome, v.numero, e.gatilho, e.iniciada_em, e.concluida_em,
                    e.estado, e.passo_atual, e.motivo, e.baixados, e.novos
               FROM execucao e JOIN roteiro r ON r.id = e.roteiro_id JOIN versao v ON v.id = e.versao_id
              WHERE (?1 IS NULL OR e.roteiro_id = ?1)
              ORDER BY e.id DESC LIMIT ?2",
        )?;
        let linhas = st
            .query_map(
                params![roteiro_id, i64::try_from(limite).unwrap_or(50)],
                |l| {
                    Ok(LinhaDeExecucao {
                        id: l.get(0)?,
                        roteiro_id: l.get(1)?,
                        roteiro: l.get(2)?,
                        versao: l.get(3)?,
                        gatilho: l.get(4)?,
                        iniciada_em: l.get(5)?,
                        concluida_em: l.get(6)?,
                        estado: EstadoDaExecucao::de(&l.get::<_, String>(7)?),
                        passo_atual: l.get(8)?,
                        motivo: l.get(9)?,
                        baixados: l.get(10)?,
                        novos: l.get(11)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(linhas)
    }

    /// Começa uma execução, **atomicamente**: se o roteiro já tem uma rodando ou esperando o
    /// usuário (neste ou em outro processo), recusa.
    ///
    /// # Errors
    /// Banco, ou o roteiro já está rodando.
    pub fn iniciar_execucao(
        &self,
        roteiro_id: i64,
        versao_id: i64,
        gatilho: &str,
    ) -> Resultado<i64> {
        // Uma transação que já nasce com a trava de escrita: a conferência e o INSERT não
        // deixam outro processo (a janela, o worker) passar no meio.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let rodando: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM execucao WHERE roteiro_id = ?1
                             AND estado IN ('rodando','aguardando_usuario'))",
            [roteiro_id],
            |l| l.get(0),
        )?;
        if rodando {
            return Err(ErroDeRoteiro::Invalido("o roteiro já está rodando".into()));
        }
        tx.execute(
            "INSERT INTO execucao (roteiro_id, versao_id, gatilho, pid) VALUES (?1,?2,?3,?4)",
            params![roteiro_id, versao_id, gatilho, std::process::id()],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// O passo em que a execução está.
    ///
    /// # Errors
    /// Banco.
    pub fn atualizar_passo(&self, execucao: i64, passo: usize) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao SET passo_atual = ?2 WHERE id = ?1",
            params![execucao, i64::try_from(passo).unwrap_or(0)],
        )?;
        Ok(())
    }

    /// Acrescenta um evento (mensagem sem segredo, já limpa pelo executor).
    ///
    /// # Errors
    /// Banco.
    pub fn evento(
        &self,
        execucao: i64,
        passo: Option<usize>,
        tipo: &str,
        mensagem: &str,
    ) -> Resultado<()> {
        self.conn.execute(
            "INSERT INTO evento (execucao_id, passo, tipo, mensagem) VALUES (?1,?2,?3,?4)",
            params![
                execucao,
                passo.and_then(|p| i64::try_from(p).ok()),
                tipo,
                mensagem.chars().take(400).collect::<String>()
            ],
        )?;
        Ok(())
    }

    /// Os eventos de uma execução.
    ///
    /// # Errors
    /// Banco.
    pub fn eventos(&self, execucao: i64) -> Resultado<Vec<Evento>> {
        let mut st = self.conn.prepare(
            "SELECT quando, passo, tipo, mensagem FROM evento WHERE execucao_id = ?1 ORDER BY id",
        )?;
        Ok(st
            .query_map([execucao], |l| {
                Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Fecha a execução.
    ///
    /// # Errors
    /// Banco.
    pub fn finalizar(
        &self,
        execucao: i64,
        estado: EstadoDaExecucao,
        motivo: Option<&str>,
        baixados: u32,
        novos: u32,
    ) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao SET estado = ?2, motivo = ?3, baixados = ?4, novos = ?5,
               concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![execucao, estado.texto(), motivo, baixados, novos],
        )?;
        Ok(())
    }

    /// A execução espera o usuário (a janela mostra "Sua vez no navegador").
    ///
    /// # Errors
    /// Banco.
    pub fn marcar_aguardando(&self, execucao: i64, motivo: &str) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao SET estado = 'aguardando_usuario', motivo = ?2, continuar = 0 WHERE id = ?1",
            params![execucao, motivo],
        )?;
        Ok(())
    }

    /// A execução voltou a rodar.
    ///
    /// # Errors
    /// Banco.
    pub fn marcar_rodando(&self, execucao: i64) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao SET estado = 'rodando', motivo = NULL, continuar = 0 WHERE id = ?1",
            [execucao],
        )?;
        Ok(())
    }

    /// A janela pede: `true` continua, `false` cancela.
    ///
    /// # Errors
    /// Banco.
    pub fn pedir_continuar(&self, execucao: i64, continuar: bool) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao SET continuar = ?2 WHERE id = ?1 AND estado = 'aguardando_usuario'",
            params![execucao, if continuar { 1 } else { 2 }],
        )?;
        Ok(())
    }

    /// O que a janela pediu: `0` nada, `1` continuar, `2` cancelar.
    ///
    /// # Errors
    /// Banco.
    pub fn pedido_do_usuario(&self, execucao: i64) -> Resultado<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT continuar FROM execucao WHERE id = ?1",
                [execucao],
                |l| l.get(0),
            )
            .optional()?
            .unwrap_or(2))
    }

    /// As execuções que esperam o usuário.
    ///
    /// # Errors
    /// Banco.
    pub fn aguardando(&self) -> Resultado<Vec<LinhaDeExecucao>> {
        Ok(self
            .historico(None, 200)?
            .into_iter()
            .filter(|e| e.estado == EstadoDaExecucao::AguardandoUsuario)
            .collect())
    }

    /// Marca como falhas as execuções que ficaram `rodando` ou `aguardando` de um processo que
    /// já não existe (o worker caiu no meio). No Linux confere `/proc/<pid>`; nos outros
    /// sistemas, só as de mais de 6 horas.
    ///
    /// # Errors
    /// Banco.
    pub fn encerrar_orfas(&self) -> Resultado<usize> {
        let mut st = self.conn.prepare(
            "SELECT id, pid, iniciada_unix, strftime('%s','now') FROM execucao
              WHERE estado IN ('rodando','aguardando_usuario')",
        )?;
        let linhas = st
            .query_map([], |l| {
                Ok((
                    l.get::<_, i64>(0)?,
                    l.get::<_, Option<i64>>(1)?,
                    l.get::<_, i64>(2)?,
                    l.get::<_, String>(3)?.parse::<i64>().unwrap_or(0),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut n = 0;
        for (id, pid, inicio, agora) in linhas {
            let viva = match pid {
                #[cfg(target_os = "linux")]
                Some(p) => std::path::Path::new(&format!("/proc/{p}")).exists(),
                _ => agora - inicio < 6 * 3600,
            };
            if !viva {
                self.finalizar(
                    id,
                    EstadoDaExecucao::Falhou,
                    Some("interrompida: o processo parou antes do fim"),
                    0,
                    0,
                )?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// O roteiro tem uma execução rodando ou esperando o usuário?
    ///
    /// # Errors
    /// Banco.
    pub fn em_execucao(&self, roteiro_id: i64) -> Resultado<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM execucao WHERE roteiro_id = ?1
                             AND estado IN ('rodando','aguardando_usuario'))",
            [roteiro_id],
            |l| l.get(0),
        )?)
    }

    /// Roteiros agendados que já passaram do intervalo desde a última execução (e que não
    /// estão rodando).
    ///
    /// # Errors
    /// Banco.
    pub fn devidos(&self, agora_unix: i64) -> Resultado<Vec<i64>> {
        let mut st = self.conn.prepare(
            "SELECT r.id FROM roteiro r
              WHERE r.ativo = 1 AND r.agenda_horas IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM execucao e WHERE e.roteiro_id = r.id
                                  AND e.estado IN ('rodando','aguardando_usuario'))
                AND COALESCE((SELECT MAX(e.iniciada_unix) FROM execucao e WHERE e.roteiro_id = r.id), 0)
                      + r.agenda_horas * 3600 <= ?1
              ORDER BY r.id",
        )?;
        Ok(st
            .query_map([agora_unix], |l| l.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
}

// ---------------------------------------------------------------------------------------
// Roteiro escrito (Luau): versões imutáveis, estados e histórico
// ---------------------------------------------------------------------------------------

/// O estado de uma versão do roteiro escrito. O caminho é `rascunho → testado → ativo →
/// aposentado`; nunca se volta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoEscrito {
    Rascunho,
    Testado,
    Ativo,
    Aposentado,
}

impl EstadoEscrito {
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            Self::Rascunho => "rascunho",
            Self::Testado => "testado",
            Self::Ativo => "ativo",
            Self::Aposentado => "aposentado",
        }
    }
    fn de(t: &str) -> Self {
        match t {
            "testado" => Self::Testado,
            "ativo" => Self::Ativo,
            "aposentado" => Self::Aposentado,
            _ => Self::Rascunho,
        }
    }
}

/// Quem escreveu a versão. Só quem importa; **ativar depende da [`Confirmacao`]**, e o texto
/// que o usuário digita depende do autor: quem não é o próprio usuário (`mcp`, `ia:*`) obriga a
/// digitar também o host principal (ver [`BancoDeRoteiros::esperado_para_ativar`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutorEscrito {
    Usuario,
    Mcp,
    /// O modelo que escreveu (`ia:<modelo>`).
    Ia(String),
}

impl AutorEscrito {
    /// `usuario`, `mcp` ou `ia:<modelo>`.
    #[must_use]
    pub fn texto(&self) -> String {
        match self {
            Self::Usuario => "usuario".to_owned(),
            Self::Mcp => "mcp".to_owned(),
            Self::Ia(m) => format!("ia:{m}"),
        }
    }
    fn de(t: &str) -> Self {
        match t {
            "usuario" => Self::Usuario,
            "mcp" => Self::Mcp,
            outro => Self::Ia(outro.strip_prefix("ia:").unwrap_or(outro).to_owned()),
        }
    }
    fn validar(&self) -> Resultado<()> {
        if let Self::Ia(m) = self
            && (m.is_empty()
                || m.len() > 80
                || !m
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':')))
        {
            return Err(ErroDeRoteiro::Invalido(
                "o modelo do autor `ia:<modelo>` tem de ter de 1 a 80 letras, dígitos, `-`, `_`, `.` ou `:`"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// A prova de que o **usuário** digitou a confirmação pedida. É o único jeito de chamar
/// [`BancoDeRoteiros::ativar`] e [`BancoDeRoteiros::desativar`]. Quem a constrói são o terminal
/// interativo (`sbf roteiros escrito ativar`, [`Confirmacao::do_terminal`]) e a janela do app
/// ([`Confirmacao::da_janela`]); o MCP e a IA não a constroem.
///
/// Atenção: isto é **autoridade por convenção** (os construtores são `pub`): o que impede o
/// MCP e a IA de ativar não é o sistema de tipos, é o teste de arquitetura
/// `crates/sbf-roteiros/tests/autoridade.rs`, que falha se `sbf-mcp`, `sbf-worker` ou `sbf-ia`
/// citarem `Confirmacao`, `da_janela`, `do_terminal`, `marcar_testado`, `.ativar(` ou
/// `.desativar(`.
#[derive(Debug)]
pub struct Confirmacao {
    esperado: String,
}

impl Confirmacao {
    /// `<roteiro>@<versao>`: o texto de ativar de uma versão escrita **pelo usuário**. Não use
    /// para decidir o que pedir: a fonte do texto a digitar é
    /// [`BancoDeRoteiros::esperado_para_ativar`], que acrescenta o host quando o autor é o MCP
    /// ou a IA. Este é só o miolo dele.
    #[must_use]
    pub fn texto_de_ativar(roteiro: &str, versao: u32) -> String {
        format!("{roteiro}@{versao}")
    }

    /// O nome do roteiro: o texto de desativar. A fonte é
    /// [`BancoDeRoteiros::esperado_para_desativar`].
    #[must_use]
    pub fn texto_de_desativar(roteiro: &str) -> String {
        roteiro.to_owned()
    }

    /// Confere o terminal e a digitação.
    ///
    /// # Errors
    /// Sem terminal interativo, ou digitação diferente do esperado.
    pub fn do_terminal(
        terminal_interativo: bool,
        digitado: &str,
        esperado: &str,
    ) -> Resultado<Self> {
        if !terminal_interativo {
            return Err(ErroDeRoteiro::Invalido(
                "ativar e desativar só num terminal interativo, com confirmação digitada; o MCP e a IA não ativam. Nada foi alterado."
                    .into(),
            ));
        }
        if digitado != esperado {
            return Err(ErroDeRoteiro::Invalido(
                "confirmação diferente do pedido; nada foi alterado.".into(),
            ));
        }
        Ok(Self {
            esperado: esperado.to_owned(),
        })
    }

    /// Confere a digitação feita na janela do app (o mesmo caminho humano das regras de
    /// reconhecimento, ADR-021). Só a janela do SBF a usa; o MCP e a IA nunca constroem uma
    /// `Confirmacao` (o teste `tests/autoridade.rs` falha se esses crates a citarem).
    ///
    /// # Errors
    /// Digitação diferente do esperado.
    pub fn da_janela(digitado: &str, esperado: &str) -> Resultado<Self> {
        if digitado != esperado {
            return Err(ErroDeRoteiro::Invalido(
                "confirmação diferente do pedido; nada foi alterado.".into(),
            ));
        }
        Ok(Self {
            esperado: esperado.to_owned(),
        })
    }
}

/// O resultado de um teste no site falso, entregue por quem executa (outra frente).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoDoTeste {
    pub passou: bool,
    /// Texto curto e **sem segredo** (o motor já limpa).
    pub resumo: String,
}

/// Uma versão do roteiro escrito. Nunca muda depois de criada, a não ser o estado e o
/// resultado do teste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersaoEscrita {
    pub roteiro: String,
    pub numero: u32,
    pub manifesto: Manifesto,
    pub fonte: String,
    pub checksum: String,
    pub estado: EstadoEscrito,
    pub autor: AutorEscrito,
    pub motivo: String,
    pub criada_em: String,
    pub resultado_do_teste: Option<String>,
}

/// O que o `importar_escrito` fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Importacao {
    pub numero: u32,
    /// `false` quando o par manifesto+fonte já era a versão mais nova e nada foi criado.
    pub criada: bool,
}

/// Uma linha da lista de roteiros escritos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeEscrito {
    pub nome: String,
    pub descricao: String,
    /// A versão em vigor, se houver.
    pub ativa: Option<u32>,
    pub ultima: u32,
    pub estado_da_ultima: EstadoEscrito,
}

/// Uma mudança de estado, para o histórico: (quando, versão, de, para, quem, detalhe).
pub type EventoEscrito = (String, u32, Option<String>, String, String, String);

/// O roteiro escrito inteiro: versões e histórico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetalheDoEscrito {
    pub nome: String,
    pub versoes: Vec<VersaoEscrita>,
    pub eventos: Vec<EventoEscrito>,
}

fn invalido_escrito(m: impl Into<String>) -> ErroDeRoteiro {
    ErroDeRoteiro::Invalido(m.into())
}

/// `i64` do SQLite para `u32` (satura em vez de estourar). Também usado por
/// `execucao_escrita.rs`, que ainda tem uma cópia local a remover.
pub(crate) fn u32_de(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl BancoDeRoteiros {
    fn id_do_escrito(&self, nome: &str) -> Resultado<i64> {
        self.conn
            .query_row(
                "SELECT id FROM roteiro_escrito WHERE nome = ?1",
                [nome],
                |l| l.get(0),
            )
            .optional()?
            .ok_or_else(|| invalido_escrito(format!("roteiro escrito `{nome}` não encontrado")))
    }

    /// Importa uma versão. **Sempre cria RASCUNHO**, nunca ativa. O mesmo par manifesto+fonte
    /// (mesmo checksum) que já é a versão mais nova devolve essa versão sem duplicar; se a mais
    /// nova com o mesmo conteúdo está aposentada, nasce uma versão nova (assim dá para
    /// reativar, depois de testar de novo, algo que foi desativado).
    ///
    /// # Errors
    /// Manifesto inválido, fonte vazia ou maior que 64 KiB, autor inválido ou falha do banco.
    pub fn importar_escrito(
        &self,
        manifesto: &Manifesto,
        fonte: &str,
        autor: &AutorEscrito,
        motivo: &str,
    ) -> Resultado<Importacao> {
        manifesto.validar()?;
        autor.validar()?;
        if fonte.trim().is_empty() {
            return Err(invalido_escrito("o roteiro escrito está vazio"));
        }
        if fonte.len() > LIMITE_DE_BYTES {
            return Err(invalido_escrito("o fonte do roteiro passa de 64 KiB"));
        }
        if tem_caractere_perigoso(fonte, true) {
            return Err(invalido_escrito(
                "o fonte do roteiro tem caractere de controle (fora quebra de linha `\\n` e tabulação) ou de direção do texto; salve o arquivo com fim de linha LF",
            ));
        }
        if motivo.chars().count() > 500 || tem_caractere_perigoso(motivo, false) {
            return Err(invalido_escrito(
                "o motivo passa de 500 caracteres ou tem caractere de controle ou de direção",
            ));
        }
        let checksum = manifesto.checksum_com(fonte)?;
        let json = manifesto.json_canonico()?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO roteiro_escrito (nome) VALUES (?1)",
            [&manifesto.nome],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM roteiro_escrito WHERE nome = ?1",
            [&manifesto.nome],
            |l| l.get(0),
        )?;
        let mais_nova: Option<(i64, String, String)> = tx
            .query_row(
                "SELECT numero, checksum, estado FROM roteiro_escrito_versao
                 WHERE roteiro_id = ?1 ORDER BY numero DESC LIMIT 1",
                [id],
                |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
            )
            .optional()?;
        let ultimo = mais_nova.as_ref().map_or(0, |m| m.0);
        if let Some((n, sum, estado)) = &mais_nova
            && *sum == checksum
            && estado != "aposentado"
        {
            return Ok(Importacao {
                numero: u32_de(*n),
                criada: false,
            });
        }
        let numero = ultimo + 1;
        tx.execute(
            "INSERT INTO roteiro_escrito_versao
               (roteiro_id, numero, manifesto, fonte, checksum, estado, autor, motivo)
             VALUES (?1,?2,?3,?4,?5,'rascunho',?6,?7)",
            params![id, numero, json, fonte, checksum, autor.texto(), motivo],
        )?;
        let versao_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
             VALUES (?1,NULL,'rascunho',?2,?3)",
            params![versao_id, autor.texto(), motivo],
        )?;
        tx.commit()?;
        Ok(Importacao {
            numero: u32_de(numero),
            criada: true,
        })
    }

    fn ler_versao(&self, nome: &str, numero: u32) -> Resultado<(i64, VersaoEscrita)> {
        let linha = self
            .conn
            .query_row(
                "SELECT v.id, v.numero, v.manifesto, v.fonte, v.checksum, v.estado, v.autor,
                        COALESCE(v.motivo,''), v.criada_em, v.resultado_do_teste
                 FROM roteiro_escrito_versao v JOIN roteiro_escrito r ON r.id = v.roteiro_id
                 WHERE r.nome = ?1 AND v.numero = ?2",
                params![nome, numero],
                |l| {
                    Ok((
                        l.get::<_, i64>(0)?,
                        l.get::<_, i64>(1)?,
                        l.get::<_, String>(2)?,
                        l.get::<_, String>(3)?,
                        l.get::<_, String>(4)?,
                        l.get::<_, String>(5)?,
                        l.get::<_, String>(6)?,
                        l.get::<_, String>(7)?,
                        l.get::<_, String>(8)?,
                        l.get::<_, Option<String>>(9)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                invalido_escrito(format!(
                    "roteiro escrito `{nome}` não tem a versão {numero}"
                ))
            })?;
        let (id, num, manifesto, fonte, checksum, estado, autor, motivo, criada_em, teste) = linha;
        let manifesto: Manifesto = serde_json::from_str(&manifesto)
            .map_err(|_| invalido_escrito("o manifesto guardado não lê"))?;
        Ok((
            id,
            VersaoEscrita {
                roteiro: nome.to_owned(),
                numero: u32_de(num),
                manifesto,
                fonte,
                checksum,
                estado: EstadoEscrito::de(&estado),
                autor: AutorEscrito::de(&autor),
                motivo,
                criada_em,
                resultado_do_teste: teste,
            },
        ))
    }

    /// Uma versão do roteiro escrito.
    ///
    /// # Errors
    /// Roteiro ou versão inexistente, ou falha do banco.
    pub fn versao_escrita(&self, nome: &str, numero: u32) -> Resultado<VersaoEscrita> {
        Ok(self.ler_versao(nome, numero)?.1)
    }

    /// A versão em vigor do roteiro escrito, se houver (é a que o executor e a agenda usam).
    ///
    /// # Errors
    /// Falha do banco.
    pub fn versao_ativa_escrita(&self, nome: &str) -> Resultado<Option<VersaoEscrita>> {
        let numero: Option<i64> = self
            .conn
            .query_row(
                "SELECT v.numero FROM roteiro_escrito_versao v
                 JOIN roteiro_escrito r ON r.id = v.roteiro_id
                 WHERE r.nome = ?1 AND v.estado = 'ativo'",
                [nome],
                |l| l.get(0),
            )
            .optional()?;
        numero
            .map(|n| self.versao_escrita(nome, u32_de(n)))
            .transpose()
    }

    /// Registra o resultado do teste. **Só de rascunho.** Se passou, a versão vira `testado`;
    /// se falhou, continua rascunho (o resumo fica no histórico).
    ///
    /// É `pub` só porque os testes de integração do crate precisam dela. **Em produção só
    /// `testar_escrito` (`teste_escrito.rs`), que de fato roda o roteiro no site falso, a
    /// chama**; nem o MCP, nem o worker, nem a IA podem citá-la (`tests/autoridade.rs`).
    ///
    /// # Errors
    /// Versão inexistente ou que não é rascunho.
    pub fn marcar_testado(
        &self,
        nome: &str,
        numero: u32,
        resultado: &ResultadoDoTeste,
    ) -> Resultado<EstadoEscrito> {
        let (id, v) = self.ler_versao(nome, numero)?;
        if v.estado != EstadoEscrito::Rascunho {
            return Err(invalido_escrito(format!(
                "só um rascunho pode ser testado; a versão {numero} está {}",
                v.estado.texto()
            )));
        }
        let resumo: String = resultado.resumo.chars().take(500).collect();
        let novo = if resultado.passou {
            EstadoEscrito::Testado
        } else {
            EstadoEscrito::Rascunho
        };
        let texto = format!(
            "{}: {resumo}",
            if resultado.passou { "passou" } else { "falhou" }
        );
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE roteiro_escrito_versao SET estado = ?1, resultado_do_teste = ?2 WHERE id = ?3",
            params![novo.texto(), texto, id],
        )?;
        tx.execute(
            "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
             VALUES (?1,'rascunho',?2,'teste',?3)",
            params![id, novo.texto(), texto],
        )?;
        tx.commit()?;
        Ok(novo)
    }

    /// O texto que o usuário tem de digitar para ativar `nome` v`numero`. É a **única fonte**
    /// desse texto: o terminal, a janela e [`BancoDeRoteiros::ativar`] usam esta função.
    ///
    /// - autor `usuario`: `<roteiro>@<versao>`;
    /// - autor `mcp` ou `ia:*`: `<roteiro>@<versao> <host-principal>` (o primeiro de `hosts`),
    ///   para que quem ativa algo proposto por outro seja obrigado a LER o host que vai receber
    ///   a senha do chaveiro; `rotulo_senha` e `hosts` são autodeclarados pelo autor.
    ///
    /// # Errors
    /// Roteiro ou versão inexistente.
    pub fn esperado_para_ativar(&self, nome: &str, numero: u32) -> Resultado<String> {
        let v = self.versao_escrita(nome, numero)?;
        let base = Confirmacao::texto_de_ativar(nome, numero);
        Ok(match v.autor {
            AutorEscrito::Usuario => base,
            AutorEscrito::Mcp | AutorEscrito::Ia(_) => {
                // `hosts` nunca é vazio (o manifesto guardado passou em `validar`)
                let host = v.manifesto.hosts.first().map_or("", String::as_str);
                format!("{base} {host}")
            }
        })
    }

    /// O texto que o usuário tem de digitar para desativar `nome`: o nome do roteiro. Desativar
    /// só para a execução, então não pede o host; é a única fonte desse texto.
    #[must_use]
    pub fn esperado_para_desativar(&self, nome: &str) -> String {
        Confirmacao::texto_de_desativar(nome)
    }

    /// Ativa uma versão `testado` e aposenta a ativa anterior, numa transação. A
    /// [`Confirmacao`] tem de ser a de [`BancoDeRoteiros::esperado_para_ativar`].
    ///
    /// # Errors
    /// Confirmação de outro pedido, versão que não está `testado`, ou falha do banco.
    pub fn ativar(&self, nome: &str, numero: u32, confirmacao: &Confirmacao) -> Resultado<()> {
        if confirmacao.esperado != self.esperado_para_ativar(nome, numero)? {
            return Err(invalido_escrito("a confirmação é de outro pedido"));
        }
        let (id, v) = self.ler_versao(nome, numero)?;
        if v.estado != EstadoEscrito::Testado {
            return Err(invalido_escrito(format!(
                "só uma versão testada pode ser ativada; a versão {numero} está {}",
                v.estado.texto()
            )));
        }
        let roteiro_id = self.id_do_escrito(nome)?;
        let tx = self.conn.unchecked_transaction()?;
        let anterior: Option<i64> = tx
            .query_row(
                "SELECT id FROM roteiro_escrito_versao WHERE roteiro_id = ?1 AND estado = 'ativo'",
                [roteiro_id],
                |l| l.get(0),
            )
            .optional()?;
        if let Some(a) = anterior {
            tx.execute(
                "UPDATE roteiro_escrito_versao SET estado = 'aposentado' WHERE id = ?1",
                [a],
            )?;
            tx.execute(
                "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
                 VALUES (?1,'ativo','aposentado','usuario',?2)",
                params![a, format!("substituída pela versão {numero}")],
            )?;
        }
        tx.execute(
            "UPDATE roteiro_escrito_versao SET estado = 'ativo' WHERE id = ?1",
            [id],
        )?;
        tx.execute(
            "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
             VALUES (?1,'testado','ativo','usuario','ativada com confirmação digitada')",
            [id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Aposenta a versão ativa: o roteiro fica sem versão em vigor (a agenda para). Devolve o
    /// número da versão aposentada.
    ///
    /// # Errors
    /// Confirmação de outro pedido, roteiro inexistente ou nenhuma versão ativa.
    pub fn desativar(&self, nome: &str, confirmacao: &Confirmacao) -> Resultado<u32> {
        if confirmacao.esperado != self.esperado_para_desativar(nome) {
            return Err(invalido_escrito("a confirmação é de outro pedido"));
        }
        self.id_do_escrito(nome)?;
        let Some(v) = self.versao_ativa_escrita(nome)? else {
            return Err(invalido_escrito(format!(
                "roteiro escrito `{nome}` não tem versão ativa"
            )));
        };
        let (id, _) = self.ler_versao(nome, v.numero)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE roteiro_escrito_versao SET estado = 'aposentado' WHERE id = ?1",
            [id],
        )?;
        tx.execute(
            "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
             VALUES (?1,'ativo','aposentado','usuario','desativada com confirmação digitada')",
            [id],
        )?;
        tx.commit()?;
        Ok(v.numero)
    }

    /// Os roteiros escritos, por nome.
    ///
    /// # Errors
    /// Falha do banco.
    pub fn listar_escritos(&self) -> Resultado<Vec<LinhaDeEscrito>> {
        let mut st = self.conn.prepare(
            "SELECT r.nome,
                    (SELECT manifesto FROM roteiro_escrito_versao v
                       WHERE v.roteiro_id = r.id ORDER BY v.numero DESC LIMIT 1),
                    (SELECT numero FROM roteiro_escrito_versao v
                       WHERE v.roteiro_id = r.id AND v.estado = 'ativo'),
                    (SELECT numero FROM roteiro_escrito_versao v
                       WHERE v.roteiro_id = r.id ORDER BY v.numero DESC LIMIT 1),
                    (SELECT estado FROM roteiro_escrito_versao v
                       WHERE v.roteiro_id = r.id ORDER BY v.numero DESC LIMIT 1)
             FROM roteiro_escrito r ORDER BY r.nome",
        )?;
        let linhas = st
            .query_map([], |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    l.get::<_, Option<String>>(1)?,
                    l.get::<_, Option<i64>>(2)?,
                    l.get::<_, Option<i64>>(3)?,
                    l.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(linhas
            .into_iter()
            .map(|(nome, manifesto, ativa, ultima, estado)| LinhaDeEscrito {
                descricao: manifesto
                    .and_then(|m| serde_json::from_str::<Manifesto>(&m).ok())
                    .map(|m| m.descricao)
                    .unwrap_or_default(),
                nome,
                ativa: ativa.map(u32_de),
                ultima: u32_de(ultima.unwrap_or(0)),
                estado_da_ultima: EstadoEscrito::de(estado.as_deref().unwrap_or("rascunho")),
            })
            .collect())
    }

    /// O roteiro escrito com todas as versões e o histórico de estados.
    ///
    /// # Errors
    /// Roteiro inexistente ou falha do banco.
    pub fn mostrar(&self, nome: &str) -> Resultado<DetalheDoEscrito> {
        let id = self.id_do_escrito(nome)?;
        let numeros: Vec<i64> = {
            let mut st = self.conn.prepare(
                "SELECT numero FROM roteiro_escrito_versao WHERE roteiro_id = ?1 ORDER BY numero",
            )?;
            st.query_map([id], |l| l.get(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut versoes = Vec::new();
        for n in numeros {
            versoes.push(self.versao_escrita(nome, u32_de(n))?);
        }
        let mut st = self.conn.prepare(
            "SELECT e.quando, v.numero, e.de, e.para, e.quem, e.detalhe
             FROM roteiro_escrito_evento e JOIN roteiro_escrito_versao v ON v.id = e.versao_id
             WHERE v.roteiro_id = ?1 ORDER BY e.id",
        )?;
        let eventos = st
            .query_map([id], |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    u32_de(l.get::<_, i64>(1)?),
                    l.get::<_, Option<String>>(2)?,
                    l.get::<_, String>(3)?,
                    l.get::<_, String>(4)?,
                    l.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DetalheDoEscrito {
            nome: nome.to_owned(),
            versoes,
            eventos,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn passos() -> Vec<Passo> {
        vec![Passo::Navegar {
            url: "http://127.0.0.1:1/".into(),
        }]
    }

    #[test]
    fn criar_listar_proposta_aprovar() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b
            .criar_roteiro(
                "extratos",
                "d",
                "http://127.0.0.1:1/",
                Some("banco"),
                &passos(),
                OrigemDaVersao::Gravacao,
            )
            .unwrap();
        assert!(
            b.criar_roteiro("extratos", "", "", None, &passos(), OrigemDaVersao::Manual)
                .is_err()
        );
        let l = b.listar().unwrap();
        assert_eq!(
            (l.len(), l[0].versao, l[0].passos, l[0].ha_proposta),
            (1, 1, 1, false)
        );
        let novos = vec![Passo::Navegar {
            url: "http://127.0.0.1:1/novo".into(),
        }];
        b.propor_versao(id, &novos, "o site mudou", OrigemDaVersao::Conserto)
            .unwrap();
        // proposta não vale: a versão ativa segue a mesma
        assert_eq!(b.versao_ativa(id).unwrap().2.passos, passos());
        assert!(b.listar().unwrap()[0].ha_proposta);
        b.aprovar_proposta(id).unwrap();
        let (_, numero, r) = b.versao_ativa(id).unwrap();
        assert_eq!((numero, r.passos), (2, novos));
        assert!(b.proposta(id).unwrap().is_none());
    }

    #[test]
    fn rejeitar_deixa_a_ativa() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b
            .criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual)
            .unwrap();
        b.propor_versao(
            id,
            &[Passo::Clicar {
                seletor: "#a".into(),
            }],
            "m",
            OrigemDaVersao::Conserto,
        )
        .unwrap();
        b.rejeitar_proposta(id).unwrap();
        assert!(b.proposta(id).unwrap().is_none());
        assert_eq!(b.versao_ativa(id).unwrap().1, 1);
    }

    #[test]
    fn agenda_so_dispara_depois_do_intervalo() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b
            .criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual)
            .unwrap();
        assert!(
            b.devidos(1_000_000).unwrap().is_empty(),
            "sem agenda não é devido"
        );
        b.definir_agenda(id, Some(24)).unwrap();
        assert_eq!(b.devidos(1_000_000).unwrap(), vec![id], "nunca rodou");
        let v = b.versao_ativa(id).unwrap().0;
        let e = b.iniciar_execucao(id, v, "agenda").unwrap();
        assert!(
            b.devidos(i64::MAX / 4).unwrap().is_empty(),
            "rodando não repete"
        );
        b.finalizar(e, EstadoDaExecucao::Concluida, None, 0, 0)
            .unwrap();
        let agora: i64 = b
            .conn
            .query_row("SELECT strftime('%s','now')", [], |l| l.get::<_, String>(0))
            .unwrap()
            .parse()
            .unwrap();
        assert!(b.devidos(agora + 3600).unwrap().is_empty());
        assert_eq!(b.devidos(agora + 25 * 3600).unwrap(), vec![id]);
    }

    #[test]
    fn so_uma_execucao_comeca_mesmo_com_processos_concorrentes() {
        let d = tempfile::tempdir().unwrap();
        let arquivo = d.path().join("roteiros.sqlite");
        let (id, v) = {
            let b = BancoDeRoteiros::abrir(&arquivo).unwrap();
            let id = b
                .criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual)
                .unwrap();
            (id, b.versao_ativa(id).unwrap().0)
        };
        let largada = std::sync::Arc::new(std::sync::Barrier::new(8));
        let fios: Vec<_> = (0..8)
            .map(|_| {
                let (a, l) = (arquivo.clone(), std::sync::Arc::clone(&largada));
                std::thread::spawn(move || {
                    let b = BancoDeRoteiros::abrir(&a).unwrap();
                    l.wait();
                    b.iniciar_execucao(id, v, "agenda").is_ok()
                })
            })
            .collect();
        let comecaram = fios
            .into_iter()
            .map(|f| f.join().unwrap())
            .filter(|ok| *ok)
            .count();
        assert_eq!(comecaram, 1, "só uma execução do mesmo roteiro por vez");
        // acabou a rodando: pode começar outra
        let b = BancoDeRoteiros::abrir(&arquivo).unwrap();
        let rodando: i64 = b
            .conn
            .query_row(
                "SELECT id FROM execucao WHERE estado = 'rodando'",
                [],
                |l| l.get(0),
            )
            .unwrap();
        b.finalizar(rodando, EstadoDaExecucao::Concluida, None, 0, 0)
            .unwrap();
        assert!(b.iniciar_execucao(id, v, "agenda").is_ok());
    }

    #[test]
    fn pausa_pelo_banco() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b
            .criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual)
            .unwrap();
        let v = b.versao_ativa(id).unwrap().0;
        let e = b.iniciar_execucao(id, v, "janela").unwrap();
        b.pedir_continuar(e, true).unwrap();
        assert_eq!(b.pedido_do_usuario(e).unwrap(), 0, "só vale aguardando");
        b.marcar_aguardando(e, "mfa").unwrap();
        assert_eq!(b.aguardando().unwrap().len(), 1);
        b.pedir_continuar(e, true).unwrap();
        assert_eq!(b.pedido_do_usuario(e).unwrap(), 1);
        b.marcar_rodando(e).unwrap();
        assert_eq!(b.pedido_do_usuario(e).unwrap(), 0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes_escrito {
    use super::*;
    use crate::manifesto::{Classe, Mfa};

    fn manifesto(nome: &str) -> Manifesto {
        Manifesto {
            nome: nome.into(),
            descricao: "Contracheque do mês".into(),
            url_inicial: "https://portal.exemplo.test/entrar".into(),
            hosts: vec!["portal.exemplo.test".into()],
            rotulo_senha: Some("senha:portal-exemplo".into()),
            rotulo_cpf: None,
            mfa: Mfa::Nenhum,
            movimenta_dinheiro: false,
            classe: Classe::Consulta,
            agenda_horas: Some(168),
        }
    }

    const FONTE: &str = "navegador.abrir('https://portal.exemplo.test/entrar')";

    fn passou() -> ResultadoDoTeste {
        ResultadoDoTeste {
            passou: true,
            resumo: "site falso ok".into(),
        }
    }

    fn conf_ativar(nome: &str, v: u32) -> Confirmacao {
        Confirmacao::do_terminal(true, &format!("{nome}@{v}"), &format!("{nome}@{v}")).unwrap()
    }

    fn conf_desativar(nome: &str) -> Confirmacao {
        Confirmacao::do_terminal(true, nome, nome).unwrap()
    }

    fn importar(b: &BancoDeRoteiros, fonte: &str) -> Importacao {
        b.importar_escrito(
            &manifesto("contracheque"),
            fonte,
            &AutorEscrito::Usuario,
            "m",
        )
        .unwrap()
    }

    /// Importa, testa e ativa a versão.
    fn ativa(b: &BancoDeRoteiros, fonte: &str) -> u32 {
        let n = importar(b, fonte).numero;
        b.marcar_testado("contracheque", n, &passou()).unwrap();
        b.ativar("contracheque", n, &conf_ativar("contracheque", n))
            .unwrap();
        n
    }

    #[test]
    fn importar_cria_rascunho_numerado_e_nunca_ativa() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let i = importar(&b, FONTE);
        assert_eq!(
            i,
            Importacao {
                numero: 1,
                criada: true
            }
        );
        let v = b.versao_escrita("contracheque", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Rascunho);
        assert_eq!(v.autor, AutorEscrito::Usuario);
        assert_eq!(v.fonte, FONTE);
        assert_eq!(v.checksum.len(), 64);
        assert_eq!(v.motivo, "m");
        assert!(b.versao_ativa_escrita("contracheque").unwrap().is_none());
        let outra = importar(&b, "navegador.registrar('v2')");
        assert_eq!(outra.numero, 2);
    }

    #[test]
    fn mesma_checksum_da_mais_nova_devolve_a_mesma_versao() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        importar(&b, FONTE);
        let de_novo = importar(&b, FONTE);
        assert_eq!(
            de_novo,
            Importacao {
                numero: 1,
                criada: false
            }
        );
        assert_eq!(b.mostrar("contracheque").unwrap().versoes.len(), 1);
        // a mudança do manifesto também é conteúdo
        let mut m = manifesto("contracheque");
        m.descricao = "outra".into();
        let i = b
            .importar_escrito(&m, FONTE, &AutorEscrito::Mcp, "muda a descrição")
            .unwrap();
        assert_eq!(
            i,
            Importacao {
                numero: 2,
                criada: true
            }
        );
        // v1 -> v2 -> a v1 de novo: a mais nova é a v2, então o conteúdo da v1 vira a v3
        let volta = importar(&b, FONTE);
        assert_eq!(
            volta,
            Importacao {
                numero: 3,
                criada: true
            }
        );
    }

    #[test]
    fn reimportar_algo_aposentado_cria_versao_nova() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let n = ativa(&b, FONTE);
        b.desativar("contracheque", &conf_desativar("contracheque"))
            .unwrap();
        assert_eq!(
            b.versao_escrita("contracheque", n).unwrap().estado,
            EstadoEscrito::Aposentado
        );
        let i = importar(&b, FONTE);
        assert_eq!(
            i,
            Importacao {
                numero: 2,
                criada: true
            }
        );
    }

    #[test]
    fn importar_valida_manifesto_fonte_e_autor() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let mut ruim = manifesto("contracheque");
        ruim.hosts.clear();
        assert!(
            b.importar_escrito(&ruim, FONTE, &AutorEscrito::Usuario, "")
                .is_err()
        );
        let m = manifesto("contracheque");
        for fonte in ["", "   \n"] {
            assert!(
                b.importar_escrito(&m, fonte, &AutorEscrito::Usuario, "")
                    .is_err()
            );
        }
        let grande = "x".repeat(LIMITE_DE_BYTES + 1);
        assert!(
            b.importar_escrito(&m, &grande, &AutorEscrito::Usuario, "")
                .is_err()
        );
        assert!(
            b.importar_escrito(&m, FONTE, &AutorEscrito::Ia(String::new()), "")
                .is_err()
        );
        assert!(
            b.importar_escrito(&m, FONTE, &AutorEscrito::Ia("a b".into()), "")
                .is_err()
        );
        assert!(
            b.importar_escrito(&m, FONTE, &AutorEscrito::Ia("claude-sonnet-5-5".into()), "")
                .is_ok()
        );
        assert!(b.listar_escritos().unwrap().len() == 1);
    }

    #[test]
    fn o_autor_fica_registrado_como_texto() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let m = manifesto("contracheque");
        b.importar_escrito(&m, "a()", &AutorEscrito::Mcp, "")
            .unwrap();
        b.importar_escrito(&m, "b()", &AutorEscrito::Ia("claude-sonnet-5-5".into()), "")
            .unwrap();
        assert_eq!(
            b.versao_escrita("contracheque", 1).unwrap().autor,
            AutorEscrito::Mcp
        );
        let v2 = b.versao_escrita("contracheque", 2).unwrap();
        assert_eq!(v2.autor, AutorEscrito::Ia("claude-sonnet-5-5".into()));
        assert_eq!(v2.autor.texto(), "ia:claude-sonnet-5-5");
    }

    #[test]
    fn marcar_testado_so_de_rascunho() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        importar(&b, FONTE);
        let falhou = ResultadoDoTeste {
            passou: false,
            resumo: "seletor sumiu".into(),
        };
        assert_eq!(
            b.marcar_testado("contracheque", 1, &falhou).unwrap(),
            EstadoEscrito::Rascunho
        );
        let v = b.versao_escrita("contracheque", 1).unwrap();
        assert_eq!(v.estado, EstadoEscrito::Rascunho);
        assert!(v.resultado_do_teste.unwrap().starts_with("falhou"));
        assert_eq!(
            b.marcar_testado("contracheque", 1, &passou()).unwrap(),
            EstadoEscrito::Testado
        );
        // testado não volta a ser testado, e versão inexistente dá erro
        assert!(b.marcar_testado("contracheque", 1, &passou()).is_err());
        assert!(b.marcar_testado("contracheque", 9, &passou()).is_err());
        assert!(b.marcar_testado("nao-existe", 1, &passou()).is_err());
    }

    #[test]
    fn ativar_sem_testado_falha_em_rascunho_ativo_e_aposentado() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        importar(&b, FONTE);
        let e = b
            .ativar("contracheque", 1, &conf_ativar("contracheque", 1))
            .unwrap_err()
            .to_string();
        assert!(e.contains("testada"), "{e}");
        assert!(b.versao_ativa_escrita("contracheque").unwrap().is_none());
        b.marcar_testado("contracheque", 1, &passou()).unwrap();
        b.ativar("contracheque", 1, &conf_ativar("contracheque", 1))
            .unwrap();
        // já ativa
        assert!(
            b.ativar("contracheque", 1, &conf_ativar("contracheque", 1))
                .is_err()
        );
        b.desativar("contracheque", &conf_desativar("contracheque"))
            .unwrap();
        // aposentada não volta
        assert!(
            b.ativar("contracheque", 1, &conf_ativar("contracheque", 1))
                .is_err()
        );
    }

    #[test]
    fn ativar_aposenta_a_anterior_e_so_uma_fica_ativa() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let v1 = ativa(&b, "a()");
        let v2 = ativa(&b, "b()");
        assert_eq!((v1, v2), (1, 2));
        let d = b.mostrar("contracheque").unwrap();
        assert_eq!(d.versoes[0].estado, EstadoEscrito::Aposentado);
        assert_eq!(d.versoes[1].estado, EstadoEscrito::Ativo);
        assert_eq!(
            b.versao_ativa_escrita("contracheque")
                .unwrap()
                .unwrap()
                .numero,
            2
        );
        let l = b.listar_escritos().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].ativa, Some(2));
        assert_eq!(l[0].ultima, 2);
        assert_eq!(l[0].estado_da_ultima, EstadoEscrito::Ativo);
        assert_eq!(l[0].descricao, "Contracheque do mês");
    }

    #[test]
    fn a_confirmacao_e_do_pedido_e_exige_terminal() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        importar(&b, FONTE);
        b.marcar_testado("contracheque", 1, &passou()).unwrap();
        // sem terminal nem se constrói
        let e = Confirmacao::do_terminal(false, "contracheque@1", "contracheque@1")
            .unwrap_err()
            .to_string();
        assert!(e.contains("terminal interativo"), "{e}");
        // digitação errada
        assert!(Confirmacao::do_terminal(true, "sim", "contracheque@1").is_err());
        // confirmação de outra versão ou de outro roteiro não vale
        assert!(
            b.ativar("contracheque", 1, &conf_ativar("contracheque", 2))
                .is_err()
        );
        assert!(
            b.ativar("contracheque", 1, &conf_ativar("outro", 1))
                .is_err()
        );
        assert!(b.versao_ativa_escrita("contracheque").unwrap().is_none());
        // desativar com a confirmação de ativar também não vale
        ativa(&b, "c()");
        assert!(
            b.desativar("contracheque", &conf_ativar("contracheque", 2))
                .is_err()
        );
    }

    fn importar_como(b: &BancoDeRoteiros, autor: &AutorEscrito) -> u32 {
        let n = b
            .importar_escrito(&manifesto("contracheque"), FONTE, autor, "proposta")
            .unwrap()
            .numero;
        b.marcar_testado("contracheque", n, &passou()).unwrap();
        n
    }

    #[test]
    fn autor_usuario_digita_so_nome_e_versao() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let n = importar_como(&b, &AutorEscrito::Usuario);
        assert_eq!(
            b.esperado_para_ativar("contracheque", n).unwrap(),
            "contracheque@1"
        );
        b.ativar("contracheque", n, &conf_ativar("contracheque", n))
            .unwrap();
    }

    #[test]
    fn autor_mcp_ou_ia_exige_o_host_principal_na_digitacao() {
        for autor in [AutorEscrito::Mcp, AutorEscrito::Ia("modelo-x".into())] {
            let b = BancoDeRoteiros::em_memoria().unwrap();
            let n = importar_como(&b, &autor);
            let esperado = b.esperado_para_ativar("contracheque", n).unwrap();
            assert_eq!(esperado, "contracheque@1 portal.exemplo.test");
            // só `nome@versão` não vale para quem o MCP ou a IA propôs
            let sem_host =
                Confirmacao::do_terminal(true, "contracheque@1", "contracheque@1").unwrap();
            let e = b
                .ativar("contracheque", n, &sem_host)
                .unwrap_err()
                .to_string();
            assert!(e.contains("outro pedido"), "{e}");
            assert!(b.versao_ativa_escrita("contracheque").unwrap().is_none());
            // com o host digitado, ativa
            let c = Confirmacao::do_terminal(true, &esperado, &esperado).unwrap();
            b.ativar("contracheque", n, &c).unwrap();
            assert_eq!(
                b.versao_ativa_escrita("contracheque")
                    .unwrap()
                    .unwrap()
                    .numero,
                n
            );
        }
    }

    #[test]
    fn o_host_do_esperado_e_o_primeiro_de_hosts() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let mut m = manifesto("contracheque");
        m.hosts = vec!["auth.exemplo.test".into(), "portal.exemplo.test".into()];
        m.url_inicial = "https://auth.exemplo.test/entrar".into();
        b.importar_escrito(&m, FONTE, &AutorEscrito::Mcp, "")
            .unwrap();
        assert_eq!(
            b.esperado_para_ativar("contracheque", 1).unwrap(),
            "contracheque@1 auth.exemplo.test"
        );
        assert!(b.esperado_para_ativar("contracheque", 9).is_err());
        assert_eq!(b.esperado_para_desativar("contracheque"), "contracheque");
    }

    #[test]
    fn fonte_e_motivo_com_controle_ou_direcao_sao_recusados() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        for ruim in [
            "a()\u{1b}[3A",
            "a()\r\nb()",
            "a() --\u{202e}",
            "a()\u{2066}",
            "a()\u{85}",
            "a()\0",
        ] {
            let e = b
                .importar_escrito(&manifesto("contracheque"), ruim, &AutorEscrito::Usuario, "")
                .unwrap_err()
                .to_string();
            assert!(e.contains("controle"), "{ruim:?}: {e}");
            assert!(
                b.importar_escrito(
                    &manifesto("contracheque"),
                    FONTE,
                    &AutorEscrito::Usuario,
                    ruim
                )
                .is_err(),
                "motivo {ruim:?}"
            );
        }
        // quebra de linha e tabulação do fonte continuam valendo
        assert!(
            b.importar_escrito(
                &manifesto("contracheque"),
                "a()\n\tb()\n",
                &AutorEscrito::Usuario,
                ""
            )
            .is_ok()
        );
    }

    #[test]
    fn desativar_sem_ativa_ou_roteiro_inexistente_da_erro() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        assert!(
            b.desativar("nao-existe", &conf_desativar("nao-existe"))
                .is_err()
        );
        importar(&b, FONTE);
        assert!(
            b.desativar("contracheque", &conf_desativar("contracheque"))
                .is_err()
        );
        let n = ativa(&b, "x()");
        assert_eq!(
            b.desativar("contracheque", &conf_desativar("contracheque"))
                .unwrap(),
            n
        );
        assert!(b.versao_ativa_escrita("contracheque").unwrap().is_none());
    }

    #[test]
    fn editar_nunca_altera_a_versao_antiga() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let n = ativa(&b, "antigo()");
        let antes = b.versao_escrita("contracheque", n).unwrap();
        // "editar" é importar o fonte novo: nasce rascunho v2, a ativa segue igual
        let nova = importar(&b, "novo()");
        assert_eq!(nova.numero, 2);
        assert_eq!(b.versao_escrita("contracheque", n).unwrap(), antes);
        assert_eq!(
            b.versao_ativa_escrita("contracheque")
                .unwrap()
                .unwrap()
                .fonte,
            "antigo()"
        );
        // e o banco recusa por baixo dos panos: nem SQL direto edita ou apaga a versão
        for sql in [
            "UPDATE roteiro_escrito_versao SET fonte = 'x' WHERE numero = 1",
            "UPDATE roteiro_escrito_versao SET manifesto = '{}' WHERE numero = 1",
            "UPDATE roteiro_escrito_versao SET checksum = 'x' WHERE numero = 1",
            "UPDATE roteiro_escrito_versao SET autor = 'mcp' WHERE numero = 1",
            "DELETE FROM roteiro_escrito_versao WHERE numero = 1",
            "UPDATE roteiro_escrito_evento SET detalhe = 'x'",
            "DELETE FROM roteiro_escrito_evento",
        ] {
            assert!(b.conn.execute(sql, []).is_err(), "{sql}");
        }
        assert_eq!(b.versao_escrita("contracheque", n).unwrap(), antes);
    }

    #[test]
    fn o_banco_recusa_transicao_invalida_e_duas_ativas() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        importar(&b, "a()");
        importar(&b, "b()");
        // rascunho direto para ativo, aposentado ou testado->rascunho
        assert!(
            b.conn
                .execute(
                    "UPDATE roteiro_escrito_versao SET estado = 'ativo' WHERE numero = 1",
                    []
                )
                .is_err()
        );
        b.marcar_testado("contracheque", 1, &passou()).unwrap();
        assert!(
            b.conn
                .execute(
                    "UPDATE roteiro_escrito_versao SET estado = 'rascunho' WHERE numero = 1",
                    []
                )
                .is_err()
        );
        // duas ativas ao mesmo tempo
        b.conn
            .execute(
                "UPDATE roteiro_escrito_versao SET estado = 'ativo' WHERE numero = 1",
                [],
            )
            .unwrap();
        b.marcar_testado("contracheque", 2, &passou()).unwrap();
        assert!(
            b.conn
                .execute(
                    "UPDATE roteiro_escrito_versao SET estado = 'ativo' WHERE numero = 2",
                    []
                )
                .is_err()
        );
    }

    #[test]
    fn o_historico_conta_a_vida_da_versao() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        ativa(&b, "a()");
        ativa(&b, "b()");
        let d = b.mostrar("contracheque").unwrap();
        let passos: Vec<(u32, Option<&str>, &str)> = d
            .eventos
            .iter()
            .map(|e| (e.1, e.2.as_deref(), e.3.as_str()))
            .collect();
        assert_eq!(
            passos,
            vec![
                (1, None, "rascunho"),
                (1, Some("rascunho"), "testado"),
                (1, Some("testado"), "ativo"),
                (2, None, "rascunho"),
                (2, Some("rascunho"), "testado"),
                (1, Some("ativo"), "aposentado"),
                (2, Some("testado"), "ativo"),
            ]
        );
    }

    #[test]
    fn mostrar_e_versao_de_roteiro_inexistente_dao_erro() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        assert!(b.mostrar("nao-existe").is_err());
        assert!(b.versao_escrita("nao-existe", 1).is_err());
        assert!(b.versao_ativa_escrita("nao-existe").unwrap().is_none());
        assert!(b.listar_escritos().unwrap().is_empty());
    }

    #[test]
    fn roteiros_escritos_de_nomes_diferentes_nao_se_misturam() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        b.importar_escrito(&manifesto("um"), "a()", &AutorEscrito::Usuario, "")
            .unwrap();
        b.importar_escrito(&manifesto("dois"), "a()", &AutorEscrito::Usuario, "")
            .unwrap();
        assert_eq!(b.versao_escrita("um", 1).unwrap().roteiro, "um");
        assert_eq!(b.versao_escrita("dois", 1).unwrap().roteiro, "dois");
        let nomes: Vec<String> = b
            .listar_escritos()
            .unwrap()
            .into_iter()
            .map(|l| l.nome)
            .collect();
        assert_eq!(nomes, ["dois", "um"]);
    }

    #[test]
    fn o_arquivo_persiste_e_migra_sem_perder_o_que_existe() {
        let dir = tempfile::tempdir().unwrap();
        let caminho = dir.path().join("roteiros.sqlite");
        {
            let b = BancoDeRoteiros::abrir(&caminho).unwrap();
            b.criar_roteiro("gravado", "", "", None, &[], OrigemDaVersao::Manual)
                .ok();
            ativa(&b, "a()");
        }
        let b = BancoDeRoteiros::abrir(&caminho).unwrap();
        assert_eq!(
            b.versao_ativa_escrita("contracheque")
                .unwrap()
                .unwrap()
                .numero,
            1
        );
    }
}
