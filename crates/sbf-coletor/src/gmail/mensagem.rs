//! A mensagem do Gmail e o que se tira dela (`message-processor.ts`, `job-requires.ts` e os
//! auxiliares de `index.ts`): cabeçalhos, anexos aceitos, exigências do job, nome seguro de
//! arquivo, data do cabeçalho `Date`.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use unicode_normalization::UnicodeNormalization;

use crate::erro::ErroDeColeta;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Cabecalho {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Corpo {
    pub attachment_id: Option<String>,
    #[serde(default)]
    pub size: i64,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Parte {
    #[serde(default)]
    pub part_id: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub headers: Vec<Cabecalho>,
    #[serde(default)]
    pub body: Corpo,
    #[serde(default)]
    pub parts: Vec<Parte>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Carga {
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub headers: Vec<Cabecalho>,
    #[serde(default)]
    pub body: Corpo,
    #[serde(default)]
    pub parts: Vec<Parte>,
}

/// `GET /messages/{id}?format=full`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MensagemDoGmail {
    pub id: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
    #[serde(default)]
    pub internal_date: String,
    #[serde(default)]
    pub payload: Carga,
}

/// Os metadados de uma mensagem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadados {
    pub id_da_mensagem: String,
    pub id_da_conversa: String,
    pub assunto: String,
    pub de: String,
    pub para: String,
    pub data: String,
    /// Milissegundos desde a época, como o Gmail devolve.
    pub data_interna: String,
    pub rotulos: Vec<String>,
}

/// Um anexo aceito para ingestão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anexo {
    pub id_da_parte: String,
    pub id_do_anexo: String,
    pub nome: String,
    pub tipo_mime: String,
    pub tamanho: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MensagemProcessada {
    pub metadados: Metadados,
    pub anexos: Vec<Anexo>,
}

/// Os formatos que o pipeline abre (`FORMATOS` de `@sbf/contracts`): extensão, MIME canônico
/// e MIMEs alternativos vistos na prática.
const FORMATOS: [(&str, &str, &[&str]); 9] = [
    (".pdf", "application/pdf", &[]),
    (".csv", "text/csv", &["text/plain", "application/csv"]),
    (
        ".xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        &[],
    ),
    (
        ".ofx",
        "application/x-ofx",
        &["application/octet-stream", "text/plain"],
    ),
    (".xml", "application/xml", &["text/xml"]),
    (".png", "image/png", &[]),
    (".jpg", "image/jpeg", &[]),
    (".jpeg", "image/jpeg", &[]),
    (".webp", "image/webp", &[]),
];

/// As extensões cujo suporte é só por OCR (`suporte: "ocr"` no registro).
const EXTENSOES_DE_IMAGEM: [&str; 4] = [".png", ".jpg", ".jpeg", ".webp"];

fn extensao_de(nome: &str) -> String {
    nome.rfind('.')
        .map(|i| nome[i..].to_lowercase())
        .unwrap_or_default()
}

fn mime_aceito(mime: &str) -> bool {
    FORMATOS
        .iter()
        .any(|(_, canonico, alternativos)| *canonico == mime || alternativos.contains(&mime))
}

fn extensao_aceita(ext: &str) -> bool {
    FORMATOS.iter().any(|(e, _, _)| *e == ext)
}

fn cabecalho<'a>(m: &'a MensagemDoGmail, nome: &str) -> &'a str {
    m.payload
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(nome))
        .map_or("", |h| h.value.as_str())
}

fn anexo_aceito(p: &Parte) -> bool {
    if p.filename.is_empty() || p.body.attachment_id.is_none() {
        return false;
    }
    if mime_aceito(&p.mime_type) {
        return true;
    }
    // Reserva: a extensão do nome. O Bun só olha quando há um ponto no nome.
    p.filename.rfind('.').is_some() && extensao_aceita(&extensao_de(&p.filename))
}

