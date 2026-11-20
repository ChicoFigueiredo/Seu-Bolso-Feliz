//! AC-LUA-05 e precisão de centavos, pelo executor no próprio processo.

mod comum;

use std::time::{Duration, Instant};

use comum::{corpo, pedido, pedido_com};
use sbf_regras::script::{ErroDoScript, ExecutorDeScript, Limites};
use sbf_script::ExecutorNoProcesso;
use serde_json::json;

fn rodar(fonte: &str) -> Result<sbf_regras::script::RespostaDoScript, ErroDoScript> {
    ExecutorNoProcesso::novo().executar(&pedido(fonte))
}

#[test]
fn script_simples_devolve_a_resposta() {
    let r = rodar(
        r#"function transformar(entrada, parametros, ajuda)
  return {corresponde = true, tipo_documental = "comprovante", revisar = true,
    campos = {valor_centavos = ajuda.centavos("1.234,56"), data_pagamento = ajuda.data("06/10/2026"),
              n = entrada.campos.a, ok = true, nome = ajuda.normalizar("  AÇÃO  Boa ")},
    motivos = {"um", "dois"}, evidencias = {"trecho"}}
end"#,
    )
    .expect("roda");
    assert!(r.corresponde && r.revisar);
    assert_eq!(r.tipo_documental.as_deref(), Some("comprovante"));
    assert_eq!(r.campos["valor_centavos"], json!(123_456));
    assert_eq!(r.campos["data_pagamento"], json!("2026-10-06"));
    assert_eq!(r.campos["n"], json!(1));
    assert_eq!(r.campos["ok"], json!(true));
    assert_eq!(r.campos["nome"], json!("acao boa"));
    assert_eq!(r.motivos, ["um", "dois"]);
    assert_eq!(r.evidencias, ["trecho"]);
}

#[test]
fn globais_proibidas_nao_existem() {
    let nomes = [
        "io",
        "os",
        "package",
        "require",
        "debug",
        "load",
        "loadstring",
        "dofile",
        "loadfile",
        "getfenv",
        "setfenv",
        "collectgarbage",
        "newproxy",
        "print",
        "warn",
        "gcinfo",
        "_G",
        "shared",
        "dostring",
    ];
    for n in nomes {
        let fonte = format!(
            "function transformar() return {{corresponde = (rawget(_ENV or {{}}, '{n}') == nil) and ({n} == nil)}} end"
        );
        // Referenciar o nome direto: global não definida vale nil.
        let r = rodar(&fonte).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert!(r.corresponde, "global {n} deveria ser nil");
    }
    for (lib, f) in [
        ("math", "random"),
        ("math", "randomseed"),
        ("os", "clock"),
        ("os", "time"),
        ("string", "dump"),
    ] {
        let fonte = format!(
            "function transformar() local t = {lib} return {{corresponde = t == nil or t.{f} == nil}} end"
        );
        assert!(rodar(&fonte).expect("roda").corresponde, "{lib}.{f}");
    }
}

#[test]
fn nao_da_para_carregar_bytecode_nem_codigo_dinamico() {
    // Bytecode (começa por byte de controle) é recusado como fonte.
    let e = rodar("\u{3}\u{1}abc").expect_err("bytecode");
    assert!(matches!(e, ErroDoScript::Sintaxe(_)), "{e:?}");
    // `load` e afins não existem.
    let e = rodar(&corpo("return load('return 1')()")).expect_err("sem load");
    assert!(matches!(e, ErroDoScript::Execucao(_)), "{e:?}");
    let e =
        rodar(&corpo("local f = loadstring('return 1') return f()")).expect_err("sem loadstring");
    assert!(matches!(e, ErroDoScript::Execucao(_)), "{e:?}");
}

#[test]
fn metatabelas_e_globais_nao_mudam() {
    for codigo in [
        "getmetatable('').__index = function() return 1 end",
        "getmetatable('').__newindex = 1",
        "string.rep = nil",
        "string.novo = 1",
        "table.insert = nil",
        "math.pi = 3",
        "setmetatable(string, {})",
        "setmetatable(_G, {})",
        "table.freeze = nil",
    ] {
        let e = rodar(&corpo(codigo)).expect_err(codigo);
        assert!(matches!(e, ErroDoScript::Execucao(_)), "{codigo}: {e:?}");
    }
}

