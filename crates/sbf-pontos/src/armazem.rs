//! `pontos.sqlite` (na pasta de dados do app, fora do cofre): um ponto por (regra, chave),
//! com estado, motivo e a assinatura da evidência.
//!
//! **Deduplicação (PA-02).** Um ponto aberto não se repete: a análise seguinte só o
//! atualiza. Resolvido ou ignorado só volta (a aberto, sem o motivo) se a assinatura da
//! evidência mudou. Um ponto aberto cuja regra rodou e já não o produz é resolvido sozinho,
//! com o motivo "deixou de valer" e assinatura vazia (se a evidência voltar, ele reabre).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};

use crate::erro::{ErroDosPontos, Resultado};
use crate::tipos::{Area, Candidato, Estado, Ponto};

/// O motivo gravado quando a análise fecha um ponto porque a evidência sumiu.
pub const MOTIVO_AUTOMATICO: &str = "deixou de valer";

/// O que uma aplicação de candidatos fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumo {
    pub novos: usize,
    pub reabertos: usize,
    pub atualizados: usize,
    pub fechados_sozinhos: usize,
}

/// Filtro da lista. `None` não filtra.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Filtro {
    pub area: Option<Area>,
    pub estado: Option<Estado>,
}

#[derive(Debug)]
pub struct Armazem {
    conexao: Connection,
}

