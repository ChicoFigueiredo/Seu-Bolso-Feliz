//! Valida a resposta do modelo, **item a item**. O provedor já checou o esquema; aqui se
//! confere o conteúdo (valores permitidos, datas, confiança) e se decide quais itens
//! valeram. Um item vale se trouxe o campo-chave (`e_financeiro` no achado, `categoria`
//! no registro) e nenhum campo ruim o derrubou: campo ruim some, o item continua.

use serde_json::Value;

use crate::normalizar::{cortar, normalizar};
use crate::prompt::{LIMITE_DA_JUSTIFICATIVA, TIPOS_DE_DOCUMENTO, TIPOS_DE_REGISTRO};
use crate::tipos::Item;

/// Uma sugestão que passou na validação.
#[derive(Debug, Clone, PartialEq)]
pub struct SugestaoValida {
    pub campo: String,
    /// Já na forma que se grava: texto para quase tudo, inteiro para `valor` e
    /// `duplicata_de`.
    pub valor: Value,
    pub confianca: f64,
    pub justificativa: String,
}

/// O campo que decide se o item valeu e que escolhe o degrau.
pub fn campo_chave(item: &Item) -> &'static str {
    match item {
        Item::Achado(_) => "e_financeiro",
        Item::Registro(_) => "categoria",
    }
}

/// A confiança do campo-chave do item, ou `None` se ele não veio.
pub fn confianca_chave(item: &Item, sugestoes: &[SugestaoValida]) -> Option<f64> {
    let chave = campo_chave(item);
    sugestoes
        .iter()
        .find(|s| s.campo == chave)
        .map(|s| s.confianca)
}

fn texto(valor: &Value, max: usize) -> Option<Value> {
    let t = valor.as_str()?.trim();
    (!t.is_empty()).then(|| Value::String(cortar(t, max)))
}