#[test]
fn entrada_e_parametros_sao_somente_leitura() {
    for codigo in [
        "entrada.novo = 1",
        "entrada.campos.a = 2",
        "parametros.x = 1",
        "parametros.lista[1] = 9",
        "table.insert(parametros.lista, 1)",
        "ajuda.mul_div_arred = nil",
        "rawset(entrada, 'x', 1)",
    ] {
        let p = pedido_com(
            &corpo(codigo),
            json!({"campos": {"a": 1}}),
            json!({"lista": [1, 2], "x": 0}),
            Limites::default(),
        );
        let e = ExecutorNoProcesso::novo().executar(&p).expect_err(codigo);
        assert!(matches!(e, ErroDoScript::Execucao(_)), "{codigo}: {e:?}");
    }
}

#[test]
fn nada_vaza_entre_pedidos() {
    let ex = ExecutorNoProcesso::novo();
    let a = pedido(
        "estado = 42\nfunction transformar() estado = estado + 1 return {corresponde = true, campos = {n = estado}} end",
    );
    let r1 = ex.executar(&a).expect("1");
    let r2 = ex.executar(&a).expect("2");
    assert_eq!(r1, r2);
    assert_eq!(r1.campos["n"], json!(43));
    // Uma global criada num script não existe no seguinte.
    let b = pedido("function transformar() return {corresponde = (estado == nil)} end");
    assert!(ex.executar(&b).expect("b").corresponde);
}

#[test]
fn laco_infinito_vira_tempo_ou_instrucoes() {
    let limites = Limites {
        tempo_ms: 100,
        instrucoes: u64::MAX,
        ..Limites::default()
    };
    let p = pedido_com(&corpo("while true do end"), json!({}), json!({}), limites);
    let t = Instant::now();
    let e = ExecutorNoProcesso::novo().executar(&p).expect_err("laço");
    assert_eq!(e, ErroDoScript::Tempo);
    assert!(t.elapsed() < Duration::from_secs(3));

    let limites = Limites {
        instrucoes: 10_000,
        tempo_ms: 60_000,
        ..Limites::default()
    };
    let p = pedido_com(&corpo("while true do end"), json!({}), json!({}), limites);
    assert_eq!(
        ExecutorNoProcesso::novo().executar(&p),
        Err(ErroDoScript::Instrucoes)
    );

    // pcall não engole o corte.
    let p = pedido_com(
        &corpo("while true do pcall(function() while true do end end) end"),
        json!({}),
        json!({}),
        Limites {
            tempo_ms: 100,
            instrucoes: u64::MAX,
            ..Limites::default()
        },
    );
    assert_eq!(
        ExecutorNoProcesso::novo().executar(&p),
        Err(ErroDoScript::Tempo)
    );
}

#[test]
fn laco_infinito_no_corpo_do_script_tambem_e_cortado() {
    let p = pedido_com(
        "while true do end\nfunction transformar() return {corresponde=true} end",
        json!({}),
        json!({}),
        Limites {
            tempo_ms: 100,
            instrucoes: u64::MAX,
            ..Limites::default()
        },
    );
    assert_eq!(
        ExecutorNoProcesso::novo().executar(&p),
        Err(ErroDoScript::Tempo)
    );
}

#[test]
fn recursao_funda_da_erro_sem_derrubar() {
    let e = rodar(
        "local function f(n) return 1 + f(n + 1) end\nfunction transformar() return {corresponde = f(1) > 0} end",
    )
    .expect_err("recursão");
    assert!(
        matches!(
            e,
            ErroDoScript::Execucao(_) | ErroDoScript::Memoria | ErroDoScript::Instrucoes
        ),
        "{e:?}"
    );
    // O processo segue vivo e o executor continua útil.
    assert!(rodar(&corpo("")).is_ok());
}

#[test]
fn alocacao_gigante_vira_memoria() {
    for codigo in [
        "local s = string.rep('x', 1000000000) return {corresponde = #s > 0}",
        "local t = {} for i = 1, 100000000 do t[i] = {i} end",
        "local s = 'x' while true do s = s .. s end",
        "local b = buffer.create(1000000000)",
    ] {
        let p = pedido_com(
            &corpo(codigo),
            json!({}),
            json!({}),
            Limites {
                memoria_bytes: 8 * 1024 * 1024,
                tempo_ms: 20_000,
                instrucoes: u64::MAX,
                ..Limites::default()
            },
        );
        let e = ExecutorNoProcesso::novo().executar(&p).expect_err(codigo);
        assert_eq!(e, ErroDoScript::Memoria, "{codigo}");
    }
}

