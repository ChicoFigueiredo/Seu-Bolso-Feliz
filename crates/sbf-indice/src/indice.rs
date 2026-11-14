//! `indice.sqlite`: trechos com o vetor como BLOB de `f32` (little-endian), busca por força
//! bruta por cosseno (sem extensão nativa; basta para dezenas de milhares de trechos) e
//! atualização incremental pelo hash do trecho.
//!
//! O banco guarda o **texto** dos trechos, como o inventário já guarda (o dado em repouso
//! fica sob a cifra de disco do SO, ADR-015 item 3).

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use crate::embedder::Embedder;
use crate::erro::{ErroDoIndice, Resultado};

/// Quantos trechos vão ao embedder por vez.
const LOTE: usize = 32;

/// O item de onde veio um trecho: `achado:<hash>`, `registro:<id>`, `movimentacao:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ItemRef {
    pub tipo: String,
    pub id: String,
}

impl ItemRef {
    #[must_use]
    pub fn novo(tipo: &str, id: impl Into<String>) -> Self {
        Self {
            tipo: tipo.to_owned(),
            id: id.into(),
        }
    }
}

impl fmt::Display for ItemRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.tipo, self.id)
    }
}

/// Um trecho a indexar. `id` é único no índice (por exemplo `achado:<hash>#2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trecho {
    pub id: String,
    pub item: ItemRef,
    pub texto: String,
}

/// Um trecho recuperado, com a nota de semelhança (cosseno, de -1 a 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Parecido {
    pub trecho_id: String,
    pub item: ItemRef,
    pub texto: String,
    pub nota: f32,
}

/// O que uma sincronização fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Relatorio {
    /// Trechos novos ou mudados, que passaram pelo embedder.
    pub embutidos: usize,
    /// Trechos iguais: o vetor ficou como estava, sem chamar o modelo.
    pub mantidos: usize,
    /// Trechos que sumiram da fonte.
    pub removidos: usize,
}

/// O índice aberto.
#[derive(Debug)]
pub struct Indice {
    conexao: Connection,
}

