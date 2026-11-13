//! INV-16 na escala do inventário real: 300 mil achados, 600 mil origens, 57 mil e-mails de
//! 1.000 remetentes. `#[ignore]`: gera cerca de 1 milhão de linhas. Rode com
//! `cargo test --release -p sbf-inventario --test leitura_escala -- --ignored --nocapture`.

mod comum;

use std::time::{Duration, Instant};

use rusqlite::Connection;
use sbf_inventario::consulta::{Area, Consulta, Filtro, Fonte, PedidoDeTabela, Pergunta};
use sbf_inventario::leitor::LeitorSqlite;

/// 300 ms de INV-16, com folga para a máquina dividida com outros processos.
const ORCAMENTO_DA_PAGINA: Duration = Duration::from_millis(500);

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
    let (_, d) = medir("EmailsPorRemetente (a consulta que levava 130 s)", || {
        leitor
            .tabela(&area(Area::EmailsPorRemetente))
            .expect("tabela")
    });
    assert!(d < Duration::from_secs(1), "{d:?}");
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
    eprintln!("-- páginas de 1000 linhas (a janela pede a primeira e depois as seguintes)");
    for a in [
        Area::Emails,
        Area::ArquivosEPastas,
        Area::EmDuvida,
        Area::ATriar,
        Area::Documentos,
        Area::Registros,
    ] {
        let (p, d1) = medir(&format!("página 1 de {a:?}"), || {
            leitor.tabela_paginada(&area(a), 0, 1000).expect("página")
        });
        let (_, d2) = medir(&format!("página 30 de {a:?} (linha 30000)"), || {
            leitor
                .tabela_paginada(&area(a), 30_000, 1000)
                .expect("página")
        });
        assert!(
            d1 < ORCAMENTO_DA_PAGINA && d2 < ORCAMENTO_DA_PAGINA,
            "{a:?}: {d1:?} e {d2:?}"
        );
        eprintln!("    total {}", p.total);
    }
    let (_, d) = medir("última página de ArquivosEPastas (linha 540000)", || {
        leitor
            .tabela_paginada(&area(Area::ArquivosEPastas), 540_000, 1000)
            .expect("página")
    });
    assert!(d < ORCAMENTO_DA_PAGINA, "{d:?}");
    medir("última página de Emails (linha 56000)", || {
        leitor
            .tabela_paginada(&area(Area::Emails), 56_000, 1000)
            .expect("página")
    });
    for a in [Area::EmailsPorRemetente, Area::PastasPessoais] {
        medir(&format!("página 1 de {a:?} (calcula tudo)"), || {
            leitor.tabela_paginada(&area(a), 0, 1000).expect("página")
        });
        medir(&format!("página 2 de {a:?} (cache)"), || {
            leitor
                .tabela_paginada(&area(a), 1000, 1000)
                .expect("página")
        });
    }
    {
        let conn = Connection::open(&banco.caminho).expect("conexão");
        let sqls = [
            (
                "achados (uma passada)",
                "SELECT count(*), sum(caminho_acervo IS NOT NULL), sum(estado='documento') FROM achado",
            ),
            ("count origem", "SELECT count(*) FROM origem"),
            (
                "o: período por fonte",
                "SELECT fonte_id, count(*), substr(min(coalesce(nullif(local_mtime, ''), gmail_date)), 1, 10), substr(max(coalesce(nullif(local_mtime, ''), gmail_date)), 1, 10) FROM origem WHERE coalesce(local_mtime, gmail_date) >= '1995' GROUP BY fonte_id",
            ),
            (
                "d: documentos por fonte",
                "SELECT x.fonte_id, count(DISTINCT a.content_hash), min(a.data_chave), max(a.data_chave) FROM achado a CROSS JOIN origem x ON x.content_hash = a.content_hash WHERE a.estado = 'documento' AND a.data_chave >= '1995' GROUP BY x.fonte_id",
            ),
            (
                "registro por fonte",
                "SELECT fonte_id, count(*), min(data), max(data) FROM registro GROUP BY fonte_id",
            ),
            (
                "ciclos (13 x 2 subconsultas)",
                "SELECT c.id, (SELECT count(*) FROM achado WHERE ciclo_id = c.id AND estado = 'documento'), (SELECT count(*) FROM registro WHERE ciclo_id = c.id) FROM ciclo c WHERE c.id <= '2026-10' ORDER BY c.id DESC LIMIT 13",
            ),
            (
                "áreas deixadas de fora",
                "SELECT motivo, count(*) FROM area_excluida GROUP BY motivo",
            ),
        ];
        for (nome, sql) in sqls {
            medir(&format!("  visão geral / {nome}"), || {
                let mut i = conn.prepare(sql).expect("preparar");
                i.query_map([], |_| Ok(())).expect("consulta").count()
            });
        }
    }
    medir("visão geral", || leitor.visao_geral().expect("painel"));
    medir("visão geral (cache)", || {
        leitor.visao_geral().expect("painel")
    });
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
