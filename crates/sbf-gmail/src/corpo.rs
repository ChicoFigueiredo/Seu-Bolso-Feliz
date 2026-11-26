//! O texto de um e-mail: o corpo `text/plain`, ou o `text/html` sem as marcas quando não há
//! texto puro. Partes com nome de arquivo são anexos e ficam de fora.

use std::sync::LazyLock;

use regex::Regex;
use sbf_coletor::gmail::mensagem::{MensagemDoGmail, Parte, decodificar_base64url};

static BLOCOS_INUTEIS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(script|style)\b.*?</(script|style)>").expect("regex fixa")
});
static QUEBRAS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*(br|/p|/div|/tr|/li|/h[1-6])\b[^>]*>").expect("regex fixa")
});
static MARCAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").expect("regex fixa"));

/// O texto da mensagem, aparado. Vazio se não há corpo de texto.
#[must_use]
pub fn texto_do_corpo(m: &MensagemDoGmail) -> String {
    let mut puro = Vec::new();
    let mut html = Vec::new();
    separar(
        &m.payload.mime_type,
        "",
        m.payload.body.data.as_deref(),
        &m.payload.parts,
        &mut puro,
        &mut html,
    );
    let texto = if puro.iter().any(|t| !t.trim().is_empty()) {
        puro.join("\n")
    } else {
        html.iter()
            .map(|h| html_para_texto(h))
            .collect::<Vec<_>>()
            .join("\n")
    };
    aparar_linhas(&texto)
}

fn separar(
    mime: &str,
    nome: &str,
    dados: Option<&str>,
    filhas: &[Parte],
    puro: &mut Vec<String>,
    html: &mut Vec<String>,
) {
    if nome.is_empty()
        && let Some(d) = dados.filter(|d| !d.is_empty())
        && let Ok(bytes) = decodificar_base64url(d)
    {
        let texto = String::from_utf8_lossy(&bytes).into_owned();
        match mime.to_lowercase().as_str() {
            "text/plain" => puro.push(texto),
            "text/html" => html.push(texto),
            _ => {}
        }
    }
    for f in filhas {
        separar(
            &f.mime_type,
            &f.filename,
            f.body.data.as_deref(),
            &f.parts,
            puro,
            html,
        );
    }
}

/// HTML para texto: tira script e estilo, vira quebra o que é fim de bloco e some com as marcas.
#[must_use]
pub fn html_para_texto(html: &str) -> String {
    let sem_blocos = BLOCOS_INUTEIS.replace_all(html, " ");
    let com_quebras = QUEBRAS.replace_all(&sem_blocos, "\n");
    let sem_marcas = MARCAS.replace_all(&com_quebras, " ");
    sem_marcas
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn aparar_linhas(texto: &str) -> String {
    let mut saida: Vec<String> = Vec::new();
    for linha in texto.replace('\r', "").lines() {
        let l = linha.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() && saida.last().is_none_or(String::is_empty) {
            continue;
        }
        saida.push(l);
    }
    saida.join("\n").trim().to_owned()
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    fn b64(t: &str) -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(t.as_bytes())
    }

    fn mensagem(payload: serde_json::Value) -> MensagemDoGmail {
        serde_json::from_value(json!({"id": "m1", "payload": payload})).expect("mensagem")
    }

    #[test]
    fn prefere_o_texto_puro_e_ignora_anexos() {
        let m = mensagem(json!({
            "mimeType": "multipart/mixed", "body": {"size": 0},
            "parts": [
                {"mimeType": "multipart/alternative", "body": {"size": 0}, "parts": [
                    {"mimeType": "text/plain", "body": {"size": 5, "data": b64("Seu boleto vence dia 24.")}},
                    {"mimeType": "text/html", "body": {"size": 5, "data": b64("<p>outra coisa</p>")}}
                ]},
                {"mimeType": "text/plain", "filename": "notas.txt", "body": {"attachmentId": "a1", "data": b64("de anexo")}}
            ]
        }));
        assert_eq!(texto_do_corpo(&m), "Seu boleto vence dia 24.");
    }

    #[test]
    fn so_html_vira_texto_sem_marcas() {
        let m = mensagem(json!({
            "mimeType": "text/html",
            "body": {"size": 5, "data": b64("<style>p{color:red}</style><p>Fatura&nbsp;aberta</p><br>R$ 10 &amp; mais</p>")}
        }));
        assert_eq!(texto_do_corpo(&m), "Fatura aberta\n\nR$ 10 & mais");
    }

    #[test]
    fn sem_corpo_e_vazio() {
        let m = mensagem(json!({"mimeType": "multipart/mixed", "body": {"size": 0}, "parts": []}));
        assert_eq!(texto_do_corpo(&m), "");
    }
}
