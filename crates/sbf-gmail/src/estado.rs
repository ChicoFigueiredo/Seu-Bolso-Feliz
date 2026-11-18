//! O estado da conexão com o Gmail, para a área Conexões da janela.
//!
//! `estado_do_gmail(cofre)` só **lê**: as credenciais (só se as variáveis existem, nunca o
//! valor) e o que o inventário do cofre sabe das coletas. Não toca na rede.

use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension as _};

use crate::fonte;
use crate::gravador::FONTE;

/// Em que pé está a conexão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SituacaoDaConexao {
    /// Faltam variáveis de ambiente com as credenciais OAuth.
    SemCredenciais,
    /// Credenciais presentes e nenhuma coleta ainda.
    Pronta,
    /// A última coleta terminou bem.
    Coletada,
    /// A última coleta falhou (ou interrompeu antes de terminar).
    ComFalha,
}

/// O estado do Gmail como a área Conexões mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDaConexao {
    pub situacao: SituacaoDaConexao,
    /// As variáveis de credencial que faltam (só os nomes).
    pub faltando: Vec<String>,
    /// Quando a última coleta terminou bem (ISO 8601 UTC).
    pub ultima_coleta: Option<String>,
    /// A consulta da última coleta.
    pub consulta: Option<String>,
    /// Quantas origens do Gmail o inventário tem (anexos e textos de e-mail).
    pub origens: u64,
    /// Quantos documentos distintos vieram do Gmail.
    pub documentos: u64,
    /// A categoria da última falha, sem conteúdo.
    pub ultima_falha: Option<String>,
}

/// O estado do Gmail neste cofre.
#[must_use]
pub fn estado_do_gmail(cofre: &Path) -> EstadoDaConexao {
    let faltando: Vec<String> = fonte::faltando().into_iter().map(str::to_owned).collect();
    let mut e = EstadoDaConexao {
        situacao: SituacaoDaConexao::SemCredenciais,
        faltando,
        ultima_coleta: None,
        consulta: None,
        origens: 0,
        documentos: 0,
        ultima_falha: None,
    };
    let banco = cofre.join("inventario.sqlite");
    if let Ok(conn) = Connection::open_with_flags(&banco, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        let _ = ler(&conn, &mut e);
    }
    e.situacao = if !e.faltando.is_empty() {
        SituacaoDaConexao::SemCredenciais
    } else if e.ultima_falha.is_some() {
        SituacaoDaConexao::ComFalha
    } else if e.ultima_coleta.is_some() {
        SituacaoDaConexao::Coletada
    } else {
        SituacaoDaConexao::Pronta
    };
    e
}

fn ler(conn: &Connection, e: &mut EstadoDaConexao) -> rusqlite::Result<()> {
    e.origens = conn.query_row(
        "SELECT count(*) FROM origem WHERE fonte_id = ?1",
        [FONTE],
        |l| l.get::<_, i64>(0),
    )? as u64;
    e.documentos = conn.query_row(
        "SELECT count(DISTINCT content_hash) FROM origem WHERE fonte_id = ?1",
        [FONTE],
        |l| l.get::<_, i64>(0),
    )? as u64;
    // A última varredura fechada decide: boa → coletada; falhou/interrompida → com falha.
    let ultima: Option<(String, Option<String>, String)> = conn
        .query_row(
            "SELECT status, concluida_em, escopo FROM varredura
              WHERE fonte_id = ?1 AND status <> 'rodando' ORDER BY id DESC LIMIT 1",
            [FONTE],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .optional()?;
    if let Some((status, quando, escopo)) = ultima {
        e.consulta = Some(escopo);
        if status == "concluida" {
            e.ultima_coleta = quando;
        } else {
            e.ultima_falha = Some(match status.as_str() {
                "interrompida" => "a última coleta foi interrompida".to_owned(),
                _ => "a última coleta falhou".to_owned(),
            });
            e.ultima_coleta = conn
                .query_row(
                    "SELECT concluida_em FROM varredura
                      WHERE fonte_id = ?1 AND status = 'concluida' ORDER BY id DESC LIMIT 1",
                    [FONTE],
                    |l| l.get(0),
                )
                .optional()?
                .flatten();
        }
    }
    Ok(())
}