fn achar_anexos(partes: &[Parte], saida: &mut Vec<Anexo>) {
    for p in partes {
        if anexo_aceito(p)
            && let Some(id) = &p.body.attachment_id
        {
            saida.push(Anexo {
                id_da_parte: p.part_id.clone(),
                id_do_anexo: id.clone(),
                nome: p.filename.clone(),
                tipo_mime: p.mime_type.clone(),
                tamanho: p.body.size,
            });
        }
        // Recursão para multipart aninhado.
        achar_anexos(&p.parts, saida);
    }
}

/// Extrai metadados e a lista de anexos aceitos.
pub fn processar(m: &MensagemDoGmail) -> MensagemProcessada {
    let metadados = Metadados {
        id_da_mensagem: m.id.clone(),
        id_da_conversa: m.thread_id.clone(),
        assunto: cabecalho(m, "Subject").to_owned(),
        de: cabecalho(m, "From").to_owned(),
        para: cabecalho(m, "To").to_owned(),
        data: cabecalho(m, "Date").to_owned(),
        data_interna: m.internal_date.clone(),
        rotulos: m.label_ids.clone(),
    };
    let mut anexos = Vec::new();
    achar_anexos(&m.payload.parts, &mut anexos);
    MensagemProcessada { metadados, anexos }
}

/// Decodifica base64url do Gmail (com ou sem `=` no fim).
pub fn decodificar_base64url(texto: &str) -> Result<Vec<u8>, ErroDeColeta> {
    URL_SAFE_NO_PAD
        .decode(texto.trim_end_matches('='))
        .map_err(|_| ErroDeColeta::Resposta {
            servico: "gmail",
            detalhe: "anexo com base64url inválido".to_owned(),
        })
}

/// `precisaDeOcr`/`resolveAttachmentRequires`: PDF e imagem exigem `ocr`; o resto, nada.
/// Um PDF por e-mail tanto é o boleto com camada de texto quanto o comprovante escaneado, e o
/// coletor não abre o arquivo para descobrir qual: exigir OCR erra para o lado seguro.
pub fn exigencias_do_anexo(nome: &str, tipo_mime: &str) -> Vec<String> {
    let mime = tipo_mime
        .to_lowercase()
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_owned();
    let ext = extensao_de(nome);
    let precisa = mime == "application/pdf"
        || mime.starts_with("image/")
        || ext == ".pdf"
        || EXTENSOES_DE_IMAGEM.contains(&ext.as_str());
    if precisa {
        vec!["ocr".to_owned()]
    } else {
        Vec::new()
    }
}

/// Sanitiza o nome para chave de objeto: tira acentos, troca o que não é `[A-Za-z0-9._-]` por
/// `_`, colapsa `_` repetidos e apara `_` das pontas (`sanitizeFilename`).
pub fn nome_seguro(nome: &str) -> String {
    let sem_acento: String = nome
        .nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect();
    let trocado: String = sem_acento
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut colapsado = String::with_capacity(trocado.len());
    for c in trocado.chars() {
        if c == '_' && colapsado.ends_with('_') {
            continue;
        }
        colapsado.push(c);
    }
    colapsado.trim_matches('_').to_owned()
}

