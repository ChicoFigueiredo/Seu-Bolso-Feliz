//! O histórico das execuções do **roteiro escrito**: o que o botão "Executar agora", a agenda
//! do worker e o `testar` deixam no `roteiros.sqlite`.
//!
//! É o mesmo desenho das execuções do roteiro gravado (`execucao` e `evento`), em tabelas
//! próprias porque as daquele apontam para `roteiro`/`versao` (JSON). Nada aqui guarda senha,
//! CPF ou código de MFA: os textos que entram já vêm limpos de segredo.
//!
//! A pausa para o usuário usa o campo `continuar`, que a janela grava (`1` continua, `2`
//! cancela), como no roteiro gravado.

use crate::agenda;
use crate::banco::{BancoDeRoteiros, EstadoDaExecucao};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::manifesto::Manifesto;
use crate::parametros::ValorBruto;
use rusqlite::{Connection, OptionalExtension as _, params};
use std::collections::BTreeMap;

/// O esquema das execuções do roteiro escrito (criado junto com o resto do banco). As colunas
/// `entrada_agenda`, `previsto_unix` e `parametros_json` (spec 18) e o estado `entrada_recusada`
/// vieram depois: [`preparar`] leva um banco antigo até aqui sem perder nada.
fn ddl_da_tabela(nome: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {nome} (
  id            INTEGER PRIMARY KEY,
  roteiro_id    INTEGER NOT NULL REFERENCES roteiro_escrito(id),
  versao_id     INTEGER NOT NULL REFERENCES roteiro_escrito_versao(id),
  gatilho       TEXT NOT NULL,
  iniciada_em   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  iniciada_unix INTEGER NOT NULL DEFAULT (strftime('%s','now')),
  concluida_em  TEXT,
  estado        TEXT NOT NULL DEFAULT 'rodando' CHECK (estado IN
                  ('rodando','aguardando_usuario','concluida','falhou','bloqueada','cancelada',
                   'entrada_recusada')),
  chamada       INTEGER NOT NULL DEFAULT 0,
  motivo        TEXT,
  baixados      INTEGER NOT NULL DEFAULT 0,
  novos         INTEGER NOT NULL DEFAULT 0,
  continuar     INTEGER NOT NULL DEFAULT 0,
  pid           INTEGER,
  entrada_agenda  INTEGER,
  previsto_unix   INTEGER,
  parametros_json TEXT
);"
    )
}

const SQL_RESTANTE: &str = "
CREATE INDEX IF NOT EXISTS execucao_escrita_roteiro ON execucao_escrita(roteiro_id, id);
CREATE TABLE IF NOT EXISTS evento_escrito_execucao (
  id          INTEGER PRIMARY KEY,
  execucao_id INTEGER NOT NULL REFERENCES execucao_escrita(id) ON DELETE CASCADE,
  quando      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  tipo        TEXT NOT NULL,
  mensagem    TEXT NOT NULL
);
";

/// As colunas que já existiam antes da spec 18 (a cópia da reconstrução leva só estas).
const COLUNAS_ANTIGAS: &str = "id, roteiro_id, versao_id, gatilho, iniciada_em, iniciada_unix, \
     concluida_em, estado, chamada, motivo, baixados, novos, continuar, pid";

/// Cria o que falta e leva um banco antigo ao esquema atual, **sem apagar nada**: colunas
/// aditivas (`ALTER TABLE ADD COLUMN`) e, só se a tabela das execuções ainda não aceita o
/// estado `entrada_recusada` (um `CHECK` não se altera), a reconstrução dela com as linhas
/// copiadas. Os gatilhos de imutabilidade das versões não são tocados.
///
/// # Errors
/// Banco.
pub(crate) fn preparar(conn: &Connection) -> Resultado<()> {
    conn.execute_batch(&ddl_da_tabela("execucao_escrita"))?;
    conn.execute_batch(SQL_RESTANTE)?;
    adicionar_coluna(conn, "roteiro_escrito", "suspenso_unix", "INTEGER")?;
    adicionar_coluna(conn, "roteiro_escrito", "suspenso_motivo", "TEXT")?;
    for (coluna, tipo) in [
        ("entrada_agenda", "INTEGER"),
        ("previsto_unix", "INTEGER"),
        ("parametros_json", "TEXT"),
    ] {
        adicionar_coluna(conn, "execucao_escrita", coluna, tipo)?;
    }
    if !aceita_entrada_recusada(conn)? {
        reconstruir(conn)?;
    }
    Ok(())
}

fn aceita_entrada_recusada(conn: &Connection) -> Resultado<bool> {
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'execucao_escrita'",
            [],
            |l| l.get(0),
        )
        .optional()?;
    Ok(sql.is_none_or(|s| s.contains("entrada_recusada")))
}

