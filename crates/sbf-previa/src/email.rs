//! E-mail `.eml` pelo `mail-parser`: remetente, destinatário, assunto, data (como veio no
//! cabeçalho), corpo em texto (o HTML vira texto) e anexos com o sha256 do conteúdo.

use crate::texto::cortar;
use crate::{Anexo, Email, ErroDaPrevia, TEXTO_MAXIMO};
use mail_parser::{Address, MessageParser, MimeHeaders};
use sha2::{Digest, Sha256};

/// Os endereços como "Nome <endereço>", separados por vírgula.
fn enderecos(endereco: Option<&Address<'_>>) -> String {
    let Some(endereco) = endereco else {
        return String::new();
    };
    endereco
        .iter()
        .map(|a| match (a.name(), a.address()) {
            (Some(nome), Some(end)) if !nome.is_empty() => format!("{nome} <{end}>"),
            (_, Some(end)) => end.to_owned(),
            (Some(nome), None) => nome.to_owned(),
            (None, None) => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// O sha256 em hexadecimal minúsculo (o formato do `content_hash`).
pub(crate) fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let resumo = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for b in resumo {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

pub(crate) fn abrir(bytes: &[u8]) -> Result<Email, ErroDaPrevia> {
    let mensagem = MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| ErroDaPrevia::Ilegivel("não é um e-mail legível".to_owned()))?;
    let corpo = mensagem
        .body_text(0)
        .map(|c| c.into_owned())
        .unwrap_or_default();
    let anexos = mensagem
        .attachments()
        .enumerate()
        .map(|(i, parte)| {
            let conteudo = parte.contents();
            Anexo {
                nome: parte
                    .attachment_name()
                    .map_or_else(|| format!("anexo-{}", i + 1), str::to_owned),
                bytes: conteudo.len() as u64,
                sha256: Some(sha256(conteudo)),
            }
        })
        .collect();
    Ok(Email {
        de: enderecos(mensagem.from()),
        para: enderecos(mensagem.to()),
        assunto: mensagem.subject().unwrap_or_default().to_owned(),
        data: mensagem
            .header_raw("Date")
            .map(str::trim)
            .unwrap_or_default()
            .to_owned(),
        corpo: cortar(corpo, TEXTO_MAXIMO),
        anexos,
    })
}
