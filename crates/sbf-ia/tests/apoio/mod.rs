//! Apoio comum dos testes de integração: registro em memória e um formato de itens de texto.
#![allow(dead_code)]

use sbf_ia::{Consumo, Formato, Registro, Resposta, ResultadoDeIa, Saida};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct Memoria {
    pub saidas: Mutex<Vec<Saida>>,
    pub cache: Mutex<HashMap<String, Value>>,
}

impl Registro for Memoria {
    fn registrar_saida(&self, saida: &Saida) -> ResultadoDeIa<()> {
        self.saidas.lock().expect("trava").push(saida.clone());
        Ok(())
    }
    fn ler_cache(&self, chave: &str) -> ResultadoDeIa<Option<Value>> {
        Ok(self.cache.lock().expect("trava").get(chave).cloned())
    }
    fn gravar_cache(&self, chave: &str, json: &Value) -> ResultadoDeIa<()> {
        self.cache
            .lock()
            .expect("trava")
            .insert(chave.to_owned(), json.clone());
        Ok(())
    }
}

/// Itens são textos; a entrada de um lote é uma linha `posicao|texto` por item.
#[derive(Debug)]
pub struct Linhas;

impl Formato<String> for Linhas {
    fn montar_entrada(&self, itens: &[&String]) -> String {
        itens
            .iter()
            .enumerate()
            .map(|(k, t)| format!("{k}|{t}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn ler_confiancas(&self, resposta: &Value, quantos: usize) -> Vec<Option<f64>> {
        (0..quantos)
            .map(|k| {
                resposta["itens"]
                    .as_array()?
                    .iter()
                    .find(|x| x["i"].as_u64() == Some(k as u64))?["confianca"]
                    .as_f64()
            })
            .collect()
    }
}

pub fn esquema() -> Value {
    json!({
        "type": "object",
        "required": ["itens"],
        "properties": {"itens": {"type": "array", "items": {
            "type": "object",
            "required": ["i", "confianca"],
            "properties": {
                "i": {"type": "integer"},
                "confianca": {"type": "number", "minimum": 0, "maximum": 1}
            }
        }}}
    })
}

pub fn consumo() -> Consumo {
    Consumo {
        tokens_entrada: 10,
        tokens_saida: 5,
        custo_dezmilesimos_usd: 3,
        nominal: true,
    }
}

pub fn resposta(json: Value) -> Resposta {
    Resposta {
        json,
        consumo: consumo(),
        modelo: "modelo-falso".to_owned(),
    }
}

/// Uma resposta que dá `confianca` a cada um dos `n` itens.
pub fn resposta_com(n: usize, confianca: f64) -> Resposta {
    resposta(
        json!({"itens": (0..n).map(|i| json!({"i": i, "confianca": confianca})).collect::<Vec<_>>()}),
    )
}
