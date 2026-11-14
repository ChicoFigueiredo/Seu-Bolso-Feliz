//! Validação de JSON Schema (o subconjunto que a triagem usa) e extração do JSON de uma
//! resposta de texto. Próprios, sem dependência: o esquema é nosso e pequeno. Nenhuma
//! mensagem de erro repete valores do JSON (podem ser dado financeiro): só caminho e regra.

use serde_json::Value;

/// Tira o JSON de uma resposta de texto: o texto puro, cercado de ```` ```json ````, ou com
/// prosa em volta (pega do primeiro `{`/`[` ao fechamento casado).
pub fn extrair_json(texto: &str) -> Result<Value, String> {
    let aparado = texto.trim();
    if let Ok(v) = serde_json::from_str::<Value>(aparado) {
        return Ok(v);
    }
    // Cerca de código.
    if let Some(inicio) = aparado.find("```") {
        let depois = &aparado[inicio + 3..];
        let depois = depois
            .strip_prefix("json")
            .or_else(|| depois.strip_prefix("JSON"))
            .unwrap_or(depois);
        if let Some(fim) = depois.find("```")
            && let Ok(v) = serde_json::from_str::<Value>(depois[..fim].trim())
        {
            return Ok(v);
        }
    }
    // Primeiro valor JSON completo a partir do primeiro `{` ou `[`.
    for (i, c) in aparado.char_indices() {
        if c == '{' || c == '[' {
            let mut fluxo = serde_json::Deserializer::from_str(&aparado[i..]).into_iter::<Value>();
            if let Some(Ok(v)) = fluxo.next() {
                return Ok(v);
            }
        }
    }
    Err("a resposta não contém JSON".to_owned())
}

/// Valida `valor` contra `esquema`. Suporta `type`, `enum`, `const`, `properties`,
/// `required`, `additionalProperties`, `items`, `minItems`, `maxItems`, `minimum`,
/// `maximum`, `minLength`, `maxLength`, `anyOf` e `oneOf`; o resto é ignorado.
///
/// # Errors
/// A primeira violação, como `caminho: regra` (sem o valor).
pub fn validar_esquema(esquema: &Value, valor: &Value) -> Result<(), String> {
    validar(esquema, valor, "$")
}

fn tipo_confere(tipo: &str, valor: &Value) -> bool {
    match tipo {
        "object" => valor.is_object(),
        "array" => valor.is_array(),
        "string" => valor.is_string(),
        "boolean" => valor.is_boolean(),
        "null" => valor.is_null(),
        "number" => valor.is_number(),
        "integer" => {
            valor.is_i64()
                || valor.is_u64()
                || valor
                    .as_f64()
                    .is_some_and(|f| f.is_finite() && f.fract() == 0.0)
        }
        _ => true,
    }
}

