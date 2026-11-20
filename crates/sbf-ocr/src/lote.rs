//! As imagens novas do inventário, em lote: o que o worker faz a cada volta.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;

use crate::achado::{DesfechoDoOcr, ocr_do_achado};
use crate::leitura::{EXTENSOES_DE_IMAGEM, LIMITE_DA_IMAGEM, Ocr};

/// Quantas imagens uma volta do worker lê (cada uma leva de alguns décimos a alguns
/// segundos; o resto fica para a próxima volta).
pub const LOTE_PADRAO: usize = 10;

/// A espera, em segundos, antes de tentar de novo uma imagem que falhou: depois da 1ª falha,
/// da 2ª e da 3ª. Na 4ª o OCR desiste dela (só o "Reconhecer de novo" manual a lê).
pub const ESPERAS_APOS_FALHA: [u64; 3] = [3_600, 6 * 3_600, 24 * 3_600];

/// A tabela das falhas. O worker a cria sozinha (`IF NOT EXISTS`): é estado de operação, não
/// dado do usuário, e assim não precisa subir a versão do esquema (um banco migrado não abre
/// em binário antigo).
const CRIAR_TABELA_DE_FALHAS: &str = "CREATE TABLE IF NOT EXISTS ocr_falha (
    content_hash    TEXT PRIMARY KEY,
    tentativas      INTEGER NOT NULL DEFAULT 1,
    ultima_falha_em INTEGER NOT NULL,
    causa           TEXT NOT NULL
)";

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
    /// A causa mais comum das falhas do lote (uma palavra, sem caminho nem conteúdo).
    pub causa: Option<&'static str>,
}

/// Lê até `lote` imagens (png, jpg, jpeg, webp) de até 15 MiB que o inventário conhece e
/// ainda não têm texto, as mais novas primeiro, e grava com o método `ocr-tesseract`. O
/// arquivo é o original (`origem.local_filepath`) se ainda existe, senão a cópia no acervo
/// do `cofre`.
///
/// A imagem que falha fica em `ocr_falha` (tentativas, instante e causa) e só volta a ser
/// tentada depois da espera de [`ESPERAS_APOS_FALHA`]; na 4ª falha o OCR desiste dela. `agora`
/// é em segundos desde 1970. Texto gravado (inclusive pelo "Reconhecer de novo") tira a
/// imagem da fila.
///
/// # Errors
/// Falha do SQLite. Falha de OCR não é erro: vira contagem.
pub fn ler_imagens_novas(
    conexao: &Connection,
    cofre: &Path,
    ocr: &Ocr,
    lote: usize,
    agora: u64,
    cancelar: &AtomicBool,
) -> rusqlite::Result<ResumoDoLote> {
    let mut resumo = ResumoDoLote::default();
    if !ocr.disponivel() {
        resumo.indisponivel = true;
        return Ok(resumo);
    }
    conexao.execute_batch(CRIAR_TABELA_DE_FALHAS)?;
    let agora = i64::try_from(agora).unwrap_or(i64::MAX);
    let mut causas: Vec<(&'static str, usize)> = Vec::new();
    let mut pendentes = 0;
    let mut feitas = 0;
    for c in candidatas(conexao, lote, agora)? {
        if feitas >= lote || cancelar.load(Ordering::Relaxed) {
            pendentes += 1;
            continue;
        }
        feitas += 1;
        let Some(caminho) = onde_esta(cofre, &c) else {
            registrar_falha(conexao, &c.sha, agora, "sem_arquivo", &mut causas)?;
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
            DesfechoDoOcr::Falhou(f) => {
                registrar_falha(conexao, &c.sha, agora, f.causa(), &mut causas)?;
                resumo.falhas += 1;
            }
        }
    }
    resumo.pendentes = pendentes;
    resumo.causa = causas.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c);
    Ok(resumo)
}

/// Grava a falha (mais uma tentativa) e a conta entre as causas do lote.
fn registrar_falha(
    conexao: &Connection,
    sha: &str,
    agora: i64,
    causa: &'static str,
    causas: &mut Vec<(&'static str, usize)>,
) -> rusqlite::Result<()> {
    conexao.execute(
        "INSERT INTO ocr_falha (content_hash, tentativas, ultima_falha_em, causa)
         VALUES (?1, 1, ?2, ?3)
         ON CONFLICT(content_hash) DO UPDATE SET
           tentativas = tentativas + 1,
           ultima_falha_em = excluded.ultima_falha_em,
           causa = excluded.causa",
        rusqlite::params![sha, agora, causa],
    )?;
    match causas.iter_mut().find(|(c, _)| *c == causa) {
        Some((_, n)) => *n += 1,
        None => causas.push((causa, 1)),
    }
    Ok(())
}

struct Candidata {
    sha: String,
    acervo: Option<String>,
    original: Option<String>,
}

fn candidatas(conexao: &Connection, limite: usize, agora: i64) -> rusqlite::Result<Vec<Candidata>> {
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
            AND NOT EXISTS (
                  SELECT 1 FROM ocr_falha f
                   WHERE f.content_hash = a.content_hash
                     AND (f.tentativas > {max}
                          OR f.ultima_falha_em + CASE f.tentativas
                               WHEN 1 THEN {e1} WHEN 2 THEN {e2} ELSE {e3} END > ?3))
          ORDER BY a.primeiro_visto_em DESC, a.content_hash
          LIMIT ?2",
        max = ESPERAS_APOS_FALHA.len(),
        e1 = ESPERAS_APOS_FALHA[0],
        e2 = ESPERAS_APOS_FALHA[1],
        e3 = ESPERAS_APOS_FALHA[2],
    );
    let mut stmt = conexao.prepare(&sql)?;
    let maximo = i64::try_from(LIMITE_DA_IMAGEM).unwrap_or(i64::MAX);
    let teto = i64::try_from(limite.saturating_add(1)).unwrap_or(i64::MAX);
    stmt.query_map(rusqlite::params![maximo, teto, agora], |l| {
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
