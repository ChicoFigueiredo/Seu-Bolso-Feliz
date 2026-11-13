//! Teste de ouro da amortização (EST-13): SAC, Price e Misto, saldo devedor, juros totais
//! e quitação antecipada contra `packages/domain/src/amortization/index.ts`, em 15 casos
//! escolhidos e 60 sorteados com semente fixa (`ouro/amortizacao.ts`). Centavo a centavo.

use serde::Deserialize;

use sbf_livro::amortizacao::{
    ConfiguracaoDeAmortizacao, Sistema, cronograma, quitacao_antecipada, saldo_devedor_apos,
    total_de_juros,
};
use sbf_livro::{Centavos, Taxa};

#[derive(Debug, Deserialize)]
struct Fixture {
    casos: Vec<Caso>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Caso {
    entrada: Entrada,
    cronograma: Vec<[i64; 7]>,
    total_de_juros: i64,
    marcos: Vec<Marco>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entrada {
    sistema: String,
    principal: i64,
    taxa_milionesimos: i64,
    parcelas: u32,
    seguro: i64,
    tarifa: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marco {
    apos: u32,
    saldo_devedor_apos: i64,
    quitacao: [i64; 4],
}

fn configuracao(e: &Entrada) -> ConfiguracaoDeAmortizacao {
    ConfiguracaoDeAmortizacao {
        sistema: match e.sistema.as_str() {
            "sac" => Sistema::Sac,
            "price" => Sistema::Price,
            _ => Sistema::Misto,
        },
        principal: Centavos(e.principal),
        taxa_mensal: Taxa {
            milionesimos: e.taxa_milionesimos,
        },
        parcelas: e.parcelas,
        seguro_mensal: Centavos(e.seguro),
        tarifa_mensal: Centavos(e.tarifa),
    }
}

#[test]
fn bate_centavo_a_centavo_com_o_ts() {
    let caminho = format!(
        "{}/ouro/fixtures/amortizacao.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let fixture: Fixture =
        serde_json::from_str(&std::fs::read_to_string(caminho).expect("fixture")).expect("json");
    assert_eq!(fixture.casos.len(), 75);
    let mut parcelas_conferidas = 0;
    for (i, caso) in fixture.casos.iter().enumerate() {
        let cfg = configuracao(&caso.entrada);
        let rust: Vec<[i64; 7]> = cronograma(&cfg)
            .iter()
            .map(|p| {
                [
                    i64::from(p.numero),
                    p.total.0,
                    p.principal.0,
                    p.juros.0,
                    p.seguro.0,
                    p.tarifa.0,
                    p.saldo_devedor.0,
                ]
            })
            .collect();
        assert_eq!(rust, caso.cronograma, "caso {i}: {:?}", caso.entrada);
        parcelas_conferidas += rust.len();
        assert_eq!(total_de_juros(&cfg).0, caso.total_de_juros, "caso {i}");
        for m in &caso.marcos {
            assert_eq!(
                saldo_devedor_apos(&cfg, m.apos).0,
                m.saldo_devedor_apos,
                "caso {i} após {}",
                m.apos
            );
            let q = quitacao_antecipada(&cfg, m.apos);
            assert_eq!(
                [
                    q.saldo_devedor.0,
                    q.total_pago.0,
                    q.total_de_juros.0,
                    q.juros_economizados.0
                ],
                m.quitacao,
                "caso {i} quitação após {}",
                m.apos
            );
        }
    }
    assert!(parcelas_conferidas > 3000, "{parcelas_conferidas}");
}
