//! Simulação: roda duas configurações do motor sobre as mesmas entradas, só leitura, e
//! devolve o que mudou (D12: o diff antes de ativar uma versão nova). Nada é gravado.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::motor::Motor;
use crate::{Decisao, Entrada, IdDaRegra, Valor};

/// O que o motor decidiu, em resumo curto: sem o texto do documento, sem versão nem chave
/// (a comparação é de resultado, não de identidade da versão).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decisao", rename_all = "snake_case")]
pub enum ResumoDaDecisao {
    Nenhuma,
    Reconhecido {
        regra: IdDaRegra,
        tipo_documental: String,
        campos: BTreeMap<String, Valor>,
        revisar: bool,
        propostas: usize,
    },
    Ambiguo {
        regras: Vec<IdDaRegra>,
    },
}

impl ResumoDaDecisao {
    pub fn de(decisao: &Decisao) -> Self {
        match decisao {
            Decisao::Nenhuma => Self::Nenhuma,
            Decisao::Reconhecido(r) => Self::Reconhecido {
                regra: r.regra.clone(),
                tipo_documental: r.tipo_documental.clone(),
                campos: r
                    .campos
                    .iter()
                    .map(|(n, c)| (n.clone(), c.valor.clone()))
                    .collect(),
                revisar: r.revisar,
                propostas: r.propostas.len(),
            },
            Decisao::Ambiguo { candidatas, .. } => Self::Ambiguo {
                regras: candidatas.iter().map(|c| c.regra.clone()).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mudanca {
    /// O id da entrada.
    pub entrada: String,
    pub antes: ResumoDaDecisao,
    pub depois: ResumoDaDecisao,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diferencas {
    pub iguais: usize,
    pub mudaram: Vec<Mudanca>,
}

/// Lê cada entrada com os dois motores e compara os resumos das decisões.
pub fn simular(atual: &Motor, candidato: &Motor, entradas: &[Entrada]) -> Diferencas {
    let mut d = Diferencas::default();
    for e in entradas {
        let antes = ResumoDaDecisao::de(&atual.ler(e).decisao);
        let depois = ResumoDaDecisao::de(&candidato.ler(e).decisao);
        if antes == depois {
            d.iguais += 1;
        } else {
            d.mudaram.push(Mudanca {
                entrada: e.id.clone(),
                antes,
                depois,
            });
        }
    }
    d
}