fn adicionar_coluna(conn: &Connection, tabela: &str, coluna: &str, tipo: &str) -> Resultado<()> {
    let existe = {
        let mut st = conn.prepare(&format!("PRAGMA table_info({tabela})"))?;
        let nomes = st
            .query_map([], |l| l.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        nomes.iter().any(|n| n == coluna)
    };
    if existe {
        return Ok(());
    }
    match conn.execute_batch(&format!("ALTER TABLE {tabela} ADD COLUMN {coluna} {tipo}")) {
        Ok(()) => Ok(()),
        // Outro processo (a janela, o worker) acabou de acrescentar a mesma coluna.
        Err(e) if e.to_string().contains("duplicate column") => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// A receita oficial do SQLite para mudar um `CHECK`: com as chaves estrangeiras desligadas
/// (senão o `DROP TABLE` apagaria os eventos por cascata), cria a tabela nova, copia, troca.
fn reconstruir(conn: &Connection) -> Resultado<()> {
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let r = (|| -> Resultado<()> {
        let tx =
            rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
        // Outro processo pode ter reconstruído enquanto esperávamos a trava.
        if aceita_entrada_recusada(&tx)? {
            return Ok(());
        }
        tx.execute_batch(&format!(
            "{}
             INSERT INTO execucao_escrita_nova ({COLUNAS_ANTIGAS}, entrada_agenda, previsto_unix,
                                                  parametros_json)
               SELECT {COLUNAS_ANTIGAS}, entrada_agenda, previsto_unix, parametros_json
                 FROM execucao_escrita;
             DROP TABLE execucao_escrita;
             ALTER TABLE execucao_escrita_nova RENAME TO execucao_escrita;
             CREATE INDEX IF NOT EXISTS execucao_escrita_roteiro
               ON execucao_escrita(roteiro_id, id);",
            ddl_da_tabela("execucao_escrita_nova")
        ))?;
        tx.commit()?;
        Ok(())
    })();
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    r
}

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

/// De onde uma execução veio e com que parâmetros resolvidos ela rodou. Tudo vazio numa
/// execução manual sem parâmetros (botão, terminal).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InicioDaExecucao {
    /// A posição da entrada em `Manifesto::agenda` (só execução agendada).
    pub entrada: Option<usize>,
    /// O instante agendado (unix) que a tornou devida.
    pub previsto_unix: Option<i64>,
    /// Os parâmetros resolvidos, em JSON (mês e sim/não, nunca segredo).
    pub parametros_json: Option<String>,
}

/// Um roteiro escrito com uma entrada da agenda devida (ou a `agenda_horas` antiga vencida).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevidoEscrito {
    pub nome: String,
    /// A versão ativa.
    pub numero: u32,
    /// A entrada de `agenda` que venceu; `None` na `agenda_horas` antiga.
    pub entrada: Option<usize>,
    /// O instante agendado que a tornou devida; `None` na `agenda_horas` antiga.
    pub previsto_unix: Option<i64>,
    /// O que a entrada entrega aos parâmetros declarados (antes de resolver o mês relativo).
    pub parametros: BTreeMap<String, ValorBruto>,
}

/// Por que a agenda de um roteiro está suspensa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspensaoDaAgenda {
    /// Quando (unix) o roteiro foi suspenso.
    pub desde_unix: i64,
    /// O motivo, já limpo de segredo (até 200 caracteres).
    pub motivo: String,
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
        self.iniciar_execucao_escrita_com(nome, numero, gatilho, &InicioDaExecucao::default())
    }

    /// Como [`Self::iniciar_execucao_escrita`], guardando de onde a execução veio (a entrada da
    /// agenda e o instante previsto) e os parâmetros resolvidos que ela recebeu.
    ///
    /// # Errors
    /// As mesmas de [`Self::iniciar_execucao_escrita`].
    pub fn iniciar_execucao_escrita_com(
        &self,
        nome: &str,
        numero: u32,
        gatilho: &str,
        inicio: &InicioDaExecucao,
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
            "INSERT INTO execucao_escrita
               (roteiro_id, versao_id, gatilho, pid, entrada_agenda, previsto_unix, parametros_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                roteiro_id,
                versao_id,
                gatilho,
                std::process::id(),
                inicio.entrada.map(|e| e as i64),
                inicio.previsto_unix,
                inicio.parametros_json
            ],
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

    /// Quantos arquivos a execução descartou por já estarem no inventário (os eventos
    /// `descartado`, spec 18). Não conta em `novos`.
    ///
    /// # Errors
    /// Banco.
    pub fn descartados_da_execucao_escrita(&self, execucao: i64) -> Resultado<u32> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM evento_escrito_execucao
              WHERE execucao_id = ?1 AND tipo = 'descartado'",
            [execucao],
            |l| l.get(0),
        )?;
        Ok(u32_de(n))
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

    /// Os roteiros escritos **ativos** cuja agenda venceu (ver [`Self::escritos_devidos_agendados`]),
    /// só os nomes, em ordem e sem repetir.
    ///
    /// # Errors
    /// Banco.
    pub fn escritos_devidos(&self, agora_unix: i64) -> Resultado<Vec<String>> {
        let mut nomes: Vec<String> = Vec::new();
        for d in self.escritos_devidos_agendados(agora_unix)? {
            if !nomes.contains(&d.nome) {
                nomes.push(d.nome);
            }
        }
        Ok(nomes)
    }

    /// Os roteiros escritos **ativos** com agenda vencida, uma linha por entrada devida (na
    /// ordem da lista do manifesto). Fica de fora quem está rodando ou esperando o usuário e
    /// quem tem a agenda **suspensa** (`navegador.entrada_recusada`).
    ///
    /// - `agenda`: a entrada está devida se há um instante agendado `s` com `base < s <= agora`,
    ///   onde `base` é o maior entre o início da última execução DAQUELA entrada (da versão em
    ///   vigor) e a ativação da versão ([`crate::agenda`]).
    /// - `agenda_horas` (antiga): vence `horas` depois do início da última execução, de qualquer
    ///   gatilho.
    ///
    /// # Errors
    /// Banco.
    pub fn escritos_devidos_agendados(&self, agora_unix: i64) -> Resultado<Vec<DevidoEscrito>> {
        let mut st = self.conn.prepare(
            "SELECT r.id, r.nome, v.id, v.numero, v.manifesto, r.suspenso_unix,
                    EXISTS(SELECT 1 FROM execucao_escrita e WHERE e.roteiro_id = r.id
                             AND e.estado IN ('rodando','aguardando_usuario'))
               FROM roteiro_escrito r
               JOIN roteiro_escrito_versao v ON v.roteiro_id = r.id AND v.estado = 'ativo'
              ORDER BY r.nome",
        )?;
        let linhas = st
            .query_map([], |l| {
                Ok((
                    l.get::<_, i64>(0)?,
                    l.get::<_, String>(1)?,
                    l.get::<_, i64>(2)?,
                    l.get::<_, i64>(3)?,
                    l.get::<_, String>(4)?,
                    l.get::<_, Option<i64>>(5)?,
                    l.get::<_, bool>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut saida = Vec::new();
        for (roteiro_id, nome, versao_id, numero, manifesto, suspenso, rodando) in linhas {
            if rodando || suspenso.is_some() {
                continue;
            }
            let Ok(m) = Manifesto::de_json(&manifesto) else {
                continue;
            };
            let numero = crate::banco::u32_de(numero);
            if !m.agenda.is_empty() {
                let ativacao: i64 = self.conn.query_row(
                    "SELECT COALESCE(MAX(CAST(strftime('%s', quando) AS INTEGER)), 0)
                       FROM roteiro_escrito_evento
                      WHERE versao_id = ?1 AND de = 'testado' AND para = 'ativo'",
                    [versao_id],
                    |l| l.get(0),
                )?;
                let mut bases = Vec::with_capacity(m.agenda.len());
                for i in 0..m.agenda.len() {
                    let ultima: i64 = self.conn.query_row(
                        "SELECT COALESCE(MAX(iniciada_unix), 0) FROM execucao_escrita
                          WHERE roteiro_id = ?1 AND versao_id = ?2 AND entrada_agenda = ?3",
                        params![roteiro_id, versao_id, i as i64],
                        |l| l.get(0),
                    )?;
                    bases.push(ultima.max(ativacao));
                }
                for d in agenda::devidas(&m, agora_unix, &bases) {
                    saida.push(DevidoEscrito {
                        nome: nome.clone(),
                        numero,
                        entrada: Some(d.entrada),
                        previsto_unix: Some(d.previsto_unix),
                        parametros: m.agenda[d.entrada].parametros.clone(),
                    });
                }
            } else if let Some(horas) = m.agenda_horas {
                let ultima: i64 = self.conn.query_row(
                    "SELECT COALESCE(MAX(iniciada_unix), 0) FROM execucao_escrita
                      WHERE roteiro_id = ?1",
                    [roteiro_id],
                    |l| l.get(0),
                )?;
                if ultima.saturating_add(i64::from(horas) * 3600) <= agora_unix {
                    saida.push(DevidoEscrito {
                        nome,
                        numero,
                        entrada: None,
                        previsto_unix: None,
                        parametros: BTreeMap::new(),
                    });
                }
            }
        }
        Ok(saida)
    }

    /// De onde a execução veio (a entrada da agenda e o instante previsto) e os parâmetros que
    /// recebeu. Tudo `None` numa execução manual sem parâmetros.
    ///
    /// # Errors
    /// Banco.
    pub fn inicio_da_execucao_escrita(&self, execucao: i64) -> Resultado<InicioDaExecucao> {
        Ok(self
            .conn
            .query_row(
                "SELECT entrada_agenda, previsto_unix, parametros_json
                   FROM execucao_escrita WHERE id = ?1",
                [execucao],
                |l| {
                    Ok(InicioDaExecucao {
                        entrada: l
                            .get::<_, Option<i64>>(0)?
                            .and_then(|e| usize::try_from(e).ok()),
                        previsto_unix: l.get(1)?,
                        parametros_json: l.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    /// **Suspende a agenda** do roteiro: `escritos_devidos` deixa de devolvê-lo até
    /// [`Self::retomar_agenda_escrita`]. Guarda a data e o motivo (sem controle, até 200
    /// caracteres; quem chama já o limpou de segredo) e deixa uma linha no histórico da versão
    /// em vigor. Suspender de novo só atualiza a data e o motivo.
    ///
    /// # Errors
    /// Roteiro inexistente ou banco.
    pub fn suspender_agenda_escrita(
        &self,
        nome: &str,
        motivo: &str,
        agora_unix: i64,
    ) -> Resultado<()> {
        let motivo: String = motivo
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(200)
            .collect();
        let tx = self.conn.unchecked_transaction()?;
        let alterou = tx.execute(
            "UPDATE roteiro_escrito SET suspenso_unix = ?2, suspenso_motivo = ?3 WHERE nome = ?1",
            params![nome, agora_unix, motivo],
        )?;
        if alterou == 0 {
            return Err(ErroDeRoteiro::Invalido(format!(
                "roteiro escrito `{nome}` não encontrado"
            )));
        }
        Self::anotar_na_versao_ativa(
            &tx,
            nome,
            "agenda_suspensa",
            "agenda",
            &format!("agenda suspensa: {motivo}"),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// **Retoma a agenda** suspensa. Só o usuário (terminal interativo, depois a janela) chama:
    /// o MCP, o worker e a IA nunca (`tests/autoridade.rs`). Devolve `false` se não estava
    /// suspensa. A base das entradas não muda: o que ficou para trás durante a suspensão roda
    /// uma vez, como numa volta qualquer (o mais recente em `(base, agora]`).
    ///
    /// # Errors
    /// Roteiro inexistente ou banco.
    pub fn retomar_agenda_escrita(&self, nome: &str) -> Resultado<bool> {
        let tx = self.conn.unchecked_transaction()?;
        let suspensa: Option<Option<i64>> = tx
            .query_row(
                "SELECT suspenso_unix FROM roteiro_escrito WHERE nome = ?1",
                [nome],
                |l| l.get(0),
            )
            .optional()?;
        let Some(suspensa) = suspensa else {
            return Err(ErroDeRoteiro::Invalido(format!(
                "roteiro escrito `{nome}` não encontrado"
            )));
        };
        if suspensa.is_none() {
            return Ok(false);
        }
        tx.execute(
            "UPDATE roteiro_escrito SET suspenso_unix = NULL, suspenso_motivo = NULL
              WHERE nome = ?1",
            [nome],
        )?;
        Self::anotar_na_versao_ativa(&tx, nome, "agenda_retomada", "usuario", "agenda retomada")?;
        tx.commit()?;
        Ok(true)
    }

    /// A suspensão da agenda do roteiro, se houver.
    ///
    /// # Errors
    /// Banco.
    pub fn agenda_suspensa_escrita(&self, nome: &str) -> Resultado<Option<SuspensaoDaAgenda>> {
        Ok(self
            .conn
            .query_row(
                "SELECT suspenso_unix, COALESCE(suspenso_motivo, '') FROM roteiro_escrito
                  WHERE nome = ?1 AND suspenso_unix IS NOT NULL",
                [nome],
                |l| {
                    Ok(SuspensaoDaAgenda {
                        desde_unix: l.get(0)?,
                        motivo: l.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    /// Uma linha no histórico de estados da versão em vigor (se há uma).
    fn anotar_na_versao_ativa(
        conn: &Connection,
        nome: &str,
        para: &str,
        quem: &str,
        detalhe: &str,
    ) -> Resultado<()> {
        // `para` nunca é `ativo`: é o `ativo` que marca o instante da ativação.
        conn.execute(
            "INSERT INTO roteiro_escrito_evento (versao_id, de, para, quem, detalhe)
             SELECT v.id, 'ativo', ?4, ?2, ?3
               FROM roteiro_escrito_versao v JOIN roteiro_escrito r ON r.id = v.roteiro_id
              WHERE r.nome = ?1 AND v.estado = 'ativo'",
            params![nome, quem, detalhe, para],
        )?;
        Ok(())
    }
}