#[test]
fn erros_de_forma_do_script() {
    assert!(matches!(
        rodar("function transformar( end"),
        Err(ErroDoScript::Sintaxe(_))
    ));
    assert_eq!(rodar("x = 1"), Err(ErroDoScript::SemTransformar));
    assert_eq!(rodar("transformar = 5"), Err(ErroDoScript::SemTransformar));
    assert!(
        matches!(rodar(&corpo("error('quebrou')")), Err(ErroDoScript::Execucao(m)) if m.contains("quebrou"))
    );
    assert!(matches!(
        rodar(&corpo("local x = nil; return x.y")),
        Err(ErroDoScript::Execucao(_))
    ));
}

#[test]
fn retorno_invalido() {
    let casos = [
        "function transformar() return 5 end",
        "function transformar() return {} end",
        "function transformar() return {corresponde = 'sim'} end",
        "function transformar() return {corresponde = true, extra = 1} end",
        "function transformar() return {corresponde = true, campos = 7} end",
        "function transformar() return {corresponde = true, campos = {a = {}}} end",
        "function transformar() return {corresponde = true, campos = {a = 1.5}} end",
        "function transformar() return {corresponde = true, campos = {a = 0/0}} end",
        "function transformar() return {corresponde = true, motivos = 'x'} end",
        "function transformar() return {corresponde = true, motivos = {1}} end",
        "function transformar() return {corresponde = true, motivos = {a = 'x'}} end",
        "function transformar() return {corresponde = true, revisar = 1} end",
        "function transformar() return {corresponde = true, tipo_documental = 3} end",
        // centavos: não inteiro, texto, booleano
        "function transformar() return {corresponde = true, campos = {valor_centavos = 10.5}} end",
        "function transformar() return {corresponde = true, campos = {valor_centavos = '10'}} end",
        "function transformar() return {corresponde = true, campos = {valor_centavos = true}} end",
        "function transformar() return {corresponde = true, campos = {valor_centavos = 1/0}} end",
        "function transformar() return {corresponde = true, campos = {valor_centavos = 2^60}} end",
    ];
    for c in casos {
        assert!(
            matches!(rodar(c), Err(ErroDoScript::Retorno(_))),
            "{c}: {:?}",
            rodar(c)
        );
    }
}

#[test]
fn centavos_inteiros_passam() {
    let r = rodar("function transformar() return {corresponde = true, campos = {a_centavos = -12300, b_centavos = 10.0}} end")
        .expect("roda");
    assert_eq!(r.campos["a_centavos"], json!(-12_300));
    assert_eq!(r.campos["b_centavos"], json!(10));
}

#[test]
fn ajuda_pelo_script() {
    let r = rodar(
        "function transformar(e, p, ajuda) return {corresponde = true, campos = {
          a_centavos = ajuda.mul_div_arred(5, 1, 2), b_centavos = ajuda.mul_div_arred(-5, 1, 2),
          c_centavos = ajuda.mul_div_arred(1000, 75000, 1000000), d_centavos = ajuda.centavos('R$ -12,30')}} end",
    )
    .expect("roda");
    assert_eq!(r.campos["a_centavos"], json!(3));
    assert_eq!(r.campos["b_centavos"], json!(-3));
    assert_eq!(r.campos["c_centavos"], json!(75));
    assert_eq!(r.campos["d_centavos"], json!(-1230));
    for c in [
        "ajuda.mul_div_arred(1, 2, 0)",
        "ajuda.mul_div_arred(1.5, 2, 3)",
        "ajuda.mul_div_arred('1', 2, 3)",
        "ajuda.mul_div_arred(1, 2)",
        "ajuda.mul_div_arred(2^60, 1, 1)",
        "ajuda.centavos('abc')",
        "ajuda.data('31/02/2026')",
    ] {
        let e = rodar(&corpo(c)).expect_err(c);
        assert!(matches!(e, ErroDoScript::Execucao(_)), "{c}: {e:?}");
    }
}

