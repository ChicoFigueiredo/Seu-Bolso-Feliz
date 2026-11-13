//! `Consulta::tabela_paginada`: as páginas das áreas grandes, juntas, são a tabela inteira, na
//! mesma ordem, e o total vem sem montar as linhas.

mod comum;

use comum::{BancoDeTeste, hash};
use rusqlite::Connection;
use sbf_inventario::consulta::{Area, Consulta, Filtro, Linha, PedidoDeTabela, Quem};
use sbf_inventario::leitor::LeitorSqlite;

fn abrir() -> (BancoDeTeste, LeitorSqlite) {
    let banco = comum::banco_de_teste();
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_hoje("2026-11-13");
    (banco, leitor)
}

fn pedido(area: Area, filtro: Filtro) -> PedidoDeTabela {
    PedidoDeTabela::Area { area, filtro }
}

/// Junta as páginas de `tamanho` linhas e confere contra a tabela inteira.
fn confere_paginas(leitor: &LeitorSqlite, p: &PedidoDeTabela, tamanho: usize) {
    let inteira = leitor.tabela(p).expect("tabela inteira");
    let mut linhas: Vec<Linha> = Vec::new();
    let mut inicio = 0;
    loop {
        let pagina = leitor.tabela_paginada(p, inicio, tamanho).expect("página");
        assert_eq!(pagina.total, inteira.linhas.len(), "{p:?}");
        assert_eq!(pagina.inicio, inicio);
        assert_eq!(pagina.tabela.colunas, inteira.colunas);
        assert!(pagina.tabela.linhas.len() <= tamanho);
        if pagina.tabela.linhas.is_empty() {
            break;
        }
        inicio += pagina.tabela.linhas.len();
        linhas.extend(pagina.tabela.linhas);
    }
    assert_eq!(linhas, inteira.linhas, "{p:?} em páginas de {tamanho}");
}

#[test]
fn paginas_das_areas_grandes_juntas_sao_a_tabela_inteira() {
    let (_b, leitor) = abrir();
    let filtros = [
        Filtro::default(),
        Filtro {
            texto: Some("energia".into()),
            ..Filtro::default()
        },
        Filtro {
            fontes: vec!["gmail".into()],
            ..Filtro::default()
        },
        Filtro {
            quem_decidiu: Some(Quem::Usuario),
            ..Filtro::default()
        },
        Filtro {
            ciclos: Some(("2026-09".into(), "2026-10".into())),
            ..Filtro::default()
        },
    ];
    for area in [
        Area::ATriar,
        Area::EmDuvida,
        Area::Documentos,
        Area::Registros,
        Area::Emails,
        Area::ArquivosEPastas,
    ] {
        for f in &filtros {
            for tamanho in [1, 3, 7, 1000] {
                confere_paginas(&leitor, &pedido(area, f.clone()), tamanho);
            }
        }
    }
}

#[test]
fn areas_pequenas_e_agregacoes_tambem_paginam() {
    let (_b, leitor) = abrir();
    for area in [
        Area::Ciclos,
        Area::Fornecedores,
        Area::Faturas,
        Area::ContasLacunas,
        Area::EmailsPorRemetente,
        Area::PastasPessoais,
        Area::Coletas,
        Area::Pendencias,
        Area::Lotes,
    ] {
        confere_paginas(&leitor, &pedido(area, Filtro::default()), 2);
    }
    // Ligados e totais também (o corte padrão).
    let ligados = PedidoDeTabela::Ligados {
        de: "fornecedor:Energia Exemplo".parse().expect("ref"),
        tipo: sbf_inventario::referencia::Tipo::Registro,
    };
    confere_paginas(&leitor, &ligados, 2);
}

#[test]
fn pagina_alem_do_fim_vem_vazia_com_o_total() {
    let (_b, leitor) = abrir();
    let p = pedido(Area::Registros, Filtro::default());
    let total = leitor.tabela(&p).expect("tabela").linhas.len();
    let pagina = leitor.tabela_paginada(&p, total + 50, 10).expect("página");
    assert!(pagina.tabela.linhas.is_empty());
    assert_eq!(pagina.total, total);
    assert!(!pagina.tabela.colunas.is_empty());
}

#[test]
fn o_cache_das_agregacoes_acompanha_o_banco() {
    let (b, leitor) = abrir();
    let p = pedido(Area::Coletas, Filtro::default());
    let antes = leitor.tabela_paginada(&p, 0, 100).expect("página").total;
    // Uma nova coleta entra por outra conexão: o leitor vê na próxima página.
    let escritor = Connection::open(&b.caminho).expect("escritor");
    escritor
        .execute(
            "INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel) VALUES (50, 'gmail', 'E2', 'x', 'teste@1')",
            [],
        )
        .expect("gravar");
    let depois = leitor.tabela_paginada(&p, 0, 100).expect("página").total;
    assert_eq!(depois, antes + 1);
    // E a Visão geral também.
    let n_antes = leitor.visao_geral().expect("painel").secoes[2]
        .tabela
        .linhas
        .len();
    escritor
        .execute(
            "INSERT INTO fonte (id, tipo, descricao) VALUES ('nova', 'app', 'Nova')",
            [],
        )
        .expect("gravar");
    assert_eq!(
        leitor.visao_geral().expect("painel").secoes[2]
            .tabela
            .linhas
            .len(),
        n_antes + 1
    );
    let _ = hash(1);
}
