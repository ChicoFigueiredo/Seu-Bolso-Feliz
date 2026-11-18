//! A biblioteca de regras de reconhecimento de fábrica: documentos financeiros brasileiros
//! comuns (CAIXA Habitação, Pix, boleto, fatura de cartão, contas de consumo, recibos).
//!
//! Cada regra é um arquivo em `fabrica/biblioteca/<id>.json`:
//!
//! ```json
//! { "regra": "...", "nome": "...", "motivo": "...",
//!   "definicao": { ... }, "exemplos": [ { "nome": "...", "texto": "...", "esperado": { ... } } ] }
//! ```
//!
//! Quando há cálculo ou texto a montar, o script Luau mora ao lado (`<id>.luau`) e o sbf
//! cola `_comum.luau` (funções de dinheiro, data e linhas) antes dele. Todas entram como
//! **rascunho**: quem ativa é o usuário, depois de rodar nos próprios documentos. Os
//! exemplos são todos fictícios ("Fulano de Tal", contrato `000000000000-0`).
//!
//! A conferência de que toda regra valida e todo exemplo passa roda em
//! `crates/sbf-script/tests/biblioteca.rs`, que tem o executor de Luau.

use serde::Deserialize;

use crate::catalogo::{Esperado, NovaVersao};
use crate::definicao::Definicao;
use crate::{Autor, Canal, Dicas, Entrada, Estado, Origem, TipoDeEntrada, VERSAO_DO_ESQUEMA};

/// As funções que o sbf cola antes de cada script da biblioteca.
pub const COMUM: &str = include_str!("../fabrica/biblioteca/_comum.luau");

macro_rules! regra_json {
    ($id:literal) => {
        include_str!(concat!("../fabrica/biblioteca/", $id, ".json"))
    };
}

macro_rules! luau {
    ($id:literal) => {
        Some(include_str!(concat!(
            "../fabrica/biblioteca/",
            $id,
            ".luau"
        )))
    };
}

/// O JSON de cada regra e o corpo do script, se houver.
const ARQUIVOS: &[(&str, Option<&str>)] = &[
    (
        regra_json!("caixa-habitacao-comprovante-prestacao"),
        luau!("caixa-habitacao-comprovante-prestacao"),
    ),
    (
        regra_json!("caixa-habitacao-prestacoes-pagas"),
        luau!("caixa-habitacao-prestacoes-pagas"),
    ),
    (
        regra_json!("caixa-habitacao-fgts"),
        luau!("caixa-habitacao-fgts"),
    ),
    (regra_json!("comprovante-pix-caixa"), None),
    (regra_json!("comprovante-pix-c6"), None),
    (
        regra_json!("comprovante-pix-nubank"),
        luau!("data-do-texto"),
    ),
    (regra_json!("boleto"), luau!("boleto")),
    (regra_json!("fatura-cartao-c6"), None),
    (regra_json!("fatura-cartao-caixa"), None),
    (
        regra_json!("fatura-cartao-nubank"),
        luau!("vencimento-do-texto"),
    ),
    (regra_json!("conta-neoenergia"), None),
    (regra_json!("conta-vivo"), None),
    (regra_json!("conta-canaa"), None),
    (regra_json!("recibo-uber"), luau!("recibo-uber")),
    (regra_json!("invoice-anthropic"), luau!("invoice-em-ingles")),
    (regra_json!("invoice-openai"), luau!("invoice-em-ingles")),
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExemploDoArquivo {
    nome: String,
    texto: String,
    /// `application/pdf` por padrão; `image/png` para print.
    #[serde(default)]
    mime: Option<String>,
    esperado: Esperado,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArquivoDeRegra {
    regra: String,
    nome: String,
    motivo: String,
    definicao: Definicao,
    exemplos: Vec<ExemploDoArquivo>,
}

/// Lê um arquivo embutido. Eles são constantes do binário e o teste da biblioteca confere
/// cada um; um arquivo quebrado é erro de quem editou, não de quem usa.
#[allow(clippy::expect_used)]
fn ler(json: &str) -> ArquivoDeRegra {
    serde_json::from_str(json).expect("regra da biblioteca de fábrica inválida")
}

fn script_completo(corpo: Option<&str>) -> Option<String> {
    corpo.map(|c| format!("{COMUM}\n{c}"))
}

/// As regras da biblioteca, todas em rascunho.
pub fn regras() -> Vec<(NovaVersao, Estado)> {
    ARQUIVOS
        .iter()
        .map(|(json, corpo)| {
            let a = ler(json);
            (
                NovaVersao {
                    regra: a.regra,
                    nome: a.nome,
                    definicao: a.definicao,
                    script: script_completo(*corpo),
                    autor: Autor::Fabrica,
                    motivo: a.motivo,
                },
                Estado::Rascunho,
            )
        })
        .collect()
}

/// Os exemplos fictícios de cada regra: `(regra, nome do exemplo, entrada, esperado)`.
pub fn exemplos() -> Vec<(String, String, Entrada, Esperado)> {
    let mut saida = Vec::new();
    for (json, _) in ARQUIVOS {
        let a = ler(json);
        for e in a.exemplos {
            let entrada = Entrada {
                versao_do_esquema: VERSAO_DO_ESQUEMA,
                id: format!("exemplo-{}-{}", a.regra, e.nome),
                tipo: TipoDeEntrada::Documento,
                origem: Origem {
                    canal: Canal::Arrastado,
                    id_externo: None,
                },
                sha256: None,
                mime: Some(e.mime.unwrap_or_else(|| "application/pdf".to_owned())),
                nome: Some(format!("{}.pdf", a.regra)),
                texto: Some(e.texto),
                dicas: Dicas::default(),
                momento: None,
            };
            saida.push((a.regra.clone(), e.nome, entrada, e.esperado));
        }
    }
    saida
}