/// `parseEmailDate`: o cabeçalho `Date` (RFC 2822, com ou sem o comentário `(UTC)` no fim)
/// em UTC; `None` se não der para ler.
pub fn data_do_cabecalho(texto: &str) -> Option<DateTime<Utc>> {
    let t = texto.trim();
    if t.is_empty() {
        return None;
    }
    let limpo = match t.rfind('(') {
        Some(i) if t.ends_with(')') => t[..i].trim(),
        _ => t,
    };
    DateTime::parse_from_rfc2822(limpo)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn mensagem(partes: serde_json::Value) -> MensagemDoGmail {
        serde_json::from_value(json!({
            "id": "m1", "threadId": "t1", "labelIds": ["Label_1"], "internalDate": "1773766000000",
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "subject", "value": "Boleto de março"},
                    {"name": "From", "value": "Cobranca <cobranca@exemplo.test>"},
                    {"name": "TO", "value": "dono@exemplo.test"},
                    {"name": "Date", "value": "Tue, 17 Mar 2026 17:06:02 +0000 (UTC)"}
                ],
                "body": {"size": 0},
                "parts": partes
            }
        }))
        .expect("mensagem")
    }

    #[test]
    fn cabecalhos_sem_diferenciar_caixa_e_anexos_recursivos() {
        let m = mensagem(json!([
            {"partId": "0", "mimeType": "text/plain", "filename": "", "body": {"size": 5, "data": "YQ"}},
            {"partId": "1", "mimeType": "application/pdf", "filename": "boleto.pdf", "body": {"attachmentId": "a1", "size": 100}},
            {"partId": "2", "mimeType": "multipart/related", "filename": "", "body": {"size": 0}, "parts": [
                {"partId": "2.1", "mimeType": "application/octet-stream", "filename": "extrato.OFX", "body": {"attachmentId": "a2", "size": 50}},
                {"partId": "2.2", "mimeType": "image/gif", "filename": "logo.gif", "body": {"attachmentId": "a3", "size": 5}},
                {"partId": "2.3", "mimeType": "application/zip", "filename": "foto.JPG", "body": {"attachmentId": "a4", "size": 5}}
            ]},
            {"partId": "3", "mimeType": "application/pdf", "filename": "sem-id.pdf", "body": {"size": 5}}
        ]));
        let p = processar(&m);
        assert_eq!(p.metadados.assunto, "Boleto de março");
        assert_eq!(p.metadados.para, "dono@exemplo.test");
        let nomes: Vec<_> = p.anexos.iter().map(|a| a.nome.as_str()).collect();
        // gif nao entra (MIME e extensao fora); zip com extensao .JPG entra pela extensao;
        // anexo sem attachmentId nao entra.
        assert_eq!(nomes, vec!["boleto.pdf", "extrato.OFX", "foto.JPG"]);
        assert_eq!(p.anexos[0].id_do_anexo, "a1");
    }

    #[test]
    fn exigencias_pdf_imagem_e_o_resto() {
        assert_eq!(exigencias_do_anexo("a.pdf", "application/pdf"), vec!["ocr"]);
        assert_eq!(
            exigencias_do_anexo("a.bin", "Application/PDF; name=a"),
            vec!["ocr"]
        );
        assert_eq!(
            exigencias_do_anexo("a.PDF", "application/octet-stream"),
            vec!["ocr"]
        );
        assert_eq!(
            exigencias_do_anexo("a.webp", "application/octet-stream"),
            vec!["ocr"]
        );
        assert_eq!(exigencias_do_anexo("a.png", "image/png"), vec!["ocr"]);
        assert!(exigencias_do_anexo("a.csv", "text/csv").is_empty());
        assert!(exigencias_do_anexo("a.ofx", "application/octet-stream").is_empty());
        assert!(exigencias_do_anexo("a.xlsx", "").is_empty());
    }

    #[test]
    fn nome_seguro_do_bun() {
        assert_eq!(
            nome_seguro("Comprovante Março (1).pdf"),
            "Comprovante_Marco_1_.pdf"
        );
        assert_eq!(nome_seguro("  __a  b__.pdf"), "a_b_.pdf");
        assert_eq!(nome_seguro("ação/..\\x.pdf"), "acao_.._x.pdf");
        assert_eq!(nome_seguro("___"), "");
    }

    #[test]
    fn data_do_cabecalho_com_e_sem_comentario() {
        let a = data_do_cabecalho("Tue, 17 Mar 2026 17:06:02 +0000 (UTC)").expect("data");
        assert_eq!(a.to_rfc3339(), "2026-03-17T17:06:02+00:00");
        let b = data_do_cabecalho("Mon, 26 Jan 2026 04:53:01 -0800 (PST)").expect("data");
        assert_eq!(b.to_rfc3339(), "2026-01-26T12:53:01+00:00");
        assert!(data_do_cabecalho("").is_none());
        assert!(data_do_cabecalho("ontem").is_none());
    }

    #[test]
    fn base64url_com_e_sem_preenchimento() {
        assert_eq!(decodificar_base64url("aGVsbG8").expect("ok"), b"hello");
        assert_eq!(decodificar_base64url("aGVsbG8=").expect("ok"), b"hello");
        assert_eq!(decodificar_base64url("_-8").expect("ok"), vec![0xff, 0xef]);
        assert!(decodificar_base64url("***").is_err());
    }
}
