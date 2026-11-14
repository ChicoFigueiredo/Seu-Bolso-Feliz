//! As regras, por área. Cada uma lê só o que precisa e devolve [`crate::Candidato`]s.

pub mod custo_ia;
pub mod dinheiro;
pub mod operacao;
pub mod tempo;

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::erro::Resultado;

/// As linhas de uma coleção do cache do livro-caixa, como JSON (só leitura). Cache ausente
/// ou sem a tabela dá `None`: a regra não pode rodar.
pub(crate) fn linhas_do_cache(
    cache: &Path,
    colecao: &str,
) -> Resultado<Option<Vec<(String, Value)>>> {
    if !cache.exists() {
        return Ok(None);
    }
    let c = Connection::open_with_flags(cache, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    c.busy_timeout(std::time::Duration::from_secs(10))?;
    let Ok(mut st) = c.prepare("SELECT id, dados FROM linha WHERE colecao = ?1 ORDER BY id") else {
        return Ok(None);
    };
    let linhas = st.query_map([colecao], |l| {
        Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?))
    })?;
    let mut saida = Vec::new();
    for l in linhas {
        let (id, dados) = l?;
        if let Ok(v) = serde_json::from_str::<Value>(&dados) {
            saida.push((id, v));
        }
    }
    Ok(Some(saida))
}
