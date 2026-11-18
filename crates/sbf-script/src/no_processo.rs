//! O executor Luau no próprio processo. É o que roda dentro do processo filho (depois do
//! endurecimento de SO) e o que os testes usam direto.
//!
//! Garantias deste nível (AC-LUA-05), todas por construção da VM, não por confiança no
//! script:
//!
//! - **uma VM nova por execução**: nenhum estado passa de um pedido ao seguinte;
//! - **só fonte**: o chunk é carregado em modo texto e o script não tem `load`;
//! - **ambiente por lista de permissão**: só sobram as funções puras do Luau; `io`, `os`,
//!   `package`, `debug`, `require`, `load*`, `getfenv`/`setfenv`, `collectgarbage`,
//!   `newproxy`, `print`, `math.random` e relógio não existem;
//! - **`Lua::sandbox(true)`**: bibliotecas e metatabela das strings somente leitura;
//! - **`entrada`, `parametros` e `ajuda` congeladas** (recursivo);
//! - **limites**: memória (alocador do Luau), instruções (contadas nas chamadas e nas voltas
//!   de laço) e tempo (conferido na mesma interrupção).
//!
//! O que a VM não consegue é interromper uma função nativa longa (um `string.find` com padrão
//! patológico); por isso o app usa o [`crate::ExecutorFilho`], cujo pai mata o processo no
//! prazo.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Value, VmState};
use sbf_regras::script::{
    ErroDoScript, ExecutorDeScript, Limites, PedidoDeScript, RespostaDoScript,
};
use serde_json::Value as Json;

use crate::ajuda::{self, INTEIRO_MAXIMO};

const PROFUNDIDADE_MAXIMA: usize = 64;
const MOTIVO_NENHUM: u8 = 0;
const MOTIVO_TEMPO: u8 = 1;
const MOTIVO_INSTRUCOES: u8 = 2;

/// Globais que sobram (lista de permissão). Tudo o mais é removido.
const GLOBAIS_PERMITIDAS: &[&str] = &[
    "assert",
    "error",
    "pcall",
    "xpcall",
    "select",
    "next",
    "pairs",
    "ipairs",
    "type",
    "typeof",
    "tostring",
    "tonumber",
    "rawequal",
    "rawget",
    "rawset",
    "rawlen",
    "setmetatable",
    "getmetatable",
    "unpack",
    "_VERSION",
    "table",
    "string",
    "math",
    "utf8",
    "bit32",
    "buffer",
    "vector",
    "coroutine",
];

/// Executor que roda o Luau neste mesmo processo.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExecutorNoProcesso;

impl ExecutorNoProcesso {
    #[must_use]
    pub fn novo() -> Self {
        Self
    }
}

impl ExecutorDeScript for ExecutorNoProcesso {
    fn executar(&self, pedido: &PedidoDeScript) -> Result<RespostaDoScript, ErroDoScript> {
        let tamanho = pedido.fonte.len()
            + serde_json::to_vec(&pedido.entrada).map_or(usize::MAX / 4, |v| v.len())
            + serde_json::to_vec(&pedido.parametros).map_or(usize::MAX / 4, |v| v.len());
        if tamanho > pedido.limites.entrada_bytes {
            return Err(ErroDoScript::Grande(format!(
                "pedido de {tamanho} bytes passa de {}",
                pedido.limites.entrada_bytes
            )));
        }
        let vigia = Vigia::nova(&pedido.limites);
        let lua = preparar(&pedido.limites, &vigia).map_err(|e| mapear(&e, &vigia))?;
        let transformar = carregar(&lua, &pedido.fonte, &vigia)?;
        let entrada = json_para_lua(&lua, &pedido.entrada, 0).map_err(|e| mapear(&e, &vigia))?;
        let parametros =
            json_para_lua(&lua, &pedido.parametros, 0).map_err(|e| mapear(&e, &vigia))?;
        let ajuda = criar_ajuda(&lua).map_err(|e| mapear(&e, &vigia))?;
        let retorno: Value = transformar
            .call((entrada, parametros, ajuda))
            .map_err(|e| mapear(&e, &vigia))?;
        let resposta = validar_retorno(&retorno)?;
        let bytes = serde_json::to_vec(&resposta).map_or(usize::MAX, |v| v.len());
        if bytes > pedido.limites.saida_bytes {
            return Err(ErroDoScript::Grande(format!(
                "resposta de {bytes} bytes passa de {}",
                pedido.limites.saida_bytes
            )));
        }
        Ok(resposta)
    }

