//! As regras que vêm com o sbf (D10).
//!
//! - `contracheque`: **ativa**, comportamento idêntico ao de hoje (o leitor nativo
//!   `sbf_planilha::contracheque::ler`, chamado por `sbf-reconhecimento` quando o texto
//!   normalizado contém `demonstrativo de pagamento`). Aqui só a detecção e o nome do
//!   leitor; quem chama roda o leitor e devolve os campos a `Motor::completar`.
//! - `contracheque` versão 2: **rascunho** (D10): a mesma detecção e o mesmo leitor, mais o
//!   script que confere o IRRF contra o parâmetro `irrf` (D8). Ativar é decisão do usuário.
//! - `pix-na-descricao`: **rascunho**; lê o favorecido e o tipo na descrição de um
//!   registro do banco (D13).

use std::collections::BTreeMap;

use crate::catalogo::{Esperado, NovaVersao};
use crate::definicao::{Condicao, Definicao, Extracao, FonteDaExtracao, Formato};
use crate::{Autor, Canal, Dicas, Entrada, Estado, Origem, TipoDeEntrada, VERSAO_DO_ESQUEMA};

/// O script do IRRF (conferência da retenção; só confere, nunca altera a rubrica).
pub const SCRIPT_IRRF: &str = include_str!("../fabrica/irrf.luau");

/// A tabela oficial do IRRF de 2026 (faixas e redutor), parâmetro `irrf`.
pub const TABELA_IRRF_2026_01: &str = include_str!("../fabrica/irrf-2026-01.json");

/// O nome do parâmetro da tabela do IRRF.
pub const PARAMETRO_IRRF: &str = "irrf";

/// Um parâmetro que vem com o sbf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParametroDeFabrica {
    pub nome: &'static str,
    /// `AAAA-MM`.
    pub vigencia_inicio: &'static str,
    pub valor: &'static str,
    pub fonte: &'static str,
}

/// Os parâmetros de fábrica: a tabela do IRRF de 2026, com a fonte oficial.
pub fn parametros_de_fabrica() -> Vec<ParametroDeFabrica> {
    vec![ParametroDeFabrica {
        nome: PARAMETRO_IRRF,
        vigencia_inicio: "2026-01",
        valor: TABELA_IRRF_2026_01,
        fonte: "https://www.gov.br/receitafederal/pt-br/assuntos/meu-imposto-de-renda/tabelas/2026",
    }]
}

/// O exemplo fictício do contracheque: o título basta para a regra reconhecer; os campos
/// vêm do leitor nativo (`Motor::completar`).
pub fn exemplos_de_fabrica() -> Vec<(&'static str, &'static str, Entrada, Esperado)> {
    let entrada = Entrada {
        versao_do_esquema: VERSAO_DO_ESQUEMA,
        id: "exemplo-contracheque-ficticio".to_owned(),
        tipo: TipoDeEntrada::Documento,
        origem: Origem {
            canal: Canal::Arrastado,
            id_externo: None,
        },
        sha256: None,
        mime: Some("application/pdf".to_owned()),
        nome: Some("contracheque-ficticio.pdf".to_owned()),
        texto: Some(
            "Fulano de Tal Exemplo\nDEMONSTRATIVO DE PAGAMENTO\nSalario Liquido R$ 1.000,00\n"
                .to_owned(),
        ),
        dicas: Dicas::default(),
        momento: None,
    };
    vec![(
        "contracheque",
        "contracheque-ficticio",
        entrada,
        Esperado::Reconhece {
            tipo_documental: "contracheque".to_owned(),
            campos: BTreeMap::new(),
        },
    )]
}

/// As regras de fábrica e o estado em que cada uma deve ficar.
pub fn regras_de_fabrica() -> Vec<(NovaVersao, Estado)> {
    let mut contracheque = Definicao::simples("contracheque", "demonstrativo de pagamento");
    contracheque.leitor = Some("contracheque".to_owned());

    // A versão 2: o mesmo leitor, mais a conferência do IRRF (rascunho: D10).
    let mut contracheque_v2 = contracheque.clone();
    contracheque_v2.parametros = vec![PARAMETRO_IRRF.to_owned()];

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
                regra: "contracheque".to_owned(),
                nome: "Contracheque (demonstrativo de pagamento)".to_owned(),
                definicao: contracheque_v2,
                script: Some(SCRIPT_IRRF.to_owned()),
                autor: Autor::Fabrica,
                motivo: "regra de fábrica: confere o IRRF retido contra a tabela oficial \
                         (só alerta, nunca altera a rubrica)"
                    .to_owned(),
            },
            Estado::Rascunho,
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
