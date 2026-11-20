//! O cofre visto por Rita: o texto guardado de um documento e os documentos parecidos que o
//! pré-teste usa (spec 15, D15). Só leitura, sobre uma conexão própria do `inventario.sqlite`.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use sbf_regras::{Entrada, TipoDeEntrada};

use crate::contexto::{ARQUIVO_DO_INVENTARIO, entrada_de_documento};

/// De onde vêm as entradas parecidas com o item (mesmo tipo de entrada e mesmo tipo real).
pub trait FonteDeSimilares: Send + Sync {
    /// Até `limite` entradas com o mesmo tipo de entrada e MIME do `item`, com texto guardado,
    /// sem o próprio item. Falha de leitura vira lista vazia.
    fn similares(&self, item: &Entrada, limite: usize) -> Vec<Entrada>;
}

/// Os documentos do inventário do cofre.
#[derive(Debug, Clone)]
pub struct SimilaresDoCofre {
    banco: PathBuf,
}

impl SimilaresDoCofre {
    pub fn novo(cofre: &Path) -> Self {
        Self {
            banco: cofre.join(ARQUIVO_DO_INVENTARIO),
        }
    }
}

fn abrir(banco: &Path) -> Option<Connection> {
    if !banco.is_file() {
        return None;
    }
    sbf_inventario::banco::abrir_leitura(banco).ok()
}

fn ler(
    conn: &Connection,
    mime: Option<&str>,
    sem: &str,
    limite: usize,
) -> rusqlite::Result<Vec<Entrada>> {
    let mut st = conn.prepare(
        "SELECT a.content_hash, a.mime_type, t.conteudo,
                COALESCE((SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                          ORDER BY o.id LIMIT 1), '')
         FROM achado a JOIN texto t ON t.content_hash = a.content_hash
         WHERE a.content_hash <> ?1 AND (?2 IS NULL OR a.mime_type = ?2)
           AND length(t.conteudo) > 0
         ORDER BY a.primeiro_visto_em DESC, a.content_hash
         LIMIT ?3",
    )?;
    let limite = i64::try_from(limite).unwrap_or(i64::MAX);
    st.query_map(params![sem, mime, limite], |l| {
        let sha: String = l.get(0)?;
        let mime: Option<String> = l.get(1)?;
        let texto: String = l.get(2)?;
        let nome: String = l.get(3)?;
        Ok(entrada_de_documento(
            &sha,
            (!nome.is_empty()).then_some(nome.as_str()),
            mime.as_deref(),
            Some(&texto),
        ))
    })?
    .collect()
}

impl FonteDeSimilares for SimilaresDoCofre {
    fn similares(&self, item: &Entrada, limite: usize) -> Vec<Entrada> {
        if item.tipo != TipoDeEntrada::Documento {
            return Vec::new();
        }
        let Some(conn) = abrir(&self.banco) else {
            return Vec::new();
        };
        ler(&conn, item.mime.as_deref(), &item.id, limite).unwrap_or_default()
    }
}

/// Os documentos parecidos de uma fonte fixa (testes).
#[derive(Debug, Clone, Default)]
pub struct SimilaresFixos(pub Vec<Entrada>);

impl FonteDeSimilares for SimilaresFixos {
    fn similares(&self, item: &Entrada, limite: usize) -> Vec<Entrada> {
        self.0
            .iter()
            .filter(|e| e.id != item.id && e.tipo == item.tipo && e.mime == item.mime)
            .take(limite)
            .cloned()
            .collect()
    }
}

/// O texto guardado de um documento (`texto.conteudo`), se houver.
pub fn texto_guardado(cofre: &Path, sha256: &str) -> Option<String> {
    let conn = abrir(&cofre.join(ARQUIVO_DO_INVENTARIO))?;
    sbf_inventario::texto::ler_texto(&conn, sha256)
        .ok()
        .flatten()
        .map(|t| t.conteudo)
        .filter(|t| !t.trim().is_empty())
}

/// Nome e MIME de um achado: o primeiro nome de origem e o `mime_type`.
pub fn nome_e_mime(cofre: &Path, sha256: &str) -> Option<(Option<String>, Option<String>)> {
    let conn = abrir(&cofre.join(ARQUIVO_DO_INVENTARIO))?;
    conn.query_row(
        "SELECT a.mime_type,
                (SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                 ORDER BY o.id LIMIT 1)
         FROM achado a WHERE a.content_hash = ?1",
        params![sha256],
        |l| {
            Ok((
                l.get::<_, Option<String>>(1)?,
                l.get::<_, Option<String>>(0)?,
            ))
        },
    )
    .optional()
    .ok()
    .flatten()
}

/// Todos os achados que começam por `prefixo` (para `--documento <sha>` abreviado).
pub fn achar_pelo_prefixo(cofre: &Path, prefixo: &str) -> Vec<String> {
    let Some(conn) = abrir(&cofre.join(ARQUIVO_DO_INVENTARIO)) else {
        return Vec::new();
    };
    let Ok(mut st) = conn.prepare(
        "SELECT content_hash FROM achado WHERE content_hash LIKE ?1 || '%' ORDER BY content_hash LIMIT 3",
    ) else {
        return Vec::new();
    };
    st.query_map(params![prefixo], |l| l.get::<_, String>(0))
        .map(|r| r.filter_map(Result::ok).collect())
        .unwrap_or_default()
}
