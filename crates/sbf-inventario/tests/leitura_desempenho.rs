//! INV-16: as consultas respondem em menos de 300 ms com dezenas de milhares de achados.
//!
//! `#[ignore]`: gera uma fixture sintética grande. Rode com
//! `cargo test -p sbf-inventario --test leitura_desempenho -- --ignored --nocapture`.

mod comum;

use std::time::{Duration, Instant};

use rusqlite::Connection;
use sbf_inventario::consulta::{Area, Consulta, Filtro, Fonte, PedidoDeTabela, Pergunta};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::referencia::{Ref, Tipo};

const LIMITE: Duration = Duration::from_millis(300);
/// A tabela inteira de 100 mil registros e os relatórios que varrem o banco todo têm 1 s:
/// a janela virtualiza a tabela, e o relatório roda em segundo plano.
const LIMITE_DA_VARREDURA_TOTAL: Duration = Duration::from_millis(1000);
const VARREDURAS_TOTAIS: [&str; 3] = ["área Registros", "filtro fonte em Registros", "Visão geral"];
const ACHADOS: u32 = 50_000;

fn medir<T>(nome: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    let d = t.elapsed();
    eprintln!("{nome:<48} {:>6} ms", d.as_millis());
    (r, d)
}

#[test]
#[ignore = "gera 50 mil achados; rode com --ignored"]
fn consultas_respondem_em_menos_de_300_ms_com_50_mil_achados() {
    let banco = comum::banco_de_teste();
    {
        let conn = Connection::open(&banco.caminho).expect("conexão");
        comum::popular_grande(&conn, ACHADOS);
    }
    let leitor_conn = Connection::open(&banco.caminho).expect("conexão");
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_hoje("2026-11-13");
    let mut lentas = Vec::new();
    let mut confere = |nome: &str, d: Duration| {
        let limite = if VARREDURAS_TOTAIS.contains(&nome) || nome.starts_with("relatório") {
            LIMITE_DA_VARREDURA_TOTAL
        } else {
            LIMITE
        };
        if d > limite {
            lentas.push(format!("{nome}: {} ms", d.as_millis()));
        }
    };

    for (nome, area) in [
        ("A triar", Area::ATriar),
        ("Em dúvida", Area::EmDuvida),
        ("Documentos", Area::Documentos),
        ("Registros", Area::Registros),
        ("Ciclos", Area::Ciclos),
        ("Fornecedores", Area::Fornecedores),
        ("Faturas", Area::Faturas),
        ("Contas (lacunas)", Area::ContasLacunas),
        ("E-mails", Area::Emails),
        ("Arquivos e pastas", Area::ArquivosEPastas),
        ("Coletas", Area::Coletas),
        ("Pendências", Area::Pendencias),
        ("Lotes", Area::Lotes),
    ] {
        let (t, d) = medir(&format!("área {nome} (sem filtro)"), || {
            leitor
                .tabela(&PedidoDeTabela::Area {
                    area,
                    filtro: Filtro::default(),
                })
                .expect("área")
        });
        eprintln!("    {} linhas", t.linhas.len());
        confere(&format!("área {nome}"), d);
    }
    // Filtrar: texto, período e quem decidiu nas áreas principais.
    for (nome, filtro) in [
        (
            "texto",
            Filtro {
                texto: Some("sintético 7".into()),
                ..Filtro::default()
            },
        ),
        (
            "período",
            Filtro {
                ciclos: Some(("2026-01".into(), "2026-03".into())),
                ..Filtro::default()
            },
        ),
        (
            "tipo",
            Filtro {
                tipos: vec!["fatura".into()],
                ..Filtro::default()
            },
        ),
        (
            "fonte",
            Filtro {
                fontes: vec!["disco:g".into()],
                ..Filtro::default()
            },
        ),
    ] {
        for (area_nome, area) in [
            ("Documentos", Area::Documentos),
            ("Registros", Area::Registros),
            ("Fornecedores", Area::Fornecedores),
        ] {
            let (_, d) = medir(&format!("filtro {nome} em {area_nome}"), || {
                leitor
                    .tabela(&PedidoDeTabela::Area {
                        area,
                        filtro: filtro.clone(),
                    })
                    .expect("área")
            });
            confere(&format!("filtro {nome} em {area_nome}"), d);
        }
    }
    {
        let ctx = sbf_inventario::vistas::Contexto::novo(&leitor_conn).expect("contexto");
        let (l, _) = medir("registros: só o SQL e as linhas tipadas", || {
            sbf_inventario::vistas::registros(&ctx, &sbf_inventario::vistas::Selecao::nova())
                .expect("registros")
        });
        medir("registros: da linha tipada para a Tabela", || {
            sbf_inventario::vistas::tabela_registros(&l)
        });
    }
    let (_, d) = medir("Visão geral", || leitor.visao_geral().expect("painel"));
    confere("Visão geral", d);
    let (_, d) = medir("contagens", || leitor.contagens().expect("contagens"));
    confere("contagens", d);
    for p in [
        Pergunta::QuantoSaiu,
        Pergunta::MaioresFornecedores,
        Pergunta::OQueSeRepete,
        Pergunta::OQueAumentou,
        Pergunta::OndeSeConcentra,
        Pergunta::NaoClassificado,
        Pergunta::PossiveisDuplicidades,
        Pergunta::OQueFaltaParaConfiar,
    ] {
        let (_, d) = medir(&format!("relatório {p:?}"), || {
            leitor
                .relatorio(p, Fonte::Inventario, &Filtro::default())
                .expect("relatório")
        });
        confere(&format!("relatório {p:?}"), d);
    }
    let forn = Ref::nova(Tipo::Fornecedor, "Fornecedor sintético 7");
    let hash = Ref::nova(Tipo::Achado, format!("{:064x}", 7));
    for (nome, de, tipo) in [
        ("fornecedor → registros", &forn, Tipo::Registro),
        ("fornecedor → documentos", &forn, Tipo::Documento),
        ("achado → origens", &hash, Tipo::Origem),
        (
            "conexão → origens",
            &Ref::nova(Tipo::Conexao, "disco:g"),
            Tipo::Origem,
        ),
        (
            "ciclo → documentos",
            &Ref::nova(Tipo::Ciclo, "2026-01"),
            Tipo::Documento,
        ),
    ] {
        let (_, d) = medir(&format!("ligados {nome}"), || {
            leitor
                .tabela(&PedidoDeTabela::Ligados {
                    de: de.clone(),
                    tipo,
                })
                .expect("ligados")
        });
        confere(&format!("ligados {nome}"), d);
    }
    let (_, d) = medir("item fornecedor", || leitor.item(&forn).expect("item"));
    confere("item fornecedor", d);
    let (_, d) = medir("item achado", || leitor.item(&hash).expect("item"));
    confere("item achado", d);
    let (_, d) = medir("rastro achado", || leitor.rastro(&hash).expect("rastro"));
    confere("rastro achado", d);
    let (_, d) = medir("por_ciclo fornecedor (13)", || {
        leitor
            .por_ciclo(&forn, Fonte::Inventario, 13)
            .expect("por ciclo")
    });
    confere("por_ciclo", d);
    assert!(lentas.is_empty(), "consultas acima de 300 ms: {lentas:#?}");
}
