//! As duas metades das áreas E-mails e Arquivos e pastas também saem por `Consulta::tabela`.

mod comum;

use comum::banco_de_teste;
use sbf_inventario::consulta::{Area, Consulta, Filtro, PedidoDeTabela};
use sbf_inventario::leitor::LeitorSqlite;

fn area(leitor: &LeitorSqlite, area: Area) -> sbf_inventario::consulta::Tabela {
    leitor
        .tabela(&PedidoDeTabela::Area {
            area,
            filtro: Filtro::default(),
        })
        .expect("tabela da área")
}

#[test]
fn inv03_emails_por_remetente_e_pastas_pessoais_saem_pela_consulta() {
    let banco = banco_de_teste();
    let leitor = LeitorSqlite::abrir(&banco.caminho).expect("abrir o leitor");
    let filtro = Filtro::default();
    assert_eq!(
        area(&leitor, Area::EmailsPorRemetente),
        leitor.emails_por_remetente(&filtro).expect("vista direta")
    );
    assert_eq!(
        area(&leitor, Area::PastasPessoais),
        leitor.pastas_pessoais(&filtro).expect("vista direta")
    );
}