fn hash(texto: &str) -> String {
    Sha256::digest(texto.as_bytes())
        .iter()
        .fold(String::new(), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn ler(l: &Row<'_>) -> rusqlite::Result<Ponto> {
    let area: String = l.get("area")?;
    let estado: String = l.get("estado")?;
    let evidencia: String = l.get("evidencia")?;
    let impacto: Option<String> = l.get("impacto")?;
    Ok(Ponto {
        id: l.get("id")?,
        regra: l.get("regra")?,
        chave: l.get("chave")?,
        area: Area::de_codigo(&area).unwrap_or(Area::Operacao),
        titulo: l.get("titulo")?,
        detalhe: l.get("detalhe")?,
        evidencia: serde_json::from_str(&evidencia).unwrap_or_default(),
        impacto: impacto.and_then(|i| serde_json::from_str(&i).ok()),
        estado: Estado::de_codigo(&estado).unwrap_or(Estado::Aberto),
        motivo: l.get("motivo")?,
        criado_em: l.get("criado_em")?,
        atualizado_em: l.get("atualizado_em")?,
        notificado_em: l.get("notificado_em")?,
    })
}

impl Armazem {
    /// Abre (criando) `pontos.sqlite`.
    ///
    /// # Errors
    /// Falha de disco ou do SQLite.
    pub fn abrir(caminho: impl AsRef<Path>) -> Resultado<Self> {
        if let Some(p) = caminho.as_ref().parent() {
            std::fs::create_dir_all(p)?;
        }
        Self::preparar(Connection::open(caminho)?)
    }

    /// Em memória (testes).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn em_memoria() -> Resultado<Self> {
        Self::preparar(Connection::open_in_memory()?)
    }

    fn preparar(conexao: Connection) -> Resultado<Self> {
        conexao.busy_timeout(std::time::Duration::from_secs(30))?;
        conexao.execute_batch(
            "CREATE TABLE IF NOT EXISTS ponto (
               id            INTEGER PRIMARY KEY,
               regra         TEXT NOT NULL,
               chave         TEXT NOT NULL,
               area          TEXT NOT NULL,
               titulo        TEXT NOT NULL,
               detalhe       TEXT NOT NULL,
               evidencia     TEXT NOT NULL,
               impacto       TEXT,
               estado        TEXT NOT NULL DEFAULT 'aberto'
                               CHECK (estado IN ('aberto', 'resolvido', 'ignorado')),
               motivo        TEXT,
               assinatura    TEXT NOT NULL,
               criado_em     TEXT NOT NULL,
               atualizado_em TEXT NOT NULL,
               notificado_em TEXT,
               UNIQUE (regra, chave)
             );
             CREATE INDEX IF NOT EXISTS ponto_estado ON ponto(estado, area);",
        )?;
        Ok(Self { conexao })
    }

    /// Aplica o resultado de uma análise. `avaliadas` são as regras que rodaram de verdade
    /// (uma regra sem dado para ler não fecha pontos).
    ///
    /// # Errors
    /// Falha do SQLite; nesse caso nada é gravado.
    pub fn aplicar(
        &mut self,
        candidatos: &[Candidato],
        avaliadas: &[&str],
        agora: &str,
    ) -> Resultado<Resumo> {
        let tx = self.conexao.transaction()?;
        let mut r = Resumo::default();
        let mut vistos: HashSet<(String, String)> = HashSet::new();
        for c in candidatos {
            if !vistos.insert((c.regra.to_owned(), c.chave.clone())) {
                continue;
            }
            let assinatura = hash(&format!("{}\u{0}{}", c.regra, c.assinatura));
            let evidencia = serde_json::to_string(&c.evidencia)?;
            let impacto = c.impacto.as_ref().map(serde_json::to_string).transpose()?;
            let atual: Option<(i64, String, String)> = tx
                .query_row(
                    "SELECT id, estado, assinatura FROM ponto WHERE regra = ?1 AND chave = ?2",
                    params![c.regra, c.chave],
                    |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
                )
                .optional()?;
            match atual {
                None => {
                    tx.execute(
                        "INSERT INTO ponto (regra, chave, area, titulo, detalhe, evidencia, impacto,
                           assinatura, criado_em, atualizado_em)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                        params![
                            c.regra,
                            c.chave,
                            c.area.codigo(),
                            c.titulo,
                            c.detalhe,
                            evidencia,
                            impacto,
                            assinatura,
                            agora
                        ],
                    )?;
                    r.novos += 1;
                }
                Some((id, estado, antiga)) => {
                    let aberto = estado == "aberto";
                    if !aberto && antiga == assinatura {
                        continue;
                    }
                    tx.execute(
                        "UPDATE ponto SET area = ?2, titulo = ?3, detalhe = ?4, evidencia = ?5,
                           impacto = ?6, assinatura = ?7, atualizado_em = ?8,
                           estado = 'aberto',
                           motivo = CASE WHEN ?9 THEN motivo ELSE NULL END,
                           notificado_em = CASE WHEN ?9 THEN notificado_em ELSE NULL END
                         WHERE id = ?1",
                        params![
                            id,
                            c.area.codigo(),
                            c.titulo,
                            c.detalhe,
                            evidencia,
                            impacto,
                            assinatura,
                            agora,
                            aberto
                        ],
                    )?;
                    if aberto {
                        if antiga != assinatura {
                            r.atualizados += 1;
                        }
                    } else {
                        r.reabertos += 1;
                    }
                }
            }
        }
        let abertos: Vec<(i64, String, String)> = {
            let mut c = tx.prepare("SELECT id, regra, chave FROM ponto WHERE estado = 'aberto'")?;
            c.query_map([], |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)))?
                .collect::<Result<_, _>>()?
        };
        for (id, regra, chave) in abertos {
            if avaliadas.contains(&regra.as_str()) && !vistos.contains(&(regra, chave)) {
                tx.execute(
                    "UPDATE ponto SET estado = 'resolvido', motivo = ?2, assinatura = '',
                       atualizado_em = ?3 WHERE id = ?1",
                    params![id, MOTIVO_AUTOMATICO, agora],
                )?;
                r.fechados_sozinhos += 1;
            }
        }
        tx.commit()?;
        Ok(r)
    }

    /// Os pontos do filtro, os mais novos primeiro.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn listar(&self, filtro: Filtro) -> Resultado<Vec<Ponto>> {
        let mut c = self.conexao.prepare(
            "SELECT * FROM ponto
              WHERE (?1 IS NULL OR area = ?1) AND (?2 IS NULL OR estado = ?2)
              ORDER BY atualizado_em DESC, id DESC",
        )?;
        let v = c
            .query_map(
                params![
                    filtro.area.map(Area::codigo),
                    filtro.estado.map(Estado::codigo)
                ],
                ler,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Um ponto pelo id.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn ponto(&self, id: i64) -> Resultado<Option<Ponto>> {
        Ok(self
            .conexao
            .query_row("SELECT * FROM ponto WHERE id = ?1", [id], ler)
            .optional()?)
    }

    /// Quantos pontos abertos há, por área.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn abertos_por_area(&self) -> Resultado<HashMap<Area, usize>> {
        let mut c = self
            .conexao
            .prepare("SELECT area, COUNT(*) FROM ponto WHERE estado = 'aberto' GROUP BY area")?;
        let linhas = c.query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, i64>(1)?)))?;
        let mut m = HashMap::new();
        for l in linhas {
            let (a, n) = l?;
            if let Some(a) = Area::de_codigo(&a) {
                m.insert(a, usize::try_from(n).unwrap_or(0));
            }
        }
        Ok(m)
    }

    fn fechar(&self, id: i64, estado: Estado, motivo: &str, agora: &str) -> Resultado<()> {
        let motivo = motivo.trim();
        if motivo.is_empty() {
            return Err(ErroDosPontos::Entrada(
                "diga o motivo para resolver ou ignorar".into(),
            ));
        }
        let n = self.conexao.execute(
            "UPDATE ponto SET estado = ?2, motivo = ?3, atualizado_em = ?4
              WHERE id = ?1 AND estado = 'aberto'",
            params![id, estado.codigo(), motivo, agora],
        )?;
        if n == 0 {
            return Err(ErroDosPontos::Entrada(
                "o ponto não existe ou já foi fechado".into(),
            ));
        }
        Ok(())
    }

    /// Marca como resolvido, com o motivo (obrigatório).
    ///
    /// # Errors
    /// Motivo vazio, ponto inexistente ou já fechado.
    pub fn resolver(&self, id: i64, motivo: &str, agora: &str) -> Resultado<()> {
        self.fechar(id, Estado::Resolvido, motivo, agora)
    }

    /// Marca como ignorado, com o motivo (obrigatório).
    ///
    /// # Errors
    /// Motivo vazio, ponto inexistente ou já fechado.
    pub fn ignorar(&self, id: i64, motivo: &str, agora: &str) -> Resultado<()> {
        self.fechar(id, Estado::Ignorado, motivo, agora)
    }

    /// Os pontos abertos que nenhum canal avisou ainda.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn para_avisar(&self) -> Resultado<Vec<Ponto>> {
        let mut c = self.conexao.prepare(
            "SELECT * FROM ponto WHERE estado = 'aberto' AND notificado_em IS NULL ORDER BY id",
        )?;
        Ok(c.query_map([], ler)?.collect::<Result<Vec<_>, _>>()?)
    }

    /// Anota que um canal avisou estes pontos.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn marcar_avisados(&self, ids: &[i64], agora: &str) -> Resultado<()> {
        for id in ids {
            self.conexao.execute(
                "UPDATE ponto SET notificado_em = ?2 WHERE id = ?1",
                params![id, agora],
            )?;
        }
        Ok(())
    }
}
