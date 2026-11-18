//! O Cálculo do Modelo de compromisso: `calcular(entrada, ajuda)` em lote, no processo e no
//! processo filho.

use std::path::PathBuf;

use sbf_regras::script::{ErroDoScript, ExecutorDeScript, ItemDeCalculo, Limites, PedidoDeCalculo};
use sbf_script::{ExecutorFilho, ExecutorNoProcesso};
use serde_json::json;

const DIARISTA: &str = r#"
function calcular(entrada, ajuda)
  local ano = tonumber(string.sub(entrada.competencia, 1, 4))
  local mes = tonumber(string.sub(entrada.competencia, 6, 7))
  local n = entrada.quantidade_ajustada
  if n == nil then
    n = ajuda.dias_da_semana_no_mes(ano, mes, entrada.parametros.dia_da_semana)
  end
  local diaria = entrada.parametros.diaria
  return {
    valor_centavos = diaria * n,
    quantidade = n,
    explicacao = "diária " .. ajuda.reais(diaria) .. " × " .. n .. " " ..
      ajuda.nome_do_dia_da_semana(entrada.parametros.dia_da_semana) .. (n == 1 and "" or "s"),
  }
end
"#;

fn item(competencia: &str, diaria: i64, ajustada: Option<i64>) -> ItemDeCalculo {
    ItemDeCalculo {
        competencia: competencia.into(),
        parametros: json!({"diaria": diaria, "dia_da_semana": 5}),
        quantidade_ajustada: ajustada,
    }
}

fn pedido(fonte: &str, itens: Vec<ItemDeCalculo>) -> PedidoDeCalculo {
    PedidoDeCalculo {
        fonte: fonte.into(),
        checksum: "teste".into(),
        itens,
        limites: Limites::default(),
    }
}

#[test]
fn diaria_vezes_as_sextas_de_meses_com_quatro_e_cinco() {
    let r = ExecutorNoProcesso::novo()
        .calcular(&pedido(
            DIARISTA,
            vec![
                item("2026-04", 19_000, None),
                item("2026-05", 21_000, None),
                item("2026-01", 21_000, Some(6)),
            ],
        ))
        .expect("lote");
    let r: Vec<_> = r.into_iter().map(|x| x.expect("mês")).collect();
    assert_eq!(r[0].valor_centavos, 76_000);
    assert_eq!(r[0].quantidade, Some(4));
    assert_eq!(r[0].explicacao, "diária R$ 190,00 × 4 sextas");
    assert_eq!(r[1].valor_centavos, 105_000);
    assert_eq!(r[1].quantidade, Some(5));
    assert_eq!(r[1].explicacao, "diária R$ 210,00 × 5 sextas");
    // Quantidade ajustada vale no lugar das sextas do mês.
    assert_eq!(r[2].valor_centavos, 126_000);
    assert_eq!(r[2].quantidade, Some(6));
}

#[test]
fn um_mes_que_falha_nao_derruba_os_outros() {
    let fonte = r#"
function calcular(entrada, ajuda)
  if entrada.competencia == "2026-05" then error("quebrou") end
  return { valor_centavos = 100, explicacao = "ok" }
end
"#;
    let r = ExecutorNoProcesso::novo()
        .calcular(&pedido(
            fonte,
            vec![
                item("2026-04", 1, None),
                item("2026-05", 1, None),
                item("2026-06", 1, None),
            ],
        ))
        .expect("lote");
    assert!(r[0].is_ok());
    assert!(matches!(r[1], Err(ErroDoScript::Execucao(_))));
    assert!(r[2].is_ok());
}

#[test]
fn retorno_invalido_e_recusado_por_mes() {
    let casos = [
        "return { valor_centavos = 1.5, explicacao = 'x' }",
        "return { valor_centavos = -1, explicacao = 'x' }",
        "return { explicacao = 'x' }",
        "return { valor_centavos = 1 }",
        "return { valor_centavos = 1, explicacao = 'x', outra = 1 }",
        "return 5",
        "return { valor_centavos = 1, explicacao = string.rep('a', 201) }",
    ];
    for corpo in casos {
        let fonte = format!("function calcular(entrada, ajuda) {corpo} end");
        let r = ExecutorNoProcesso::novo()
            .calcular(&pedido(&fonte, vec![item("2026-05", 1, None)]))
            .expect("lote");
        assert!(
            matches!(r[0], Err(ErroDoScript::Retorno(_))),
            "{corpo}: {:?}",
            r[0]
        );
    }
}

