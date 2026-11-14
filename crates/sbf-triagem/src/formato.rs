//! O [`Formato`] da triagem para a fila do motor (`sbf_ia::Fila`): como montar a entrada de
//! um lote e como ler a confiança de cada item da resposta. A fila cuida do resto (lotes de
//! 25, cache, retentativa única, dois degraus, limite, registro do que saiu).

use sbf_ia::Formato;
use serde_json::Value;

use crate::entrada::{Vocabulario, montar_entrada};
use crate::resposta::confianca_chave_generica;
use crate::tipos::Item;

#[derive(Debug, Clone, Default)]
pub struct FormatoDaTriagem {
    pub vocabulario: Vocabulario,
}

impl FormatoDaTriagem {
    pub fn novo(vocabulario: Vocabulario) -> Self {
        Self { vocabulario }
    }
}

impl Formato<Item> for FormatoDaTriagem {
    fn montar_entrada(&self, itens: &[&Item]) -> String {
        montar_entrada(&self.vocabulario, itens)
    }

    /// A confiança do campo-chave de cada item (`n` = posição + 1). `None`: o item faltou ou
    /// veio sem o campo-chave, e a fila o manda de volta uma vez.
    fn ler_confiancas(&self, resposta: &Value, quantos: usize) -> Vec<Option<f64>> {
        let mut saida = vec![None; quantos];
        let Some(lista) = resposta.get("itens").and_then(Value::as_array) else {
            return saida;
        };
        for entrada in lista {
            let posicao = entrada
                .get("n")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| n.checked_sub(1));
            if let Some(p) = posicao.filter(|p| *p < quantos) {
                saida[p] = confianca_chave_generica(entrada);
            }
        }
        saida
    }
}
