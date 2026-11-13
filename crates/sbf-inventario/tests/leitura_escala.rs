//! INV-16 na escala do inventário real: 300 mil achados, 600 mil origens, 57 mil e-mails de
//! 1.000 remetentes. `#[ignore]`: gera cerca de 1 milhão de linhas. Rode com
//! `cargo test --release -p sbf-inventario --test leitura_escala -- --ignored --nocapture`.

mod comum;

use std::time::{Duration, Instant};

use rusqlite::Connection;
use sbf_inventario::consulta::{Area, Consulta, Filtro, Fonte, PedidoDeTabela, Pergunta};
use sbf_inventario::leitor::LeitorSqlite;

fn medir<T>(nome: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    let d = t.elapsed();
    eprintln!("{nome:<56} {:>7} ms", d.as_millis());
    (r, d)
}

fn area(a: Area) -> PedidoDeTabela {
    PedidoDeTabela::Area {
        area: a,
        filtro: Filtro::default(),
    }
}

#[test]
#[ignore = "gera 1 milhão de linhas; rode com --ignored"]
fn medidas_na_escala_do_inventario_real() {
    let banco = comum::banco_de_teste();
    let ((), d) = medir("gerar a fixture", || {
        let conn = Connection::open(&banco.caminho).expect("conexão");
        comum::popular_escala_real(&conn);
    });
    let _ = d;
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_hoje("2026-11-13");
    for a in [
        Area::EmailsPorRemetente,
        Area::Emails,
        Area::ArquivosEPastas,
        Area::PastasPessoais,
        Area::EmDuvida,
        Area::ATriar,
        Area::Documentos,
        Area::Registros,
        Area::Fornecedores,
        Area::Ciclos,
    ] {
        let (t, _) = medir(&format!("tabela {a:?}"), || {
            leitor.tabela(&area(a)).expect("tabela")
        });
        eprintln!("    {} linhas", t.linhas.len());
    }
    medir("visão geral", || leitor.visao_geral().expect("painel"));
    medir("contagens", || leitor.contagens().expect("contagens"));
    for p in [
        Pergunta::QuantoSaiu,
        Pergunta::PossiveisDuplicidades,
        Pergunta::NaoClassificado,
    ] {
        medir(&format!("relatório {p:?}"), || {
            leitor
                .relatorio(p, Fonte::Inventario, &Filtro::default())
                .expect("relatório")
        });
    }
}
