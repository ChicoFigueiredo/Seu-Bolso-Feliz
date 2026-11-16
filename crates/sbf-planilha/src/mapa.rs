//! O mapa opcional (`--mapa`, JSON): diz, por descrição da planilha, a natureza, o centro,
//! o prazo (parcelas que faltam), o fornecedor da Pluggy e o dia do vencimento.
//!
//! ```json
//! { "itens": [ { "descricao": "Aluguel", "natureza": "saida", "centro": "Casa",
//!                "parcelas_que_faltam": 12, "fornecedor": "IMOBILIARIA X", "dia": 5 } ] }
//! ```
//!
//! `natureza`: `entrada`, `saida`, `desconto_em_folha`, `passagem_entrada`, `passagem_saida`.

use std::path::Path;

use sbf_planejamento::{Natureza, Sentido};
use serde::Deserialize;

use crate::texto::normalizar;
use crate::{ErroPlanilha, Resultado};

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct ItemDoMapa {
    pub descricao: String,
    #[serde(default)]
    pub natureza: Option<String>,
    #[serde(default)]
    pub centro: Option<String>,
    /// Parcelas que faltam, contando a do mês importado.
    #[serde(default)]
    pub parcelas_que_faltam: Option<u32>,
    /// O fornecedor correspondente no inventário (Pluggy).
    #[serde(default)]
    pub fornecedor: Option<String>,
    /// Dia do vencimento, quando a planilha não o traz (o salário, por exemplo).
    #[serde(default)]
    pub dia: Option<u8>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Mapa {
    #[serde(default)]
    pub itens: Vec<ItemDoMapa>,
}

impl Mapa {
    pub fn vazio() -> Mapa {
        Mapa::default()
    }

    pub fn ler(arquivo: &Path) -> Resultado<Mapa> {
        let texto = std::fs::read_to_string(arquivo)
            .map_err(|e| ErroPlanilha::Mapa(format!("não deu para ler o arquivo ({e})")))?;
        Mapa::de_json(&texto)
    }

    pub fn de_json(texto: &str) -> Resultado<Mapa> {
        let mapa: Mapa =
            serde_json::from_str(texto).map_err(|e| ErroPlanilha::Mapa(e.to_string()))?;
        for i in &mapa.itens {
            if let Some(n) = &i.natureza
                && natureza_de_texto(n).is_none()
            {
                return Err(ErroPlanilha::Mapa(format!("natureza desconhecida: {n}")));
            }
        }
        Ok(mapa)
    }

    /// O item para a descrição da planilha (comparação normalizada).
    pub fn item(&self, descricao: &str) -> Option<&ItemDoMapa> {
        let d = normalizar(descricao);
        self.itens.iter().find(|i| normalizar(&i.descricao) == d)
    }

    /// Os fornecedores do inventário que o mapa associa a algum item.
    pub fn fornecedores(&self) -> impl Iterator<Item = (&str, &str)> {
        self.itens
            .iter()
            .filter_map(|i| Some((i.descricao.as_str(), i.fornecedor.as_deref()?)))
    }
}

pub fn natureza_de_texto(t: &str) -> Option<Natureza> {
    Some(match normalizar(t).replace(' ', "_").as_str() {
        "entrada" => Natureza::Entrada,
        "saida" => Natureza::Saida,
        "desconto_em_folha" => Natureza::DescontoEmFolha,
        "passagem_entrada" => Natureza::Passagem { sentido: Sentido::Entrada },
        "passagem_saida" => Natureza::Passagem { sentido: Sentido::Saida },
        _ => return None,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_json_e_acha_por_descricao_normalizada() {
        let m = Mapa::de_json(
            r#"{"itens":[{"descricao":"Plano de Saúde","natureza":"desconto_em_folha","parcelas_que_faltam":3}]}"#,
        )
        .expect("mapa");
        let i = m.item("plano  de saude").expect("item");
        assert_eq!(i.parcelas_que_faltam, Some(3));
        assert_eq!(natureza_de_texto(i.natureza.as_deref().unwrap_or("")), Some(Natureza::DescontoEmFolha));
        assert!(m.item("outro").is_none());
    }

    #[test]
    fn natureza_invalida_e_erro() {
        assert!(Mapa::de_json(r#"{"itens":[{"descricao":"x","natureza":"voando"}]}"#).is_err());
    }
}