    fn verificar(&self, fonte: &str) -> Result<(), ErroDoScript> {
        let limites = Limites {
            memoria_bytes: 4 * 1024 * 1024,
            tempo_ms: 100,
            instrucoes: 100_000,
            ..Limites::default()
        };
        if fonte.len() > limites.entrada_bytes {
            return Err(ErroDoScript::Grande("fonte grande demais".into()));
        }
        let vigia = Vigia::nova(&limites);
        let lua = preparar(&limites, &vigia).map_err(|e| mapear(&e, &vigia))?;
        carregar(&lua, fonte, &vigia).map(|_| ())
    }
}

/// Estado compartilhado com a interrupção: o contador, o prazo e o motivo do corte.
#[derive(Debug, Clone)]
struct Vigia {
    motivo: Arc<AtomicU8>,
}

impl Vigia {
    fn nova(_limites: &Limites) -> Self {
        Self {
            motivo: Arc::new(AtomicU8::new(MOTIVO_NENHUM)),
        }
    }
}

fn preparar(limites: &Limites, vigia: &Vigia) -> mlua::Result<Lua> {
    let libs = StdLib::COROUTINE
        | StdLib::TABLE
        | StdLib::STRING
        | StdLib::UTF8
        | StdLib::BIT
        | StdLib::MATH
        | StdLib::BUFFER
        | StdLib::VECTOR;
    let lua = Lua::new_with(libs, LuaOptions::default())?;
    // Ambiente por lista de permissão.
    let globais = lua.globals();
    let mut remover = Vec::new();
    for par in globais.pairs::<Value, Value>() {
        let (chave, _) = par?;
        let permitida = matches!(&chave, Value::String(s)
            if s.to_str().is_ok_and(|s| GLOBAIS_PERMITIDAS.contains(&&*s)));
        if !permitida {
            remover.push(chave);
        }
    }
    for chave in remover {
        globais.set(chave, Value::Nil)?;
    }
    let matematica: Table = globais.get("math")?;
    matematica.set("random", Value::Nil)?;
    matematica.set("randomseed", Value::Nil)?;
    lua.sandbox(true)?;
    lua.set_memory_limit(limites.memoria_bytes)?;

    let instrucoes_max = limites.instrucoes;
    let prazo = Instant::now() + Duration::from_millis(limites.tempo_ms);
    let contagem = AtomicU64::new(0);
    let motivo = Arc::clone(&vigia.motivo);
    lua.set_interrupt(move |_| {
        if motivo.load(Ordering::Relaxed) != MOTIVO_NENHUM {
            return Err(mlua::Error::runtime("interrompido"));
        }
        let n = contagem.fetch_add(1, Ordering::Relaxed) + 1;
        if n > instrucoes_max {
            motivo.store(MOTIVO_INSTRUCOES, Ordering::Relaxed);
            return Err(mlua::Error::runtime("limite de instruções"));
        }
        if n.is_multiple_of(256) && Instant::now() >= prazo {
            motivo.store(MOTIVO_TEMPO, Ordering::Relaxed);
            return Err(mlua::Error::runtime("limite de tempo"));
        }
        Ok(VmState::Continue)
    });
    Ok(lua)
}

/// Compila (só texto), roda o corpo do script (que só deve definir funções) e devolve
/// `transformar`.
fn carregar(lua: &Lua, fonte: &str, vigia: &Vigia) -> Result<Function, ErroDoScript> {
    // Bytecode do Luau começa por um byte de versão (< 0x20); fonte nunca começa assim.
    if fonte
        .as_bytes()
        .first()
        .is_some_and(|b| *b < 0x20 && !b.is_ascii_whitespace())
    {
        return Err(ErroDoScript::Sintaxe(
            "só código-fonte é aceito (bytecode recusado)".into(),
        ));
    }
    let bloco = lua
        .load(fonte)
        .set_name("=regra")
        .set_mode(mlua::chunk::ChunkMode::Text)
        .into_function()
        .map_err(|e| mapear(&e, vigia))?;
    bloco
        .call::<MultiValue>(())
        .map_err(|e| mapear(&e, vigia))?;
    match lua.globals().get::<Value>("transformar") {
        Ok(Value::Function(f)) => Ok(f),
        Ok(_) => Err(ErroDoScript::SemTransformar),
        Err(e) => Err(mapear(&e, vigia)),
    }
}

fn mapear(erro: &mlua::Error, vigia: &Vigia) -> ErroDoScript {
    match vigia.motivo.load(Ordering::Relaxed) {
        MOTIVO_TEMPO => return ErroDoScript::Tempo,
        MOTIVO_INSTRUCOES => return ErroDoScript::Instrucoes,
        _ => {}
    }
    mapear_sem_motivo(erro)
}

