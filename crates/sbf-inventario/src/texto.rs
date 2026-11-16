//! O texto extraído de um achado (`texto`) e o `canonical_fingerprint`: o SQL em um lugar só.
//!
//! Quem extrai (a rodada de PDF protegido, o reconhecimento de um arquivo solto) trata o texto
//! como o `e3-extrair.ts` trata (corte de 20 KB, impressão canônica; ver `sbf-pdf::texto`) e
//! grava por aqui. A linha em `texto` é por achado: a primeira extração vale, a seguinte não
//! sobrescreve (`INSERT OR IGNORE`).

use rusqlite::{Connection, OptionalExtension, params};

/// Uma linha de `texto`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextoGravado {
    /// `pdftotext`, `pdf-sem-texto`, `texto-simples`, `ocr-por`...
    pub metodo: String,
    /// Já cortado como o Bun corta; vazio quando não há texto.
    pub conteudo: String,
}

/// `texto.metodo` de um arquivo de texto simples lido direto.
pub const METODO_TEXTO_SIMPLES: &str = "texto-simples";

/// O achado já tem linha em `texto`?
///
/// # Errors
/// Falha do SQLite.
pub fn tem_texto(conexao: &Connection, content_hash: &str) -> rusqlite::Result<bool> {
    conexao
        .query_row(
            "SELECT 1 FROM texto WHERE content_hash = ?1",
            [content_hash],
            |_| Ok(()),
        )
        .optional()
        .map(|l| l.is_some())
}

/// A linha de `texto` do achado, se houver.
///
/// # Errors
/// Falha do SQLite.
pub fn ler_texto(
    conexao: &Connection,
    content_hash: &str,
) -> rusqlite::Result<Option<TextoGravado>> {
    conexao
        .query_row(
            "SELECT metodo, conteudo FROM texto WHERE content_hash = ?1",
            [content_hash],
            |l| {
                Ok(TextoGravado {
                    metodo: l.get(0)?,
                    conteudo: l.get(1)?,
                })
            },
        )
        .optional()
}

/// Grava o texto do achado e, quando há, o `canonical_fingerprint` dele (só se o texto entrou:
/// a primeira extração vale). Não abre transação: roda
/// dentro da de quem chama (a rodada de PDF protegido) ou use [`registrar_texto`].
///
/// # Errors
/// Falha do SQLite (o achado precisa existir: `texto.content_hash` é chave estrangeira).
pub fn gravar_texto(
    conexao: &Connection,
    content_hash: &str,
    metodo: &str,
    conteudo: &str,
    impressao: Option<&str>,
) -> rusqlite::Result<()> {
    let inseriu = conexao.execute(
        "INSERT OR IGNORE INTO texto (content_hash, metodo, conteudo) VALUES (?1, ?2, ?3)",
        params![content_hash, metodo, conteudo],
    )?;
    // A impressão é do texto que ficou: se já havia outro, ela não muda.
    if let (Some(impressao), 1) = (impressao, inseriu) {
        conexao.execute(
            "UPDATE achado SET canonical_fingerprint = ?2 WHERE content_hash = ?1",
            params![content_hash, impressao],
        )?;
    }
    Ok(())
}

/// [`gravar_texto`] numa transação IMMEDIATE curta, com a espera de 30 s por um banco
/// ocupado.
///
/// # Errors
/// Falha do SQLite.
pub fn registrar_texto(
    conexao: &Connection,
    content_hash: &str,
    metodo: &str,
    conteudo: &str,
    impressao: Option<&str>,
) -> rusqlite::Result<()> {
    conexao.busy_timeout(std::time::Duration::from_secs(30))?;
    let tx =
        rusqlite::Transaction::new_unchecked(conexao, rusqlite::TransactionBehavior::Immediate)?;
    gravar_texto(&tx, content_hash, metodo, conteudo, impressao)?;
    tx.commit()
}
