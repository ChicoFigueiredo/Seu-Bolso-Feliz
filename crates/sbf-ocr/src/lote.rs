//! As imagens novas do inventário, em lote: o que o worker faz a cada volta.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;

use crate::achado::{DesfechoDoOcr, ocr_do_achado};
use crate::leitura::{EXTENSOES_DE_IMAGEM, LIMITE_DA_IMAGEM, Ocr};

/// Quantas imagens uma volta do worker lê (cada uma leva de alguns décimos a alguns
/// segundos; o resto fica para a próxima volta).
pub const LOTE_PADRAO: usize = 10;

/// O que um lote fez. Só contagens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDoLote {
    /// Imagens com texto lido e gravado.
    pub lidas: usize,
    /// Imagens que o OCR leu e não tinham texto.
    pub sem_texto: usize,
    /// Imagens que não puderam ser lidas (arquivo sumido, ilegível, prazo, erro).
    pub falhas: usize,
    /// Há imagens novas esperando para a próxima volta (conta só até onde o lote enxergou).
    pub pendentes: usize,
    /// Não há tesseract: nada foi lido nem gravado.
    pub indisponivel: bool,
}

/// Lê até `lote` imagens (png, jpg, jpeg, webp) de até 15 MiB que o inventário conhece e
/// ainda não têm texto, as mais novas primeiro, e grava com o método `ocr-tesseract`. O
/// arquivo é o original (`origem.local_filepath`) se ainda existe, senão a cópia no acervo
/// do `cofre`.
///
/// `ja_falharam` guarda os achados que falharam nesta execução, para não os tentar de novo a
/// cada volta (ele não vai para o banco: reiniciar o worker tenta de novo).
///
/// # Errors
/// Falha do SQLite. Falha de OCR não é erro: vira contagem.
pub fn ler_imagens_novas(
    conexao: &Connection,
    cofre: &Path,
    ocr: &Ocr,
    lote: usize,
    ja_falharam: &mut HashSet<String>,
    cancelar: &AtomicBool,
) -> rusqlite::Result<ResumoDoLote> {
    let mut resumo = ResumoDoLote::default();
    if !ocr.disponivel() {
        resumo.indisponivel = true;
        return Ok(resumo);
    }
    let candidatas = candidatas(conexao, lote.saturating_add(ja_falharam.len()))?;
    let mut feitas = 0;
    for c in candidatas {
        if ja_falharam.contains(&c.sha) {
            continue;
        }
        if feitas >= lote {
            resumo.pendentes += 1;
            continue;
        }
        if cancelar.load(Ordering::Relaxed) {
            resumo.pendentes += 1;
            continue;
        }
        feitas += 1;
        let Some(caminho) = onde_esta(cofre, &c) else {
            ja_falharam.insert(c.sha);
            resumo.falhas += 1;
            continue;
        };
        match ocr_do_achado(conexao, &c.sha, &caminho, ocr)? {
            DesfechoDoOcr::Lido(_) => resumo.lidas += 1,
            DesfechoDoOcr::SemTexto => resumo.sem_texto += 1,
            DesfechoDoOcr::JaHavia(_) => {}
            DesfechoDoOcr::Indisponivel => {
                resumo.indisponivel = true;
                break;
            }
            _ => {
                ja_falharam.insert(c.sha);
                resumo.falhas += 1;
            }
        }
    }
    Ok(resumo)
}

struct Candidata {
    sha: String,
    acervo: Option<String>,
    original: Option<String>,
}

fn candidatas(conexao: &Connection, limite: usize) -> rusqlite::Result<Vec<Candidata>> {
    let extensoes = EXTENSOES_DE_IMAGEM
        .iter()
        .map(|e| format!("'{e}'"))
        .collect::<Vec<_>>()
        .join(", ");
    // `limite + 1`: a linha a mais diz se ainda sobra imagem depois do lote.
    let sql = format!(
        "SELECT a.content_hash, a.caminho_acervo,
                (SELECT o.local_filepath FROM origem o
                  WHERE o.content_hash = a.content_hash AND o.local_filepath IS NOT NULL
                  ORDER BY o.id DESC LIMIT 1)
           FROM achado a
          WHERE NOT EXISTS (SELECT 1 FROM texto t WHERE t.content_hash = a.content_hash)
            AND a.file_size_bytes BETWEEN 1 AND ?1
            AND lower(coalesce(a.extensao, '')) IN ({extensoes})
          ORDER BY a.primeiro_visto_em DESC, a.content_hash
          LIMIT ?2"
    );
    let mut stmt = conexao.prepare(&sql)?;
    let maximo = i64::try_from(LIMITE_DA_IMAGEM).unwrap_or(i64::MAX);
    let teto = i64::try_from(limite.saturating_add(1)).unwrap_or(i64::MAX);
    stmt.query_map(rusqlite::params![maximo, teto], |l| {
        Ok(Candidata {
            sha: l.get(0)?,
            acervo: l.get(1)?,
            original: l.get(2)?,
        })
    })?
    .collect()
}

/// O arquivo a ler: o original se existe, senão a cópia do acervo (só um caminho relativo e
/// sem `..`, dentro do cofre).
fn onde_esta(cofre: &Path, c: &Candidata) -> Option<PathBuf> {
    if let Some(o) = c.original.as_deref().map(PathBuf::from)
        && o.is_absolute()
        && o.is_file()
    {
        return Some(o);
    }
    let relativo = Path::new(c.acervo.as_deref()?);
    if relativo
        .components()
        .any(|p| !matches!(p, Component::Normal(_)))
    {
        return None;
    }
    let caminho = cofre.join(relativo);
    caminho.is_file().then_some(caminho)
}