#[test]
fn pedido_e_resposta_grandes() {
    let p = pedido_com(
        &corpo(""),
        json!({"texto": "x".repeat(2000)}),
        json!({}),
        Limites {
            entrada_bytes: 1000,
            ..Limites::default()
        },
    );
    assert!(matches!(
        ExecutorNoProcesso::novo().executar(&p),
        Err(ErroDoScript::Grande(_))
    ));
    let p = pedido_com(
        "function transformar() return {corresponde = true, campos = {t = string.rep('x', 5000)}} end",
        json!({}),
        json!({}),
        Limites {
            saida_bytes: 1000,
            ..Limites::default()
        },
    );
    assert!(matches!(
        ExecutorNoProcesso::novo().executar(&p),
        Err(ErroDoScript::Grande(_))
    ));
}

#[test]
fn verificar_compila_e_confere_transformar() {
    let ex = ExecutorNoProcesso::novo();
    assert_eq!(ex.verificar(&corpo("")), Ok(()));
    assert!(matches!(
        ex.verificar("function transformar( end"),
        Err(ErroDoScript::Sintaxe(_))
    ));
    assert_eq!(
        ex.verificar("local x = 1"),
        Err(ErroDoScript::SemTransformar)
    );
    // O corpo é executado com limite curto: laço no topo não trava o editor.
    assert!(ex.verificar("while true do end").is_err());
    // Não roda `transformar`.
    assert_eq!(
        ex.verificar("function transformar() while true do end end"),
        Ok(())
    );
}

#[test]
fn determinismo_cem_execucoes() {
    let p = pedido_com(
        "function transformar(e, p, a)
           local soma = 0
           for k, v in pairs(e.campos) do soma = soma + v end
           local ks = {}
           for k in pairs(e.campos) do table.insert(ks, k) end
           table.sort(ks)
           return {corresponde = true, campos = {soma_centavos = soma, chaves = table.concat(ks, ',')},
                   motivos = ks}
         end",
        json!({"campos": {"z": 3, "a": 1, "m": 2, "b": 10, "c": 20}}),
        json!({}),
        Limites::default(),
    );
    let ex = ExecutorNoProcesso::novo();
    let primeira = ex.executar(&p).expect("1");
    for _ in 0..100 {
        assert_eq!(ex.executar(&p).expect("n"), primeira);
    }
}

// ---- `exibir`: linhas curtas para o cartão do Reconhecimento ----

#[test]
fn exibir_chega_na_resposta_e_ausente_vale_vazio() {
    let r = rodar(
        r#"function transformar() return {corresponde = true, exibir = {"primeira", "segunda"}} end"#,
    )
    .expect("roda");
    assert_eq!(r.exibir, ["primeira", "segunda"]);
    let sem = rodar("function transformar() return {corresponde = true} end").expect("roda");
    assert!(sem.exibir.is_empty());
}

#[test]
fn exibir_aceita_o_limite_exato() {
    let r = rodar(
        r#"function transformar()
  local linha = string.rep("é", 160)
  return {corresponde = true, exibir = {linha, linha, linha, linha, linha}}
end"#,
    )
    .expect("cinco linhas de 160 caracteres cabem");
    assert_eq!(r.exibir.len(), 5);
    assert!(r.exibir.iter().all(|l| l.chars().count() == 160));
}

#[test]
fn exibir_fora_do_limite_ou_que_nao_e_texto_e_retorno_invalido() {
    let casos = [
        // seis linhas
        r#"return {corresponde = true, exibir = {"a", "b", "c", "d", "e", "f"}}"#,
        // uma linha com 161 caracteres (conta caracteres, não bytes)
        r#"return {corresponde = true, exibir = {string.rep("é", 161)}}"#,
        // não é texto
        r"return {corresponde = true, exibir = {1}}",
        r"return {corresponde = true, exibir = {true}}",
        r"return {corresponde = true, exibir = {{}}}",
        // não é lista
        r#"return {corresponde = true, exibir = "solto"}"#,
        r#"return {corresponde = true, exibir = {a = "x"}}"#,
    ];
    for corpo in casos {
        let fonte = format!("function transformar() {corpo} end");
        match rodar(&fonte) {
            Err(ErroDoScript::Retorno(_)) => {}
            outro => panic!("{corpo}: esperava Retorno, veio {outro:?}"),
        }
    }
}
