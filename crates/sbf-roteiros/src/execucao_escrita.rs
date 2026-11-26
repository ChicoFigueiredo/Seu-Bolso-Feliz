//! O histórico das execuções do **roteiro escrito**: o que o botão "Executar agora", a agenda
//! do worker e o `testar` deixam no `roteiros.sqlite`.
//!
//! É o mesmo desenho das execuções do roteiro gravado (`execucao` e `evento`), em tabelas
//! próprias porque as daquele apontam para `roteiro`/`versao` (JSON). Nada aqui guarda senha,
//! CPF ou código de MFA: os textos que entram já vêm limpos de segredo.
//!
//! A pausa para o usuário usa o campo `continuar`, que a janela grava (`1` continua, `2`
//! cancela), como no roteiro gravado.

use crate::banco::{BancoDeRoteiros, EstadoDaExecucao};
use crate::erro::{ErroDeRoteiro, Resultado};
use rusqlite::{OptionalExtension as _, params};

/// O esquema das execuções do roteiro escrito (criado junto com o resto do banco).
pub(crate) const SQL: &str = "
CREATE TABLE IF NOT EXISTS execucao_escrita (
  id            INTEGER PRIMARY KEY,
  roteiro_id    INTEGER NOT NULL REFERENCES roteiro_escrito(id),
  versao_id     INTEGER NOT NULL REFERENCES roteiro_escrito_versao(id),
  gatilho       TEXT NOT NULL,
  iniciada_em   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  iniciada_unix INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  concluida_em  TEXT,
  estado        TEXT NOT NULL DEFAULT 'rodando' CHECK (estado IN
                  ('rodando','aguardando_usuario','concluida','falhou','bloqueada','cancelada')),
  chamada       INTEGER NOT NULL DEFAULT 0,
  motivo        TEXT,
  baixados      INTEGER NOT NULL DEFAULT 0,
  novos         INTEGER NOT NULL DEFAULT 0,
  continuar     INTEGER NOT NULL DEFAULT 0,
  pid           INTEGER
);
CREATE INDEX IF NOT EXISTS execucao_escrita_roteiro ON execucao_escrita(roteiro_id, id);
CREATE TABLE IF NOT EXISTS evento_escrito_execucao (
  id          INTEGER PRIMARY KEY,
  execucao_id INTEGER NOT NULL REFERENCES execucao_escrita(id) ON DELETE CASCADE,
  quando      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  tipo        TEXT NOT NULL,
  mensagem    TEXT NOT NULL
);
";

/// Uma execução do roteiro escrito, para o histórico da janela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeExecucaoEscrita {
    pub id: i64,
    pub roteiro: String,
    pub versao: u32,
    pub gatilho: String,
    pub iniciada_em: String,
    pub concluida_em: Option<String>,
    pub estado: EstadoDaExecucao,
    /// Quantas chamadas a verbos o roteiro já fez (onde parou).
    pub chamada: i64,
    pub motivo: Option<String>,
    pub baixados: i64,
    pub novos: i64,
}

/// Um evento da execução: (quando, tipo, mensagem).
pub type EventoDaExecucaoEscrita = (String, String, String);

use crate::banco::u32_de;