#[test]
fn sem_a_funcao_calcular_cada_mes_acusa_e_a_verificacao_tambem() {
    let ex = ExecutorNoProcesso::novo();
    let r = ex
        .calcular(&pedido("x = 1", vec![item("2026-05", 1, None)]))
        .expect("lote");
    assert_eq!(r[0], Err(ErroDoScript::SemCalcular));
    assert_eq!(
        ex.verificar_calculo("x = 1"),
        Err(ErroDoScript::SemCalcular)
    );
    assert!(matches!(
        ex.verificar_calculo("function calcular("),
        Err(ErroDoScript::Sintaxe(_))
    ));
    assert_eq!(ex.verificar_calculo(DIARISTA), Ok(()));
}

#[test]
fn o_calculo_tem_o_mesmo_isolamento_e_limites_das_regras() {
    // Laço infinito: estoura as instruções naquele mês, e o seguinte roda.
    let fonte = r#"
function calcular(entrada, ajuda)
  if entrada.competencia == "2026-05" then while true do end end
  return { valor_centavos = 7, explicacao = "ok" }
end
"#;
    let mut p = pedido(
        fonte,
        vec![item("2026-05", 1, None), item("2026-06", 1, None)],
    );
    p.limites.instrucoes = 10_000;
    let r = ExecutorNoProcesso::novo().calcular(&p).expect("lote");
    assert!(matches!(
        r[0],
        Err(ErroDoScript::Instrucoes | ErroDoScript::Tempo)
    ));
    assert_eq!(r[1].as_ref().map(|x| x.valor_centavos), Ok(7));
    // Sem io/os e sem mexer na entrada.
    for corpo in [
        "io.open('x')",
        "os.time()",
        "entrada.competencia = 'x'",
        "ajuda.reais = nil",
    ] {
        let fonte = format!(
            "function calcular(entrada, ajuda) {corpo} return {{valor_centavos = 1, explicacao = 'x'}} end"
        );
        let r = ExecutorNoProcesso::novo()
            .calcular(&pedido(&fonte, vec![item("2026-05", 1, None)]))
            .expect("lote");
        assert!(matches!(r[0], Err(ErroDoScript::Execucao(_))), "{corpo}");
    }
}

#[test]
fn bytecode_e_recusado() {
    let r = ExecutorNoProcesso::novo()
        .calcular(&pedido("\u{1}\u{2}", vec![item("2026-05", 1, None)]))
        .expect("lote");
    assert!(matches!(r[0], Err(ErroDoScript::Sintaxe(_))));
}

#[test]
fn lote_grande_demais_e_recusado_inteiro() {
    let mut p = pedido(DIARISTA, vec![item("2026-05", 1, None)]);
    p.limites.entrada_bytes = 50;
    assert!(matches!(
        ExecutorNoProcesso::novo().calcular(&p),
        Err(ErroDoScript::Grande(_))
    ));
}

fn filho() -> ExecutorFilho {
    ExecutorFilho::novo(
        PathBuf::from(env!("CARGO_BIN_EXE_sbf-script-filho")),
        vec![],
    )
}

#[test]
fn o_processo_filho_calcula_o_lote_de_360_meses() {
    let ex = filho();
    let itens: Vec<ItemDeCalculo> = (0..360)
        .map(|i| {
            let ano = 2026 + i / 12;
            let mes = i % 12 + 1;
            item(&format!("{ano:04}-{mes:02}"), 21_000, None)
        })
        .collect();
    let inicio = std::time::Instant::now();
    let r = ex.calcular(&pedido(DIARISTA, itens)).expect("lote");
    eprintln!("360 meses no filho: {:?}", inicio.elapsed());
    assert_eq!(r.len(), 360);
    assert!(r.iter().all(Result::is_ok));
    // 2026-05 é o índice 4: 5 sextas.
    assert_eq!(r[4].as_ref().map(|x| x.valor_centavos), Ok(105_000));
    assert_eq!(ex.verificar_calculo(DIARISTA), Ok(()));
    assert_eq!(
        ex.verificar_calculo("x = 1"),
        Err(ErroDoScript::SemCalcular)
    );
}