fn validar(esquema: &Value, valor: &Value, caminho: &str) -> Result<(), String> {
    let Some(regras) = esquema.as_object() else {
        return Ok(()); // `true`/`{}`: aceita tudo.
    };
    if let Some(tipo) = regras.get("type") {
        let ok = match tipo {
            Value::String(t) => tipo_confere(t, valor),
            Value::Array(ts) => ts
                .iter()
                .filter_map(Value::as_str)
                .any(|t| tipo_confere(t, valor)),
            _ => true,
        };
        if !ok {
            return Err(format!("{caminho}: tipo diferente do esperado"));
        }
    }
    if let Some(Value::Array(opcoes)) = regras.get("enum")
        && !opcoes.contains(valor)
    {
        return Err(format!("{caminho}: valor fora da lista permitida"));
    }
    if let Some(c) = regras.get("const")
        && c != valor
    {
        return Err(format!("{caminho}: valor diferente do fixo"));
    }
    for chave in ["anyOf", "oneOf"] {
        if let Some(Value::Array(alternativas)) = regras.get(chave) {
            let casam = alternativas
                .iter()
                .filter(|a| validar(a, valor, caminho).is_ok())
                .count();
            let ok = if chave == "anyOf" {
                casam >= 1
            } else {
                casam == 1
            };
            if !ok {
                return Err(format!("{caminho}: nenhuma alternativa de {chave} confere"));
            }
        }
    }
    if let Some(n) = valor.as_f64() {
        if regras
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{caminho}: abaixo do mínimo"));
        }
        if regras
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{caminho}: acima do máximo"));
        }
    }
    if let Some(s) = valor.as_str() {
        let tamanho = s.chars().count() as u64;
        if regras
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| tamanho < m)
        {
            return Err(format!("{caminho}: texto curto demais"));
        }
        if regras
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|m| tamanho > m)
        {
            return Err(format!("{caminho}: texto longo demais"));
        }
    }
    if let Some(itens) = valor.as_array() {
        let n = itens.len() as u64;
        if regras
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{caminho}: poucos itens"));
        }
        if regras
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{caminho}: itens demais"));
        }
        if let Some(esquema_item) = regras.get("items") {
            for (i, item) in itens.iter().enumerate() {
                validar(esquema_item, item, &format!("{caminho}[{i}]"))?;
            }
        }
    }
    if let Some(objeto) = valor.as_object() {
        if let Some(Value::Array(obrigatorios)) = regras.get("required") {
            for campo in obrigatorios.iter().filter_map(Value::as_str) {
                if !objeto.contains_key(campo) {
                    return Err(format!("{caminho}: falta o campo `{campo}`"));
                }
            }
        }
        let propriedades = regras.get("properties").and_then(Value::as_object);
        for (nome, v) in objeto {
            match propriedades.and_then(|p| p.get(nome)) {
                Some(esquema_campo) => validar(esquema_campo, v, &format!("{caminho}.{nome}"))?,
                None => match regras.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        return Err(format!("{caminho}: campo `{nome}` não previsto"));
                    }
                    Some(extra @ Value::Object(_)) => {
                        validar(extra, v, &format!("{caminho}.{nome}"))?;
                    }
                    _ => {}
                },
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn extrai_json_puro_cercado_e_com_prosa() {
        assert_eq!(
            extrair_json(" {\"a\":1} ").unwrap_or_default(),
            json!({"a":1})
        );
        assert_eq!(
            extrair_json("```json\n{\"a\":1}\n```").unwrap_or_default(),
            json!({"a":1})
        );
        assert_eq!(
            extrair_json("Aqui está:\n```\n[1,2]\n```\nPronto.").unwrap_or_default(),
            json!([1, 2])
        );
        assert_eq!(
            extrair_json("Claro! {\"a\":{\"b\":\"}\"}} fim").unwrap_or_default(),
            json!({"a":{"b":"}"}})
        );
        assert!(extrair_json("sem json aqui").is_err());
    }

    #[test]
    fn valida_o_subconjunto() {
        let esquema = json!({
            "type":"object","required":["itens"],"additionalProperties":false,
            "properties":{"itens":{"type":"array","maxItems":2,"items":{
                "type":"object","required":["id","confianca"],
                "properties":{
                    "id":{"type":"integer"},
                    "confianca":{"type":"number","minimum":0,"maximum":1},
                    "justificativa":{"type":"string","maxLength":5},
                    "e_financeiro":{"enum":["sim","nao","incerto"]}
                }}}}
        });
        let ok = json!({"itens":[{"id":1,"confianca":0.5,"e_financeiro":"sim"}]});
        assert!(validar_esquema(&esquema, &ok).is_ok());
        for ruim in [
            json!([]),
            json!({"itens":[{"id":1}]}),
            json!({"itens":[{"id":1.5,"confianca":0.5}]}),
            json!({"itens":[{"id":1,"confianca":1.5}]}),
            json!({"itens":[{"id":1,"confianca":0.5,"justificativa":"longa demais"}]}),
            json!({"itens":[{"id":1,"confianca":0.5,"e_financeiro":"talvez"}]}),
            json!({"itens":[1,2,3]}),
            json!({"itens":[],"extra":1}),
        ] {
            assert!(validar_esquema(&esquema, &ruim).is_err(), "{ruim}");
        }
    }

    #[test]
    fn a_mensagem_nao_repete_o_valor() {
        let esquema = json!({"type":"object","properties":{"c":{"enum":["a"]}}});
        let erro = validar_esquema(&esquema, &json!({"c":"CPF 123.456.789-00"})).unwrap_err();
        assert!(!erro.contains("123"), "{erro}");
    }
}