impl BancoDeRoteiros {
    /// Começa uma execução da versão `numero` do roteiro escrito `nome`, **atomicamente**: se
    /// o roteiro já tem uma rodando ou esperando o usuário (neste ou em outro processo),
    /// recusa. Vale para qualquer versão (a ativa, no botão e na agenda; um rascunho, no
    /// `testar`).
    ///
    /// # Errors
    /// Roteiro ou versão inexistente, roteiro já rodando, ou falha do banco.
    pub fn iniciar_execucao_escrita(
        &self,
        nome: &str,
        numero: u32,
        gatilho: &str,
    ) -> Resultado<i64> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let ids: Option<(i64, i64)> = tx
            .query_row(
                "SELECT r.id, v.id FROM roteiro_escrito r
                 JOIN roteiro_escrito_versao v ON v.roteiro_id = r.id
                 WHERE r.nome = ?1 AND v.numero = ?2",
                params![nome, numero],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .optional()?;
        let Some((roteiro_id, versao_id)) = ids else {
            return Err(ErroDeRoteiro::Invalido(format!(
                "roteiro escrito `{nome}` versão {numero} não encontrado"
            )));
        };
        let rodando: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM execucao_escrita WHERE roteiro_id = ?1
                             AND estado IN ('rodando','aguardando_usuario'))",
            [roteiro_id],
            |l| l.get(0),
        )?;
        if rodando {
            return Err(ErroDeRoteiro::Invalido("o roteiro já está rodando".into()));
        }
        tx.execute(
            "INSERT INTO execucao_escrita (roteiro_id, versao_id, gatilho, pid) VALUES (?1,?2,?3,?4)",
            params![roteiro_id, versao_id, gatilho, std::process::id()],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// A chamada a verbo em que a execução está (a n-ésima).
    ///
    /// # Errors
    /// Banco.
    pub fn atualizar_chamada_escrita(&self, execucao: i64, chamada: u32) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao_escrita SET chamada = ?2 WHERE id = ?1",
            params![execucao, chamada],
        )?;
        Ok(())
    }

    /// Acrescenta um evento (mensagem sem segredo, já limpa por quem chama).
    ///
    /// # Errors
    /// Banco.
    pub fn evento_escrito(&self, execucao: i64, tipo: &str, mensagem: &str) -> Resultado<()> {
        self.conn.execute(
            "INSERT INTO evento_escrito_execucao (execucao_id, tipo, mensagem) VALUES (?1,?2,?3)",
            params![
                execucao,
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
    pub fn eventos_escritos(
        &self,
        execucao: i64,
    ) -> Result<Vec<EventoDaExecucaoEscrita>, ErroDeRoteiro> {
        let mut st = self.conn.prepare(
            "SELECT quando, tipo, mensagem FROM evento_escrito_execucao
              WHERE execucao_id = ?1 ORDER BY id",
        )?;
        Ok(st
            .query_map([execucao], |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Fecha a execução.
    ///
    /// # Errors
    /// Banco.
    pub fn finalizar_escrita(
        &self,
        execucao: i64,
        estado: EstadoDaExecucao,
        motivo: Option<&str>,
        baixados: u32,
        novos: u32,
    ) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao_escrita SET estado = ?2, motivo = ?3, baixados = ?4, novos = ?5,
               concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?1",
            params![execucao, estado.texto(), motivo, baixados, novos],
        )?;
        Ok(())
    }

    /// A execução espera o usuário (a janela mostra "Sua vez no navegador").
    ///
    /// # Errors
    /// Banco.
    pub fn marcar_aguardando_escrita(&self, execucao: i64, motivo: &str) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao_escrita SET estado = 'aguardando_usuario', motivo = ?2, continuar = 0
              WHERE id = ?1",
            params![execucao, motivo],
        )?;
        Ok(())
    }

    /// A execução voltou a rodar.
    ///
    /// # Errors
    /// Banco.
    pub fn marcar_rodando_escrita(&self, execucao: i64) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao_escrita SET estado = 'rodando', motivo = NULL, continuar = 0
              WHERE id = ?1",
            [execucao],
        )?;
        Ok(())
    }

    /// A janela pede: `true` continua, `false` cancela.
    ///
    /// # Errors
    /// Banco.
    pub fn pedir_continuar_escrita(&self, execucao: i64, continuar: bool) -> Resultado<()> {
        self.conn.execute(
            "UPDATE execucao_escrita SET continuar = ?2
              WHERE id = ?1 AND estado = 'aguardando_usuario'",
            params![execucao, if continuar { 1 } else { 2 }],
        )?;
        Ok(())
    }

    /// O que a janela pediu: `0` nada, `1` continuar, `2` cancelar (execução sumida vale
    /// cancelar).
    ///
    /// # Errors
    /// Banco.
    pub fn pedido_do_usuario_escrita(&self, execucao: i64) -> Resultado<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT continuar FROM execucao_escrita WHERE id = ?1",
                [execucao],
                |l| l.get(0),
            )
            .optional()?
            .unwrap_or(2))
    }

    /// O histórico das execuções (as mais novas primeiro), de um roteiro ou de todos.
    ///
    /// # Errors
    /// Banco.
    pub fn historico_escrito(
        &self,
        nome: Option<&str>,
        limite: usize,
    ) -> Resultado<Vec<LinhaDeExecucaoEscrita>> {
        let mut st = self.conn.prepare(
            "SELECT e.id, r.nome, v.numero, e.gatilho, e.iniciada_em, e.concluida_em, e.estado,
                    e.chamada, e.motivo, e.baixados, e.novos
               FROM execucao_escrita e
               JOIN roteiro_escrito r ON r.id = e.roteiro_id
               JOIN roteiro_escrito_versao v ON v.id = e.versao_id
              WHERE (?1 IS NULL OR r.nome = ?1)
              ORDER BY e.id DESC LIMIT ?2",
        )?;
        let linhas = st
            .query_map(
                params![nome, i64::try_from(limite).unwrap_or(i64::MAX)],
                |l| {
                    Ok(LinhaDeExecucaoEscrita {
                        id: l.get(0)?,
                        roteiro: l.get(1)?,
                        versao: u32_de(l.get(2)?),
                        gatilho: l.get(3)?,
                        iniciada_em: l.get(4)?,
                        concluida_em: l.get(5)?,
                        estado: EstadoDaExecucao::de(&l.get::<_, String>(6)?),
                        chamada: l.get(7)?,
                        motivo: l.get(8)?,
                        baixados: l.get(9)?,
                        novos: l.get(10)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(linhas)
    }

    /// As execuções do roteiro escrito que esperam o usuário.
    ///
    /// # Errors
    /// Banco.
    pub fn aguardando_escritas(&self) -> Resultado<Vec<LinhaDeExecucaoEscrita>> {
        Ok(self
            .historico_escrito(None, 200)?
            .into_iter()
            .filter(|e| e.estado == EstadoDaExecucao::AguardandoUsuario)
            .collect())
    }

    /// O roteiro escrito tem uma execução rodando ou esperando o usuário?
    ///
    /// # Errors
    /// Banco.
    pub fn em_execucao_escrita(&self, nome: &str) -> Resultado<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM execucao_escrita e
                             JOIN roteiro_escrito r ON r.id = e.roteiro_id
                            WHERE r.nome = ?1 AND e.estado IN ('rodando','aguardando_usuario'))",
            [nome],
            |l| l.get(0),
        )?)
    }

    /// Marca como falhas as execuções do roteiro escrito que ficaram `rodando` ou `aguardando`
    /// de um processo que já não existe (o worker caiu no meio). Mesma regra do roteiro
    /// gravado: no Linux confere `/proc/<pid>`; nos outros sistemas, só as de mais de 6 horas.
    ///
    /// # Errors
    /// Banco.
    pub fn encerrar_orfas_escritas(&self) -> Resultado<usize> {
        let mut st = self.conn.prepare(
            "SELECT id, pid, iniciada_unix, strftime('%s','now') FROM execucao_escrita
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
                self.finalizar_escrita(
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

    /// Os roteiros escritos **ativos** cuja agenda venceu: a `agenda_horas` do manifesto da
    /// versão ativa e a última execução (de qualquer gatilho) dizem. Quem não tem agenda, ou
    /// está rodando, fica de fora. Devolve os nomes, em ordem.
    ///
    /// # Errors
    /// Banco.
    pub fn escritos_devidos(&self, agora_unix: i64) -> Resultado<Vec<String>> {
        let mut st = self.conn.prepare(
            "SELECT r.nome, v.manifesto,
                    COALESCE((SELECT MAX(e.iniciada_unix) FROM execucao_escrita e
                               WHERE e.roteiro_id = r.id), 0),
                    EXISTS(SELECT 1 FROM execucao_escrita e WHERE e.roteiro_id = r.id
                             AND e.estado IN ('rodando','aguardando_usuario'))
               FROM roteiro_escrito r
               JOIN roteiro_escrito_versao v ON v.roteiro_id = r.id AND v.estado = 'ativo'
              ORDER BY r.nome",
        )?;
        let linhas = st
            .query_map([], |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    l.get::<_, String>(1)?,
                    l.get::<_, i64>(2)?,
                    l.get::<_, bool>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut saida = Vec::new();
        for (nome, manifesto, ultima, rodando) in linhas {
            if rodando {
                continue;
            }
            let Ok(m) = crate::manifesto::Manifesto::de_json(&manifesto) else {
                continue;
            };
            let Some(horas) = m.agenda_horas else {
                continue;
            };
            if ultima.saturating_add(i64::from(horas) * 3600) <= agora_unix {
                saida.push(nome);
            }
        }
        Ok(saida)
    }
}
