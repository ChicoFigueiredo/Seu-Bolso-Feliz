//! Zip: a lista do diretório central, sem extrair nada.

use crate::{EntradaDoZip, ErroDaPrevia};
use std::path::Path;

/// O máximo de entradas listadas (um zip hostil pode declarar milhões).
const ENTRADAS_MAXIMAS: usize = 10_000;

pub(crate) fn listar(caminho: &Path) -> Result<Vec<EntradaDoZip>, ErroDaPrevia> {
    let arquivo = std::fs::File::open(caminho).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ErroDaPrevia::SemArquivo,
        _ => ErroDaPrevia::Ilegivel(e.to_string()),
    })?;
    if arquivo.metadata().map_or(0, |m| m.len()) > crate::TAMANHO_MAXIMO {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(arquivo))
        .map_err(|e| ErroDaPrevia::Ilegivel(format!("zip: {e}")))?;
    let total = zip.len();
    let mut entradas = Vec::with_capacity(total.min(ENTRADAS_MAXIMAS));
    for i in 0..total.min(ENTRADAS_MAXIMAS) {
        let entrada = zip
            .by_index_raw(i)
            .map_err(|e| ErroDaPrevia::Ilegivel(format!("zip: {e}")))?;
        entradas.push(EntradaDoZip {
            nome: entrada.name().map_or_else(
                |_| String::from_utf8_lossy(entrada.name_raw()).into_owned(),
                |n| n.into_owned(),
            ),
            bytes: entrada.size(),
        });
    }
    if total > ENTRADAS_MAXIMAS {
        entradas.push(EntradaDoZip {
            nome: format!("[…] e mais {} entradas", total - ENTRADAS_MAXIMAS),
            bytes: 0,
        });
    }
    Ok(entradas)
}
