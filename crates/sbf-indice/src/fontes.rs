//! De onde vêm os trechos (spec 10): o texto dos achados (pedaços de ~800 caracteres), a
//! descrição e o fornecedor dos registros, e as movimentações do cache do livro-caixa.
//!
//! O cache do livro-caixa é lido direto (`livro-caixa.sqlite`, tabela `linha`, só leitura),
//! sem depender da estação: o JSON das linhas é o do contrato.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::embedder::Embedder;
use crate::erro::{ErroDoIndice, Resultado};
use crate::indice::{Indice, ItemRef, Relatorio, Trecho};
use crate::pedacos::dividir;

/// `R$ 1.234,56`-simples: sem milhar, `R$ 23,40`.
fn reais(centavos: i64) -> String {
    let abs = centavos.unsigned_abs();
    format!("R$ {},{:02}", abs / 100, abs % 100)
}

/// Os achados com texto extraído, em pedaços. O primeiro pedaço leva o nome do arquivo.
///
/// # Errors
/// Falha do SQLite.
pub fn trechos_dos_achados(inv: &Connection) -> Resultado<Vec<Trecho>> {
    let mut c = inv.prepare(
        "SELECT a.content_hash, t.conteudo,
                COALESCE((SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                           ORDER BY o.id LIMIT 1), '')
           FROM achado a JOIN texto t ON t.content_hash = a.content_hash
          WHERE a.estado <> 'descartado' ORDER BY a.content_hash",
    )?;
    let linhas = c.query_map([], |l| {
        Ok((
            l.get::<_, String>(0)?,
            l.get::<_, String>(1)?,
            l.get::<_, String>(2)?,
        ))
    })?;
    let mut saida = Vec::new();
    for l in linhas {
        let (hash, texto, arquivo) = l?;
        for (i, p) in dividir(&texto).into_iter().enumerate() {
            let texto = if i == 0 && !arquivo.is_empty() {
                format!("arquivo {arquivo}: {p}")
            } else {
                p
            };
            saida.push(Trecho {
                id: format!("achado:{hash}#{i}"),
                item: ItemRef::novo("achado", hash.clone()),
                texto,
            });
        }
    }
    Ok(saida)
}

/// Um trecho por registro com descrição ou fornecedor.
///
/// # Errors
/// Falha do SQLite.
pub fn trechos_dos_registros(inv: &Connection) -> Resultado<Vec<Trecho>> {
    let mut c = inv.prepare(
        "SELECT id, data, tipo, valor_centavos, descricao_original,
                COALESCE(NULLIF(fornecedor_normalizado, ''), supplier_name_raw), categoria_provavel
           FROM registro ORDER BY id",
    )?;
    let linhas = c.query_map([], |l| {
        Ok((
            l.get::<_, i64>(0)?,
            l.get::<_, String>(1)?,
            l.get::<_, String>(2)?,
            l.get::<_, i64>(3)?,
            l.get::<_, Option<String>>(4)?,
            l.get::<_, Option<String>>(5)?,
            l.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut saida = Vec::new();
    for l in linhas {
        let (id, data, tipo, valor, descricao, fornecedor, categoria) = l?;
        let descricao = descricao.unwrap_or_default();
        let fornecedor = fornecedor.unwrap_or_default();
        if descricao.trim().is_empty() && fornecedor.trim().is_empty() {
            continue;
        }
        let mut texto = format!("{data} {tipo} {descricao} {fornecedor} {}", reais(valor));
        if let Some(cat) = categoria.filter(|c| !c.is_empty()) {
            texto.push_str(&format!(" categoria {cat}"));
        }
        saida.push(Trecho {
            id: format!("registro:{id}"),
            item: ItemRef::novo("registro", id.to_string()),
            texto: texto.split_whitespace().collect::<Vec<_>>().join(" "),
        });
    }
    Ok(saida)
}

/// As linhas de uma coleção do cache do livro-caixa, como JSON. Cache ausente ou sem a
/// tabela dá lista vazia (a estação ainda não sincronizou).
///
/// # Errors
/// Falha ao ler uma linha que existe.
pub fn linhas_do_cache(cache: &Path, colecao: &str) -> Resultado<Vec<(String, Value)>> {
    if !cache.exists() {
        return Ok(Vec::new());
    }
    let c = Connection::open_with_flags(cache, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| ErroDoIndice::Livro(e.to_string()))?;
    c.busy_timeout(std::time::Duration::from_secs(10))?;
    let Ok(mut st) = c.prepare("SELECT id, dados FROM linha WHERE colecao = ?1 ORDER BY id") else {
        return Ok(Vec::new());
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
    Ok(saida)
}

fn nomes(cache: &Path, colecao: &str) -> Resultado<HashMap<String, String>> {
    Ok(linhas_do_cache(cache, colecao)?
        .into_iter()
        .filter_map(|(id, v)| Some((id, v.get("nome")?.as_str()?.to_owned())))
        .collect())
}

/// Um trecho por movimentação do cache do livro-caixa.
///
/// # Errors
/// Falha ao ler o cache.
pub fn trechos_das_movimentacoes(cache: &Path) -> Resultado<Vec<Trecho>> {
    let fornecedores = nomes(cache, "fornecedores")?;
    let categorias = nomes(cache, "categorias")?;
    let mut saida = Vec::new();
    for (id, v) in linhas_do_cache(cache, "movimentacoes")? {
        let texto_de = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
        let id_de = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
        let fornecedor = fornecedores
            .get(id_de("fornecedor"))
            .map_or("", String::as_str);
        let categoria = categorias
            .get(id_de("categoria"))
            .map_or("", String::as_str);
        let descricao = texto_de("descricao");
        if descricao.trim().is_empty() && fornecedor.is_empty() {
            continue;
        }
        let valor = v.get("valor").and_then(Value::as_i64).unwrap_or(0);
        let mut texto = format!(
            "{} {} {descricao} {fornecedor} {}",
            texto_de("data"),
            texto_de("tipo"),
            reais(valor)
        );
        if !categoria.is_empty() {
            texto.push_str(&format!(" categoria {categoria}"));
        }
        saida.push(Trecho {
            id: format!("movimentacao:{id}"),
            item: ItemRef::novo("movimentacao", id),
            texto: texto.split_whitespace().collect::<Vec<_>>().join(" "),
        });
    }
    Ok(saida)
}

/// A atualização incremental inteira: achados, registros e, se houver cache, movimentações.
/// É o que o `sbf worker` roda. Soma os relatórios das três fontes.
///
/// # Errors
/// Falha do SQLite, do cache ou do embedder.
pub fn atualizar(
    indice: &mut Indice,
    inventario: &Connection,
    cache_do_livro: Option<&Path>,
    embedder: &dyn Embedder,
) -> Resultado<Relatorio> {
    let mut total = Relatorio::default();
    let mut somar = |r: Relatorio| {
        total.embutidos += r.embutidos;
        total.mantidos += r.mantidos;
        total.removidos += r.removidos;
    };
    somar(indice.sincronizar("achado", &trechos_dos_achados(inventario)?, embedder)?);
    somar(indice.sincronizar("registro", &trechos_dos_registros(inventario)?, embedder)?);
    if let Some(cache) = cache_do_livro {
        somar(indice.sincronizar("movimentacao", &trechos_das_movimentacoes(cache)?, embedder)?);
    }
    Ok(total)
}
