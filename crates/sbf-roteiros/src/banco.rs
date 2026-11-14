//! O `roteiros.sqlite`, na pasta de dados do app (fora do cofre): roteiros, versões,
//! execuções e eventos. **Nunca guarda senha**: o passo de senha só tem o rótulo do chaveiro.
//!
//! A janela e o worker abrem o mesmo arquivo (WAL, espera de 5 s). A pausa para o usuário
//! passa por aqui: o executor marca a execução como `aguardando_usuario` e fica olhando o
//! campo `continuar`; a janela grava `1` (Continuar) ou `2` (Cancelar).

use crate::erro::{ErroDeRoteiro, Resultado};
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
  continuar    INTEGER NOT NULL DEFAULT 0
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
    fn de(t: &str) -> Self {
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

/// O banco dos roteiros.
#[derive(Debug)]
pub struct BancoDeRoteiros {
    conn: Connection,
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
        let id = self.criar_roteiro(nome, descricao, url_inicial, rotulo_senha, &[], OrigemDaVersao::Gravacao)?;
        self.conn.execute("UPDATE roteiro SET ativo = 0 WHERE id = ?1", [id])?;
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
        Roteiro { formato: crate::passo::FORMATO, nome, descricao, passos: passos.to_vec() }
            .validar()
            .map_err(ErroDeRoteiro::Invalido)?;
        self.conn.execute(
            "UPDATE versao SET passos = ?2 WHERE roteiro_id = ?1 AND estado = 'ativa'",
            params![roteiro_id, serde_json::to_string(passos).unwrap_or_default()],
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
            .query_row("SELECT id FROM roteiro WHERE nome = ?1", [nome], |l| l.get(0))
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
        self.conn
            .execute("UPDATE versao SET estado = 'ativa' WHERE id = ?1", [p.versao_id])?;
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
        for (id, nome, descricao, url_inicial, rotulo_senha, agenda_horas, ativo, versao, passos, ha_proposta) in linhas {
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
    pub fn historico(&self, roteiro_id: Option<i64>, limite: usize) -> Resultado<Vec<LinhaDeExecucao>> {
        let mut st = self.conn.prepare(
            "SELECT e.id, e.roteiro_id, r.nome, v.numero, e.gatilho, e.iniciada_em, e.concluida_em,
                    e.estado, e.passo_atual, e.motivo, e.baixados, e.novos
               FROM execucao e JOIN roteiro r ON r.id = e.roteiro_id JOIN versao v ON v.id = e.versao_id
              WHERE (?1 IS NULL OR e.roteiro_id = ?1)
              ORDER BY e.id DESC LIMIT ?2",
        )?;
        let linhas = st
            .query_map(params![roteiro_id, i64::try_from(limite).unwrap_or(50)], |l| {
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
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(linhas)
    }

    /// Começa uma execução.
    ///
    /// # Errors
    /// Banco.
    pub fn iniciar_execucao(&self, roteiro_id: i64, versao_id: i64, gatilho: &str) -> Resultado<i64> {
        self.conn.execute(
            "INSERT INTO execucao (roteiro_id, versao_id, gatilho) VALUES (?1,?2,?3)",
            params![roteiro_id, versao_id, gatilho],
        )?;
        Ok(self.conn.last_insert_rowid())
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
    pub fn evento(&self, execucao: i64, passo: Option<usize>, tipo: &str, mensagem: &str) -> Resultado<()> {
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
    pub fn eventos(&self, execucao: i64) -> Resultado<Vec<(String, Option<i64>, String, String)>> {
        let mut st = self.conn.prepare(
            "SELECT quando, passo, tipo, mensagem FROM evento WHERE execucao_id = ?1 ORDER BY id",
        )?;
        Ok(st
            .query_map([execucao], |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)))?
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
            .query_row("SELECT continuar FROM execucao WHERE id = ?1", [execucao], |l| l.get(0))
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

    /// Marca como falhas as execuções que ficaram `rodando` ou `aguardando` de uma vida
    /// anterior do processo (o worker caiu no meio).
    ///
    /// # Errors
    /// Banco.
    pub fn encerrar_orfas(&self) -> Resultado<usize> {
        Ok(self.conn.execute(
            "UPDATE execucao SET estado = 'falhou', motivo = 'interrompida: o processo parou antes do fim',
               concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE estado IN ('rodando','aguardando_usuario')",
            [],
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

#[cfg(test)]
mod testes {
    use super::*;

    fn passos() -> Vec<Passo> {
        vec![Passo::Navegar { url: "http://127.0.0.1:1/".into() }]
    }

    #[test]
    fn criar_listar_proposta_aprovar() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b
            .criar_roteiro("extratos", "d", "http://127.0.0.1:1/", Some("banco"), &passos(), OrigemDaVersao::Gravacao)
            .unwrap();
        assert!(b.criar_roteiro("extratos", "", "", None, &passos(), OrigemDaVersao::Manual).is_err());
        let l = b.listar().unwrap();
        assert_eq!((l.len(), l[0].versao, l[0].passos, l[0].ha_proposta), (1, 1, 1, false));
        let novos = vec![Passo::Navegar { url: "http://127.0.0.1:1/novo".into() }];
        b.propor_versao(id, &novos, "o site mudou", OrigemDaVersao::Conserto).unwrap();
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
        let id = b.criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual).unwrap();
        b.propor_versao(id, &[Passo::Clicar { seletor: "#a".into() }], "m", OrigemDaVersao::Conserto).unwrap();
        b.rejeitar_proposta(id).unwrap();
        assert!(b.proposta(id).unwrap().is_none());
        assert_eq!(b.versao_ativa(id).unwrap().1, 1);
    }

    #[test]
    fn agenda_so_dispara_depois_do_intervalo() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b.criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual).unwrap();
        assert!(b.devidos(1_000_000).unwrap().is_empty(), "sem agenda não é devido");
        b.definir_agenda(id, Some(24)).unwrap();
        assert_eq!(b.devidos(1_000_000).unwrap(), vec![id], "nunca rodou");
        let v = b.versao_ativa(id).unwrap().0;
        let e = b.iniciar_execucao(id, v, "agenda").unwrap();
        assert!(b.devidos(i64::MAX / 4).unwrap().is_empty(), "rodando não repete");
        b.finalizar(e, EstadoDaExecucao::Concluida, None, 0, 0).unwrap();
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
    fn pausa_pelo_banco() {
        let b = BancoDeRoteiros::em_memoria().unwrap();
        let id = b.criar_roteiro("x", "", "", None, &passos(), OrigemDaVersao::Manual).unwrap();
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