fn hash_do_texto(texto: &str) -> String {
    Sha256::digest(texto.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn para_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn de_bytes(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

fn cosseno(a: &[f32], b: &[f32]) -> f32 {
    let (mut ab, mut aa, mut bb) = (0.0_f32, 0.0_f32, 0.0_f32);
    for (x, y) in a.iter().zip(b) {
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0.0 || bb == 0.0 {
        0.0
    } else {
        ab / (aa.sqrt() * bb.sqrt())
    }
}

impl Indice {
    /// Abre (criando) o índice em `caminho`.
    ///
    /// # Errors
    /// Falha do SQLite ou de disco.
    pub fn abrir(caminho: impl AsRef<Path>) -> Resultado<Self> {
        if let Some(pasta) = caminho.as_ref().parent() {
            std::fs::create_dir_all(pasta).map_err(|e| ErroDoIndice::Outro(e.to_string()))?;
        }
        Self::preparar(Connection::open(caminho)?)
    }

    /// Índice em memória (testes).
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn em_memoria() -> Resultado<Self> {
        Self::preparar(Connection::open_in_memory()?)
    }

    fn preparar(conexao: Connection) -> Resultado<Self> {
        conexao.busy_timeout(std::time::Duration::from_secs(30))?;
        conexao.execute_batch(
            "CREATE TABLE IF NOT EXISTS trecho (
               id        TEXT PRIMARY KEY,
               fonte     TEXT NOT NULL,
               item_tipo TEXT NOT NULL,
               item_id   TEXT NOT NULL,
               texto     TEXT NOT NULL,
               hash      TEXT NOT NULL,
               modelo    TEXT NOT NULL,
               vetor     BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS trecho_fonte ON trecho(fonte);
             CREATE INDEX IF NOT EXISTS trecho_item ON trecho(item_tipo, item_id);",
        )?;
        Ok(Self { conexao })
    }

    /// Quantos trechos há.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn total(&self) -> Resultado<usize> {
        let n: i64 = self
            .conexao
            .query_row("SELECT COUNT(*) FROM trecho", [], |l| l.get(0))?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Faz o índice da `fonte` (`achado`, `registro`, `movimentacao`) ficar igual a `trechos`.
    /// Só o que é novo, mudou (hash do texto) ou foi indexado por outro modelo passa pelo
    /// embedder; o que sumiu da fonte é apagado (RG-02).
    ///
    /// # Errors
    /// Falha do SQLite ou do embedder; nesse caso nada desta chamada fica gravado.
    pub fn sincronizar(
        &mut self,
        fonte: &str,
        trechos: &[Trecho],
        embedder: &dyn Embedder,
    ) -> Resultado<Relatorio> {
        let modelo = embedder.nome().to_owned();
        let existentes: HashMap<String, (String, String)> = {
            let mut c = self
                .conexao
                .prepare("SELECT id, hash, modelo FROM trecho WHERE fonte = ?1")?;
            c.query_map([fonte], |l| Ok((l.get(0)?, (l.get(1)?, l.get(2)?))))?
                .collect::<Result<_, _>>()?
        };
        let mut relatorio = Relatorio::default();
        let mut a_embutir: Vec<(&Trecho, String)> = Vec::new();
        for t in trechos {
            let h = hash_do_texto(&t.texto);
            match existentes.get(&t.id) {
                Some((hash, m)) if *hash == h && *m == modelo => relatorio.mantidos += 1,
                _ => a_embutir.push((t, h)),
            }
        }
        let presentes: std::collections::HashSet<&str> =
            trechos.iter().map(|t| t.id.as_str()).collect();
        let a_remover: Vec<&String> = existentes
            .keys()
            .filter(|id| !presentes.contains(id.as_str()))
            .collect();
        // Embute tudo antes de abrir a transação: falha do modelo não deixa meio índice.
        let mut vetores: Vec<Vec<f32>> = Vec::with_capacity(a_embutir.len());
        for lote in a_embutir.chunks(LOTE) {
            let textos: Vec<String> = lote.iter().map(|(t, _)| t.texto.clone()).collect();
            let v = embedder.embutir_trechos(&textos)?;
            if v.len() != textos.len() {
                return Err(ErroDoIndice::Embedder(
                    "o modelo devolveu vetores a menos".into(),
                ));
            }
            vetores.extend(v);
        }
        let tx = self.conexao.transaction()?;
        for id in &a_remover {
            tx.execute("DELETE FROM trecho WHERE id = ?1", [id])?;
            relatorio.removidos += 1;
        }
        for ((t, h), v) in a_embutir.iter().zip(&vetores) {
            tx.execute(
                "INSERT INTO trecho (id, fonte, item_tipo, item_id, texto, hash, modelo, vetor)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (id) DO UPDATE SET fonte = excluded.fonte, item_tipo = excluded.item_tipo,
                   item_id = excluded.item_id, texto = excluded.texto, hash = excluded.hash,
                   modelo = excluded.modelo, vetor = excluded.vetor",
                params![
                    t.id,
                    fonte,
                    t.item.tipo,
                    t.item.id,
                    t.texto,
                    h,
                    modelo,
                    para_bytes(v)
                ],
            )?;
            relatorio.embutidos += 1;
        }
        tx.commit()?;
        Ok(relatorio)
    }

    /// Os `k` trechos mais próximos de `consulta` (vetor da pergunta), do mais para o menos
    /// parecido. Força bruta: lê todos os vetores e ordena pela nota.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn buscar(&self, consulta: &[f32], k: usize) -> Resultado<Vec<Parecido>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let mut notas: Vec<(f32, String)> = {
            let mut c = self.conexao.prepare("SELECT id, vetor FROM trecho")?;
            let linhas = c.query_map([], |l| {
                let id: String = l.get(0)?;
                let b: Vec<u8> = l.get(1)?;
                Ok((id, b))
            })?;
            let mut v = Vec::new();
            for l in linhas {
                let (id, b) = l?;
                let vetor = de_bytes(&b);
                if vetor.len() == consulta.len() {
                    v.push((cosseno(consulta, &vetor), id));
                }
            }
            v
        };
        notas.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        notas.truncate(k);
        let mut saida = Vec::with_capacity(notas.len());
        for (nota, id) in notas {
            let (tipo, item_id, texto): (String, String, String) = self.conexao.query_row(
                "SELECT item_tipo, item_id, texto FROM trecho WHERE id = ?1",
                [&id],
                |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
            )?;
            saida.push(Parecido {
                trecho_id: id,
                item: ItemRef { tipo, id: item_id },
                texto,
                nota,
            });
        }
        Ok(saida)
    }
}
