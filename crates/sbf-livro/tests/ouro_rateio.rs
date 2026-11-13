//! Teste de ouro da direção do rateio contra
//! `packages/domain/src/allocation/transaction-direction.ts` (fixture `ouro/rateio.ts`).
//!
//! Divergência intencional (grill Q15): `adjustment` não entra em lado nenhum do centro
//! (o TS o conta como saída) e, por isso, não aceita rateio (o TS aceita).

use serde::Deserialize;

use sbf_livro::TipoDeMovimentacao;
use sbf_livro::rateio::{Direcao, aceita_rateio, direcao_do_codigo, direcao_no_centro};

#[derive(Debug, Deserialize)]
struct Fixture {
    casos: Vec<Caso>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Caso {
    tipo: String,
    direcao: String,
    aceita_rateio: bool,
}

fn fixture() -> Fixture {
    let caminho = format!("{}/ouro/fixtures/rateio.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(caminho).expect("fixture")).expect("json")
}

fn do_ts(direcao: &str) -> Option<Direcao> {
    match direcao {
        "inflow" => Some(Direcao::Entrada),
        "outflow" => Some(Direcao::Saida),
        _ => None,
    }
}

#[test]
fn bate_com_o_ts_em_todos_os_tipos_menos_o_ajuste() {
    let casos = fixture().casos;
    assert_eq!(casos.len(), 9);
    for caso in casos.iter().filter(|c| c.tipo != "adjustment") {
        assert_eq!(
            direcao_do_codigo(&caso.tipo),
            do_ts(&caso.direcao),
            "{}",
            caso.tipo
        );
        if let Some(tipo) = TipoDeMovimentacao::de_codigo(&caso.tipo) {
            assert_eq!(
                direcao_no_centro(tipo),
                do_ts(&caso.direcao),
                "{}",
                caso.tipo
            );
            assert_eq!(aceita_rateio(tipo), caso.aceita_rateio, "{}", caso.tipo);
        }
    }
}

#[test]
fn ajuste_fica_fora_do_centro_divergindo_do_ts_de_proposito() {
    let ajuste = fixture()
        .casos
        .into_iter()
        .find(|c| c.tipo == "adjustment")
        .expect("caso do ajuste");
    // O TS conta como saída e aceita rateio...
    assert_eq!(
        (ajuste.direcao.as_str(), ajuste.aceita_rateio),
        ("outflow", true)
    );
    // ... o Rust, pelo grill Q15, não.
    assert_eq!(direcao_no_centro(TipoDeMovimentacao::Ajuste), None);
    assert_eq!(direcao_do_codigo("adjustment"), None);
    assert!(!aceita_rateio(TipoDeMovimentacao::Ajuste));
}