fn mapear_sem_motivo(erro: &mlua::Error) -> ErroDoScript {
    match erro {
        mlua::Error::MemoryError(_) => ErroDoScript::Memoria,
        mlua::Error::SyntaxError { message, .. } => ErroDoScript::Sintaxe(curto(message)),
        mlua::Error::CallbackError { cause, .. } => mapear_sem_motivo(cause),
        outro => {
            let texto = outro.to_string();
            if texto.contains("not enough memory") {
                ErroDoScript::Memoria
            } else {
                ErroDoScript::Execucao(curto(&texto))
            }
        }
    }
}

fn curto(texto: &str) -> String {
    let primeira = texto.lines().next().unwrap_or("");
    primeira.chars().take(300).collect()
}

// --- entrada: JSON -> tabelas congeladas ----------------------------------------------

fn json_para_lua(lua: &Lua, valor: &Json, fundo: usize) -> mlua::Result<Value> {
    if fundo > PROFUNDIDADE_MAXIMA {
        return Err(mlua::Error::runtime("entrada funda demais"));
    }
    Ok(match valor {
        Json::Null => Value::Nil,
        Json::Bool(b) => Value::Boolean(*b),
        Json::Number(n) => match n.as_f64() {
            Some(f) => Value::Number(f),
            None => return Err(mlua::Error::runtime("número fora do alcance")),
        },
        Json::String(s) => Value::String(lua.create_string(s)?),
        Json::Array(itens) => {
            let t = lua.create_table_with_capacity(itens.len(), 0)?;
            for (i, item) in itens.iter().enumerate() {
                t.raw_set(i + 1, json_para_lua(lua, item, fundo + 1)?)?;
            }
            t.set_readonly(true);
            Value::Table(t)
        }
        Json::Object(mapa) => {
            let t = lua.create_table_with_capacity(0, mapa.len())?;
            for (k, v) in mapa {
                t.raw_set(k.as_str(), json_para_lua(lua, v, fundo + 1)?)?;
            }
            t.set_readonly(true);
            Value::Table(t)
        }
    })
}

// --- ajuda -----------------------------------------------------------------------------

fn inteiro(v: &Value, nome: &str) -> mlua::Result<i64> {
    let erro = || mlua::Error::runtime(format!("{nome}: esperava um inteiro (até 2^53)"));
    match v {
        Value::Integer(i) => {
            let i = *i;
            if i.abs() <= INTEIRO_MAXIMO {
                Ok(i)
            } else {
                Err(erro())
            }
        }
        Value::Number(n)
            if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 =>
        {
            #[allow(clippy::cast_possible_truncation)]
            Ok(*n as i64)
        }
        _ => Err(erro()),
    }
}

fn criar_ajuda(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set(
        "mul_div_arred",
        lua.create_function(|_, (a, b, c): (Value, Value, Value)| {
            let r = ajuda::mul_div_arred(
                inteiro(&a, "mul_div_arred")?,
                inteiro(&b, "mul_div_arred")?,
                inteiro(&c, "mul_div_arred")?,
            )
            .map_err(mlua::Error::runtime)?;
            #[allow(clippy::cast_precision_loss)]
            Ok(r as f64)
        })?,
    )?;
    t.set(
        "centavos",
        lua.create_function(|_, texto: mlua::LuaString| {
            let texto = texto
                .to_str()
                .map_err(|_| mlua::Error::runtime("centavos: texto inválido"))?;
            #[allow(clippy::cast_precision_loss)]
            ajuda::centavos(&texto)
                .map(|v| v as f64)
                .map_err(mlua::Error::runtime)
        })?,
    )?;
    t.set(
        "data",
        lua.create_function(|_, texto: mlua::LuaString| {
            let texto = texto
                .to_str()
                .map_err(|_| mlua::Error::runtime("data: texto inválido"))?;
            ajuda::data(&texto).map_err(mlua::Error::runtime)
        })?,
    )?;
    t.set(
        "normalizar",
        lua.create_function(|_, texto: mlua::LuaString| {
            let texto = texto
                .to_str()
                .map_err(|_| mlua::Error::runtime("normalizar: texto inválido"))?;
            Ok(ajuda::normalizar(&texto))
        })?,
    )?;
    t.set_readonly(true);
    Ok(t)
}

// --- saída: tabela -> RespostaDoScript -------------------------------------------------

fn retorno(msg: impl Into<String>) -> ErroDoScript {
    ErroDoScript::Retorno(msg.into())
}

fn texto_de(v: &Value, onde: &str) -> Result<String, ErroDoScript> {
    match v {
        Value::String(s) => s
            .to_str()
            .map(|s| s.to_string())
            .map_err(|_| retorno(format!("{onde}: texto não é UTF-8"))),
        _ => Err(retorno(format!("{onde}: esperava texto"))),
    }
}

