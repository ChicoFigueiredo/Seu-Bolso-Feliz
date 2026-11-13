//! Teste de ouro do equivalente mensal da recorrência contra `equivalenteMensal` de
//! `apps/web/src/app/actions/painel-recorrentes.ts` (fixture `ouro/recorrencia.ts`).
//! O TS devolve reais em ponto flutuante; o Rust, a fração exata de centavos. Os dois
//! têm de coincidir até a sexta casa e arredondar para o mesmo centavo, salvo no meio
//! centavo exato, onde o TS erra para baixo por ponto flutuante e o Rust não.

use serde::Deserialize;

use sbf_livro::recorrencia::equivalente_mensal;
use sbf_livro::{Centavos, Frequencia};

#[derive(Debug, Deserialize)]
struct Fixture {
    casos: Vec<Caso>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Caso {
    frequencia: Frequencia,
    valor_centavos: i64,
    intervalo: Option<i32>,
    equivalente_centavos: Option<f64>,
}

#[test]
fn bate_com_o_ts_em_toda_frequencia() {
    let caminho = format!(
        "{}/ouro/fixtures/recorrencia.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let fixture: Fixture =
        serde_json::from_str(&std::fs::read_to_string(caminho).expect("fixture")).expect("json");
    assert_eq!(fixture.casos.len(), 5 * 11 + 11 * 11);
    for caso in &fixture.casos {
        let rust = equivalente_mensal(
            Centavos(caso.valor_centavos),
            caso.frequencia,
            caso.intervalo,
        );
        match (rust, caso.equivalente_centavos) {
            (None, None) => {}
            (Some(f), Some(ts)) => {
                let exato = f.como_f64();
                assert!(
                    (exato - ts).abs() <= 1e-6 * ts.abs().max(1.0),
                    "{caso:?}: {exato}"
                );
                // Meio centavo exato (ex.: 1.234.567,89 quinzenal = 2.674.897,095): o TS
                // chega a ...094999 por erro de ponto flutuante; a fração do Rust é exata.
                let empate = ((ts - ts.floor()) - 0.5).abs() < 1e-6;
                if !empate {
                    #[allow(clippy::cast_possible_truncation)]
                    let ts_arredondado = ts.round() as i64;
                    assert_eq!(f.arredondar(), Centavos(ts_arredondado), "{caso:?}");
                }
            }
            outro => panic!("{caso:?}: {outro:?}"),
        }
    }
}
