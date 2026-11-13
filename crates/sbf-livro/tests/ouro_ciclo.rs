//! Teste de ouro do ciclo (EST-12): o `Ciclo` do Rust contra o `getCurrentPeriod` do TS
//! (`packages/domain/src/financial-cycle/index.ts`), dia a dia de 2024 a 2026, em cinco
//! horários de São Paulo e sete dias de corte. Fixtures geradas por `ouro/ciclo.ts`.
//!
//! - `ciclo-sao-paulo.json`: o TS rodando em `TZ=America/Sao_Paulo`. O Rust bate em tudo.
//! - `ciclo-utc.json`: o TS rodando em `TZ=UTC`, como o web roda na Vercel. É o erro de
//!   fuso do web, documentado aqui: o Rust **não** reproduz.

use serde::Deserialize;

use sbf_livro::ciclo::{Ciclo, DiaDeCorte};
use sbf_livro::data::Data;

#[derive(Debug, Deserialize)]
struct Fixture {
    casos: Vec<Caso>,
}

#[derive(Debug, Deserialize)]
struct Caso {
    corte: u8,
    hora: String,
    trechos: Vec<Trecho>,
}

#[derive(Debug, Deserialize)]
struct Trecho {
    desde: String,
    inicio: String,
    fim: String,
    rotulo: String,
}

fn ler(nome: &str) -> Fixture {
    let caminho = format!("{}/ouro/fixtures/{nome}", env!("CARGO_MANIFEST_DIR"));
    let texto = std::fs::read_to_string(&caminho).expect("fixture de ouro");
    serde_json::from_str(&texto).expect("fixture válida")
}

fn data(t: &str) -> Data {
    Data::de_iso(t).expect("data da fixture")
}

/// O instante Unix do dia `d` às `hora` de São Paulo (UTC−3).
fn instante(d: Data, hora: &str) -> i64 {
    let (h, m) = hora.split_once(':').expect("HH:MM");
    let h: i64 = h.parse().expect("hora");
    let m: i64 = m.parse().expect("minuto");
    d.dias_desde_1970() * 86_400 + (h + 3) * 3600 + m * 60
}

/// Cada dia de 2024 a 2026 com o trecho da fixture que o cobre.
fn dia_a_dia(caso: &Caso) -> Vec<(Data, &Trecho)> {
    let ultimo = data("2026-12-31");
    let mut dias = Vec::new();
    for (i, trecho) in caso.trechos.iter().enumerate() {
        let ate = caso
            .trechos
            .get(i + 1)
            .map_or(ultimo, |prox| data(&prox.desde).somar_dias(-1));
        let mut d = data(&trecho.desde);
        while d <= ate {
            dias.push((d, trecho));
            d = d.somar_dias(1);
        }
    }
    dias
}

#[test]
fn bate_com_o_ts_em_sao_paulo_dia_a_dia_de_2024_a_2026() {
    let fixture = ler("ciclo-sao-paulo.json");
    assert_eq!(fixture.casos.len(), 35);
    let mut conferidos = 0;
    for caso in &fixture.casos {
        let corte = DiaDeCorte::novo(caso.corte).expect("corte");
        let dias = dia_a_dia(caso);
        assert_eq!(dias.len(), 1096, "2024 a 2026 tem 1096 dias");
        for (d, trecho) in dias {
            let c = Ciclo::do_instante(instante(d, &caso.hora), corte);
            assert_eq!(
                (c.inicio.to_string(), c.fim.to_string(), c.rotulo()),
                (
                    trecho.inicio.clone(),
                    trecho.fim.clone(),
                    trecho.rotulo.clone()
                ),
                "corte {} às {} em {d}",
                caso.corte,
                caso.hora
            );
            assert!(c.contem(d));
            conferidos += 1;
        }
    }
    assert_eq!(conferidos, 35 * 1096);
}

#[test]
fn bate_com_o_ciclo_do_inventario_da_fatia_1_no_corte_20() {
    let corte = DiaDeCorte::novo(20).expect("corte");
    let mut d = data("2024-01-01");
    while d <= data("2026-12-31") {
        let iso = d.to_string();
        assert_eq!(
            Some(Ciclo::da_data(d, corte).nome()),
            sbf_inventario::visao_geral::ciclo_da_data(&iso),
            "{iso}"
        );
        d = d.somar_dias(1);
    }
}

/// O erro de UTC do web (spec 07, "O que existe hoje"): `getCurrentPeriod` usa os getters
/// locais de `Date`; na Vercel o local é UTC, e o `paraIso` em São Paulo devolve o início
/// e o fim um dia antes. Das 21h às 23h59 de São Paulo o UTC já virou o dia seguinte.
#[test]
fn o_ts_em_utc_erra_e_o_rust_da_o_dia_de_sao_paulo() {
    let utc = ler("ciclo-utc.json");
    let corte20 = |hora: &str| {
        utc.casos
            .iter()
            .find(|c| c.corte == 20 && c.hora == hora)
            .expect("caso do corte 20")
    };
    let trecho_em = |caso: &Caso, d: Data| {
        dia_a_dia(caso)
            .into_iter()
            .find(|(x, _)| *x == d)
            .map(|(_, t)| (t.inicio.clone(), t.fim.clone()))
            .expect("dia coberto")
    };
    let corte = DiaDeCorte::PADRAO;

    // Meio-dia de 25/10/2026: o web diz 19/out–18/nov; o ciclo 2026-10 é 20/out–19/nov.
    let d = data("2026-10-25");
    assert_eq!(
        trecho_em(corte20("12:00"), d),
        ("2026-10-19".into(), "2026-11-18".into())
    );
    let c = Ciclo::do_instante(instante(d, "12:00"), corte);
    assert_eq!(
        (c.nome(), c.inicio, c.fim),
        ("2026-10".into(), data("2026-10-20"), data("2026-11-19"))
    );

    // 21h de 19/11/2026, último dia do ciclo 2026-10: o web já mostra o ciclo seguinte.
    let d = data("2026-11-19");
    assert_eq!(
        trecho_em(corte20("21:00"), d),
        ("2026-11-19".into(), "2026-12-18".into())
    );
    let c = Ciclo::do_instante(instante(d, "21:00"), corte);
    assert_eq!(c.nome(), "2026-10");
    let c = Ciclo::do_instante(instante(d, "23:59"), corte);
    assert_eq!(c.nome(), "2026-10");

    // Em UTC, o início sai sempre um dia antes do de São Paulo (todo dia, todo horário).
    let sp = ler("ciclo-sao-paulo.json");
    for (caso_utc, caso_sp) in utc.casos.iter().zip(&sp.casos) {
        for (t_utc, t_sp) in caso_utc.trechos.iter().zip(&caso_sp.trechos) {
            assert_eq!(
                data(&t_utc.inicio).somar_dias(1),
                data(&t_sp.inicio),
                "corte {} às {}",
                caso_sp.corte,
                caso_sp.hora
            );
        }
    }
}
