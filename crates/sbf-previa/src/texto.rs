//! Texto (xml, ofx, ofc, html, csv, txt, json): os primeiros [`TEXTO_MAXIMO`] bytes, em
//! UTF-8 ou, se não for, Latin-1 (o OFX dos bancos brasileiros costuma vir assim). O HTML
//! vira texto simples.

use crate::{ErroDaPrevia, TEXTO_MAXIMO};
use std::io::Read;
use std::path::Path;

/// Corta `texto` em no máximo `limite` bytes, numa fronteira de caractere, marcando o corte.
pub(crate) fn cortar(mut texto: String, limite: usize) -> String {
    if texto.len() <= limite {
        return texto;
    }
    let mut fim = limite;
    while !texto.is_char_boundary(fim) {
        fim -= 1;
    }
    texto.truncate(fim);
    texto.push_str("\n[…] (cortado na prévia)");
    texto
}

/// UTF-8 (sem BOM; um caractere partido no fim do corte é descartado) ou Latin-1.
pub(crate) fn decodificar(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(t) => t.to_owned(),
        // Só faltou o fim de um caractere (o corte caiu no meio dele): é UTF-8.
        Err(e) if e.error_len().is_none() => {
            String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned()
        }
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

pub(crate) fn abrir(caminho: &Path, html: bool) -> Result<String, ErroDaPrevia> {
    let arquivo = std::fs::File::open(caminho).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ErroDaPrevia::SemArquivo,
        _ => ErroDaPrevia::Ilegivel(e.to_string()),
    })?;
    let tamanho = arquivo.metadata().map_or(0, |m| m.len());
    if tamanho > crate::TAMANHO_MAXIMO {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    let mut bytes = Vec::with_capacity(TEXTO_MAXIMO.min(tamanho as usize));
    arquivo
        .take(TEXTO_MAXIMO as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| ErroDaPrevia::Ilegivel(e.to_string()))?;
    let mut texto = decodificar(&bytes);
    if html {
        texto = mail_parser::decoders::html::html_to_text(&texto);
    }
    if tamanho > TEXTO_MAXIMO as u64 {
        texto.push_str("\n[…] (a prévia mostra só o começo do arquivo)");
    }
    Ok(texto)
}