fn booleano(t: &Table, nome: &str, obrigatorio: bool) -> Result<bool, ErroDoScript> {
    match t
        .raw_get::<Value>(nome)
        .map_err(|e| retorno(e.to_string()))?
    {
        Value::Boolean(b) => Ok(b),
        Value::Nil if !obrigatorio => Ok(false),
        Value::Nil => Err(retorno(format!("falta `{nome}`"))),
        _ => Err(retorno(format!("`{nome}` deve ser booleano"))),
    }
}

fn lista_de_textos(t: &Table, nome: &str) -> Result<Vec<String>, ErroDoScript> {
    match t
        .raw_get::<Value>(nome)
        .map_err(|e| retorno(e.to_string()))?
    {
        Value::Nil => Ok(Vec::new()),
        Value::Table(lista) => {
            let n = lista.raw_len();
            let mut total = 0usize;
            for par in lista.pairs::<Value, Value>() {
                par.map_err(|e| retorno(e.to_string()))?;
                total += 1;
            }
            if total != n {
                return Err(retorno(format!("`{nome}` deve ser uma lista")));
            }
            (1..=n)
                .map(|i| {
                    let v: Value = lista.raw_get(i).map_err(|e| retorno(e.to_string()))?;
                    texto_de(&v, nome)
                })
                .collect()
        }
        _ => Err(retorno(format!("`{nome}` deve ser uma lista de textos"))),
    }
}

fn numero_inteiro(v: &Value, onde: &str) -> Result<i64, ErroDoScript> {
    let n = match v {
        Value::Integer(i) => {
            let i = *i;
            return if i.abs() <= INTEIRO_MAXIMO {
                Ok(i)
            } else {
                Err(retorno(format!("{onde}: inteiro grande demais")))
            };
        }
        Value::Number(n) => *n,
        _ => return Err(retorno(format!("{onde}: esperava um número inteiro"))),
    };
    if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
        #[allow(clippy::cast_possible_truncation)]
        Ok(n as i64)
    } else {
        Err(retorno(format!(
            "{onde}: não é um inteiro exato (centavos não admitem fração)"
        )))
    }
}

fn campo_para_json(nome: &str, v: &Value) -> Result<Json, ErroDoScript> {
    let onde = format!("campos.{nome}");
    match v {
        Value::Boolean(b) => {
            if nome.ends_with("_centavos") {
                return Err(retorno(format!("{onde}: deve ser inteiro")));
            }
            Ok(Json::Bool(*b))
        }
        Value::Integer(_) | Value::Number(_) => Ok(Json::from(numero_inteiro(v, &onde)?)),
        Value::String(_) => {
            if nome.ends_with("_centavos") {
                return Err(retorno(format!("{onde}: deve ser inteiro, não texto")));
            }
            Ok(Json::String(texto_de(v, &onde)?))
        }
        _ => Err(retorno(format!("{onde}: só texto, inteiro ou booleano"))),
    }
}

fn validar_retorno(v: &Value) -> Result<RespostaDoScript, ErroDoScript> {
    let Value::Table(t) = v else {
        return Err(retorno("transformar deve devolver uma tabela"));
    };
    const CONHECIDAS: &[&str] = &[
        "corresponde",
        "tipo_documental",
        "campos",
        "revisar",
        "motivos",
        "evidencias",
    ];
    for par in t.pairs::<Value, Value>() {
        let (k, _) = par.map_err(|e| retorno(e.to_string()))?;
        let ok =
            matches!(&k, Value::String(s) if s.to_str().is_ok_and(|s| CONHECIDAS.contains(&&*s)));
        if !ok {
            return Err(retorno("a tabela devolvida tem chave desconhecida"));
        }
    }
    let corresponde = booleano(t, "corresponde", true)?;
    let tipo_documental = match t
        .raw_get::<Value>("tipo_documental")
        .map_err(|e| retorno(e.to_string()))?
    {
        Value::Nil => None,
        outro => Some(texto_de(&outro, "tipo_documental")?),
    };
    let mut campos = BTreeMap::new();
    match t
        .raw_get::<Value>("campos")
        .map_err(|e| retorno(e.to_string()))?
    {
        Value::Nil => {}
        Value::Table(c) => {
            for par in c.pairs::<Value, Value>() {
                let (k, val) = par.map_err(|e| retorno(e.to_string()))?;
                let nome = texto_de(&k, "nome de campo")?;
                campos.insert(nome.clone(), campo_para_json(&nome, &val)?);
            }
        }
        _ => return Err(retorno("`campos` deve ser uma tabela")),
    }
    Ok(RespostaDoScript {
        corresponde,
        tipo_documental,
        campos,
        revisar: booleano(t, "revisar", false)?,
        motivos: lista_de_textos(t, "motivos")?,
        evidencias: lista_de_textos(t, "evidencias")?,
    })
}