fn inteiro(valor: &Value) -> Option<i64> {
    match valor {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn data_valida(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Ok(a), Ok(m), Ok(d)) = (
        s[0..4].parse::<u32>(),
        s[5..7].parse::<u32>(),
        s[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    (1990..=2100).contains(&a) && (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// Normaliza o valor do campo para o item. `None` descarta a sugestão.
fn valor_do_campo(item: &Item, campo: &str, valor: &Value) -> Option<Value> {
    match (item, campo) {
        (Item::Achado(_), "e_financeiro") => {
            let v = normalizar(valor.as_str()?);
            ["sim", "nao", "incerto"]
                .contains(&v.as_str())
                .then_some(Value::String(v))
        }
        (Item::Achado(_), "tipo_documento") => {
            let v = normalizar(valor.as_str()?).replace(' ', "_");
            TIPOS_DE_DOCUMENTO
                .contains(&v.as_str())
                .then_some(Value::String(v))
        }
        (Item::Achado(_), "fornecedor")
        | (Item::Registro(_), "fornecedor" | "categoria" | "conta") => texto(valor, 120),
        (Item::Achado(_), "data_chave") => {
            let t = valor.as_str()?.trim();
            data_valida(t).then(|| Value::String(t.to_owned()))
        }
        (Item::Achado(_), "valor") => inteiro(valor).filter(|v| *v >= 0).map(Value::from),
        (Item::Registro(_), "tipo") => {
            let v = normalizar(valor.as_str()?);
            TIPOS_DE_REGISTRO
                .contains(&v.as_str())
                .then_some(Value::String(v))
        }
        (Item::Registro(r), "duplicata_de") => {
            let id = inteiro(valor)?;
            r.parecidos.contains(&id).then(|| Value::from(id))
        }
        _ => None,
    }
}

/// Valida o valor de **um** campo sugerido para `item` (o mesmo critério da resposta do
/// modelo). `None`: campo fora da lista do tipo do item, valor inválido ou confiança fora
/// de 0 a 1. Usado pelo servidor MCP, que recebe sugestões de um agente externo.
pub fn validar_sugestao(
    item: &Item,
    campo: &str,
    valor: &Value,
    confianca: f64,
    justificativa: Option<&str>,
) -> Option<SugestaoValida> {
    let valor = valor_do_campo(item, campo, valor)?;
    if !(0.0..=1.0).contains(&confianca) {
        return None;
    }
    let justificativa = justificativa
        .map(str::trim)
        .filter(|j| !j.is_empty())
        .map_or_else(
            || "sem justificativa do modelo".to_owned(),
            |j| cortar(j, LIMITE_DA_JUSTIFICATIVA),
        );
    Some(SugestaoValida {
        campo: campo.to_owned(),
        valor,
        confianca,
        justificativa,
    })
}

fn sugestao_valida(item: &Item, bruta: &Value) -> Option<SugestaoValida> {
    let campo = bruta.get("campo")?.as_str()?.to_owned();
    let valor = valor_do_campo(item, &campo, bruta.get("valor")?)?;
    let confianca = bruta.get("confianca")?.as_f64()?;
    if !(0.0..=1.0).contains(&confianca) {
        return None;
    }
    let justificativa = bruta
        .get("justificativa")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|j| !j.is_empty())
        .map_or_else(
            || "sem justificativa do modelo".to_owned(),
            |j| cortar(j, LIMITE_DA_JUSTIFICATIVA),
        );
    Some(SugestaoValida {
        campo,
        valor,
        confianca,
        justificativa,
    })
}

/// A confiança do campo-chave de um item da resposta, sem saber o tipo do item (o `Formato`
/// da fila do motor não o conhece): vale `e_financeiro` válido ou `categoria` não vazia.
pub fn confianca_chave_generica(entrada: &Value) -> Option<f64> {
    entrada
        .get("sugestoes")?
        .as_array()?
        .iter()
        .filter_map(|s| {
            let campo = s.get("campo")?.as_str()?;
            let valor = s.get("valor")?;
            let valido = match campo {
                "e_financeiro" => valor
                    .as_str()
                    .is_some_and(|v| ["sim", "nao", "incerto"].contains(&normalizar(v).as_str())),
                "categoria" => valor.as_str().is_some_and(|v| !v.trim().is_empty()),
                _ => false,
            };
            let c = s.get("confianca")?.as_f64()?;
            (valido && (0.0..=1.0).contains(&c)).then_some(c)
        })
        .reduce(f64::max)
}

/// Interpreta a resposta de um lote. O resultado tem uma posição por item de `itens`:
/// `Some` com as sugestões válidas, ou `None` se o item não valeu (faltou, veio vazio ou
/// sem o campo-chave). Com o mesmo campo repetido, fica o de maior confiança.
pub fn interpretar(json: &Value, itens: &[Item]) -> Vec<Option<Vec<SugestaoValida>>> {
    let mut saida: Vec<Option<Vec<SugestaoValida>>> = vec![None; itens.len()];
    let Some(lista) = json.get("itens").and_then(Value::as_array) else {
        return saida;
    };
    for entrada in lista {
        let Some(n) = entrada.get("n").and_then(Value::as_u64) else {
            continue;
        };
        let Some(posicao) = usize::try_from(n).ok().and_then(|n| n.checked_sub(1)) else {
            continue;
        };
        let Some(item) = itens.get(posicao) else {
            continue;
        };
        let mut validas: Vec<SugestaoValida> = Vec::new();
        for bruta in entrada
            .get("sugestoes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(s) = sugestao_valida(item, bruta) else {
                continue;
            };
            match validas.iter_mut().find(|v| v.campo == s.campo) {
                Some(existente) if existente.confianca >= s.confianca => {}
                Some(existente) => *existente = s,
                None => validas.push(s),
            }
        }
        if confianca_chave(item, &validas).is_some() {
            saida[posicao] = Some(validas);
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use serde_json::json;

    use super::*;
    use crate::tipos::{ItemDeAchado, ItemDeRegistro};

    fn achado() -> Item {
        Item::Achado(ItemDeAchado::default())
    }

    fn registro() -> Item {
        Item::Registro(ItemDeRegistro {
            id: 7,
            parecidos: vec![9],
            ..ItemDeRegistro::default()
        })
    }

    fn s(campo: &str, valor: Value, confianca: f64) -> Value {
        json!({"campo": campo, "valor": valor, "confianca": confianca, "justificativa": "porque sim"})
    }

    #[test]
    fn item_valido_precisa_do_campo_chave() {
        let itens = [achado(), registro()];
        let r = json!({"itens": [
            {"n": 1, "sugestoes": [s("fornecedor", json!("Energia Exemplo"), 0.9)]},
            {"n": 2, "sugestoes": [s("categoria", json!("Mercado"), 0.8)]},
        ]});
        let v = interpretar(&r, &itens);
        assert!(v[0].is_none(), "achado sem e_financeiro não vale");
        assert_eq!(v[1].as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn campo_ruim_some_mas_o_item_fica() {
        let itens = [achado()];
        let r = json!({"itens": [{"n": 1, "sugestoes": [
            s("e_financeiro", json!("Sim"), 0.95),
            s("data_chave", json!("12/11/2026"), 0.9),
            s("valor", json!(-5), 0.9),
            s("tipo_documento", json!("Nota Fiscal"), 0.9),
            s("categoria", json!("Mercado"), 0.9),
            s("fornecedor", json!("  "), 0.9),
        ]}]});
        let v = interpretar(&r, &itens);
        let campos: Vec<&str> = v[0]
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| s.campo.as_str())
            .collect();
        assert_eq!(campos, ["e_financeiro", "tipo_documento"]);
        assert_eq!(v[0].as_ref().unwrap()[1].valor, json!("nota_fiscal"));
    }

    #[test]
    fn confianca_fora_do_intervalo_e_justificativa_longa() {
        let itens = [registro()];
        let longa = "x".repeat(300);
        let r = json!({"itens": [{"n": 1, "sugestoes": [
            {"campo": "categoria", "valor": "Mercado", "confianca": 87, "justificativa": "a"},
            {"campo": "tipo", "valor": "Compra", "confianca": 0.7, "justificativa": longa},
            s("categoria", json!("Mercado"), 0.6),
        ]}]});
        let v = interpretar(&r, &itens);
        let v = v[0].as_ref().unwrap();
        assert_eq!(v.len(), 2, "a confiança 87 caiu");
        let tipo = v.iter().find(|s| s.campo == "tipo").unwrap();
        assert_eq!(tipo.valor, json!("compra"));
        assert_eq!(tipo.justificativa.chars().count(), 140);
    }

    #[test]
    fn duplicata_so_vale_para_um_dos_parecidos() {
        let itens = [registro()];
        let r = json!({"itens": [{"n": 1, "sugestoes": [
            s("categoria", json!("Mercado"), 0.9),
            s("duplicata_de", json!(9), 0.9),
        ]}]});
        assert_eq!(interpretar(&r, &itens)[0].as_ref().unwrap().len(), 2);
        let r = json!({"itens": [{"n": 1, "sugestoes": [
            s("categoria", json!("Mercado"), 0.9),
            s("duplicata_de", json!(123), 0.9),
        ]}]});
        assert_eq!(interpretar(&r, &itens)[0].as_ref().unwrap().len(), 1);
    }

    #[test]
    fn n_desconhecido_e_resposta_torta_nao_derrubam() {
        let itens = [achado()];
        assert_eq!(
            interpretar(&json!({"itens": [{"n": 9, "sugestoes": []}]}), &itens),
            [None]
        );
        assert_eq!(interpretar(&json!("oi"), &itens), [None]);
        assert_eq!(
            interpretar(&json!({"itens": [{"sugestoes": []}]}), &itens),
            [None]
        );
    }
}
