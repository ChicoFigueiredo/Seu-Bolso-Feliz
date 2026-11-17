//! As regras que vêm com o sbf (D10).
//!
//! - `contracheque`: **ativa**, comportamento idêntico ao de hoje (o leitor nativo
//!   `sbf_planilha::contracheque::ler`, chamado por `sbf-reconhecimento` quando o texto
//!   normalizado contém `demonstrativo de pagamento`). Aqui só a detecção e o nome do
//!   leitor; quem chama roda o leitor e devolve os campos a `Motor::completar`.
//! - `pix-na-descricao`: **rascunho**; lê o favorecido e o tipo na descrição de um
//!   registro do banco (D13).

use std::collections::BTreeMap;

use crate::catalogo::NovaVersao;
use crate::definicao::{Condicao, Definicao, Extracao, FonteDaExtracao, Formato};
use crate::{Autor, Estado, TipoDeEntrada};

/// As regras de fábrica e o estado em que cada uma deve ficar.
pub fn regras_de_fabrica() -> Vec<(NovaVersao, Estado)> {
    let mut contracheque = Definicao::simples("contracheque", "demonstrativo de pagamento");
    contracheque.leitor = Some("contracheque".to_owned());

    let mut pix = Definicao::simples("pix-na-descricao", "x");
    pix.aplica_a = vec![TipoDeEntrada::Registro];
    pix.detectar.todos = vec![Condicao::Descricao(r"(?i)\bpix\b".to_owned())];
    pix.extrair = BTreeMap::from([
        (
            "favorecido".to_owned(),
            Extracao {
                regex: r"(?i)\bpix\b(?:\s+(?:enviado|enviada|recebido|recebida))?\s*[-:]?\s*(\S.*?)\s*$"
                    .to_owned(),
                grupo: 1,
                formato: Formato::Texto,
                fonte: FonteDaExtracao::Descricao,
            },
        ),
        (
            "tipo_de_transferencia".to_owned(),
            Extracao {
                regex: r"(?i)\b(pix)\b".to_owned(),
                grupo: 1,
                formato: Formato::Minusculas,
                fonte: FonteDaExtracao::Descricao,
            },
        ),
    ]);

    vec![
        (
            NovaVersao {
                regra: "contracheque".to_owned(),
                nome: "Contracheque (demonstrativo de pagamento)".to_owned(),
                definicao: contracheque,
                script: None,
                autor: Autor::Fabrica,
                motivo: "regra de fábrica: o leitor de contracheque que o sbf já tinha".to_owned(),
            },
            Estado::Ativa,
        ),
        (
            NovaVersao {
                regra: "pix-na-descricao".to_owned(),
                nome: "Pix na descrição do banco".to_owned(),
                definicao: pix,
                script: None,
                autor: Autor::Fabrica,
                motivo: "regra de fábrica: favorecido e tipo de um Pix lidos da descrição"
                    .to_owned(),
            },
            Estado::Rascunho,
        ),
    ]
}
