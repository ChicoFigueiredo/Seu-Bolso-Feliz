//! O script de fábrica do IRRF, contra a tabela oficial de 2026 (irrf-2026-01).
//! Os esperados foram calculados à mão (comentados em cada caso), em centavos.

mod comum;

use comum::pedido_com;
use sbf_regras::script::{ErroDoScript, ExecutorDeScript, Limites, RespostaDoScript};
use sbf_script::ExecutorNoProcesso;
use serde_json::{Value, json};

const SCRIPT: &str = include_str!("../../sbf-regras/fabrica/irrf.luau");
const TABELA: &str = include_str!("../../sbf-regras/fabrica/irrf-2026-01.json");

fn rodar(campos: Value) -> Result<RespostaDoScript, ErroDoScript> {
    let tabela: Value = serde_json::from_str(TABELA).expect("tabela");
    let p = pedido_com(
        SCRIPT,
        json!({"tipo_documental": "contracheque", "canal": "pasta", "campos": campos}),
        json!({"irrf": tabela}),
        Limites::default(),
    );
    ExecutorNoProcesso::novo().executar(&p)
}

fn estimado(base: i64, rendimento: Option<i64>) -> i64 {
    let mut campos = json!({"base_irrf_centavos": base, "irrf_retido_centavos": 0});
    if let Some(r) = rendimento {
        campos["rendimento_tributavel_centavos"] = json!(r);
    }
    let r = rodar(campos).expect("roda");
    r.campos["irrf_estimado_centavos"]
        .as_i64()
        .expect("estimado")
}

#[test]
fn o_script_de_fabrica_passa_na_verificacao() {
    assert_eq!(ExecutorNoProcesso::novo().verificar(SCRIPT), Ok(()));
}

#[test]
fn fronteiras_da_tabela_e_do_redutor() {
    // Faixa 1 acaba em 2.428,80: isento.
    assert_eq!(estimado(242_880, Some(242_880)), 0);
    // 2.428,81 cai na faixa de 7,5%: 242881*0,075 = 18216,075 -> 18216; menos 18216 = 0.
    assert_eq!(estimado(242_881, Some(242_881)), 0);
    // 4.664,68 ainda é 22,5%: 466468*0,225 = 104955,3 -> 104955; menos 67549 = 37406.
    // Rendimento <= 5.000,00: redutor = min(37406; 31289) = 31289. Estimado 6117.
    assert_eq!(estimado(466_468, Some(466_468)), 6_117);
    // 5.000,00 na faixa de 27,5%: 137500 - 90873 = 46627; redutor 31289 -> 15338.
    assert_eq!(estimado(500_000, Some(500_000)), 15_338);
    // 5.000,01: imposto = 500001*0,275 = 137500,275 -> 137500; 46627.
    // Redutor = 97862 - 500001*0,133145 (66572,633 -> 66573) = 31289. Estimado 15338.
    assert_eq!(estimado(500_001, Some(500_001)), 15_338);
    // Meio da faixa do redutor, 6.000,00: imposto 165000 - 90873 = 74127;
    // redutor 97862 - 79887 = 17975; estimado 56152.
    assert_eq!(estimado(600_000, Some(600_000)), 56_152);
    // 7.350,00: imposto 735000*0,275 = 202125 - 90873 = 111252;
    // redutor 97862 - 97862,175(-> 97862) = 0.
    assert_eq!(estimado(735_000, Some(735_000)), 111_252);
    // 7.350,01: imposto 202125,275 -> 202125 - 90873 = 111252; sem redutor.
    assert_eq!(estimado(735_001, Some(735_001)), 111_252);
    // Base alta sem rendimento informado: redutor é 0 com certeza.
    // 10.000,00: 275000 - 90873 = 184127.
    assert_eq!(estimado(1_000_000, None), 184_127);
}

#[test]
fn redutor_nunca_passa_do_imposto() {
    // Base com imposto pequeno e rendimento menor que 5.000: o redutor é o próprio imposto.
    // 3.000,00 na faixa de 15%: 300000*0,15 = 45000 - 39416 = 5584 -> redutor 5584 -> 0.
    assert_eq!(estimado(300_000, Some(300_000)), 0);
    // Rendimento maior que a base (13o, adiantamentos...): usa o rendimento no redutor.
    // base 4.664,68 (imposto 37406) e rendimento 6.000,00 (redutor 17975): 19431.
    assert_eq!(estimado(466_468, Some(600_000)), 19_431);
}

#[test]
fn diferenca_e_revisao() {
    // Retido = estimado: diferença 0, sem revisão, evidências com a versão da tabela.
    let r = rodar(
        json!({"base_irrf_centavos": 600_000, "irrf_retido_centavos": 56_152,
                          "rendimento_tributavel_centavos": 600_000}),
    )
    .expect("roda");
    assert_eq!(r.campos["irrf_diferenca_centavos"], json!(0));
    assert!(r.corresponde && !r.revisar);
    assert_eq!(r.evidencias, ["rubrica_irrf", "tabela:irrf:irrf-2026-01"]);
    // Retido a mais: diferença positiva (retido - estimado) e revisar.
    let r = rodar(
        json!({"base_irrf_centavos": 600_000, "irrf_retido_centavos": 60_000,
                          "rendimento_tributavel_centavos": 600_000}),
    )
    .expect("roda");
    assert_eq!(r.campos["irrf_diferenca_centavos"], json!(3_848));
    assert!(r.revisar);
    // Retido a menos: diferença negativa.
    let r = rodar(
        json!({"base_irrf_centavos": 600_000, "irrf_retido_centavos": 0,
                          "rendimento_tributavel_centavos": 600_000}),
    )
    .expect("roda");
    assert_eq!(r.campos["irrf_diferenca_centavos"], json!(-56_152));
    assert!(r.revisar);
}

#[test]
fn dados_faltando_pedem_revisao_sem_inventar() {
    let r = rodar(json!({"irrf_retido_centavos": 100})).expect("roda");
    assert!(r.corresponde && r.revisar && r.campos.is_empty());
    assert!(r.motivos[0].contains("base"));
    // Base abaixo de 7.350,00 sem rendimento: o redutor é incerto.
    let r = rodar(json!({"base_irrf_centavos": 500_000, "irrf_retido_centavos": 0})).expect("roda");
    assert!(r.revisar && r.campos.is_empty());
    assert_eq!(r.motivos, ["falta o rendimento tributável para o redutor"]);
}
