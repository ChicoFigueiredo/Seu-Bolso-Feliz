//! A fixture compartilhada (`tests/comum`) grava o log de decisões direto por SQL. Este
//! teste garante que ela usa as mesmas formas JSON que o núcleo grava (`projecao.rs`):
//! `reprojetar` roda sobre ela sem erro e não muda a projeção. Sem isto, uma fixture com
//! forma errada esconde defeitos de leitura do log (achado em 2026-11-13, quando
//! `sbf reprojetar` falhou sobre o banco de exemplo).

mod comum;

use rusqlite::Connection;
use sbf_inventario::banco::Inventario;

/// As colunas da projeção que as decisões da fixture tocam.
fn retrato(caminho: &std::path::Path) -> Vec<String> {
    let conn = Connection::open(caminho).expect("abrir");
    let mut linhas = Vec::new();
    for sql in [
        "SELECT content_hash || '|' || estado || '|' || coalesce(decidido_por, '') FROM achado ORDER BY 1",
        "SELECT id || '|' || estado || '|' || coalesce(categoria_provavel, '') || '|' || coalesce(decidido_por, '') FROM registro ORDER BY id",
        "SELECT id || '|' || coalesce(aprovado_em, '') FROM lote ORDER BY id",
    ] {
        let mut consulta = conn.prepare(sql).expect("preparar");
        let valores = consulta
            .query_map([], |l| l.get::<_, String>(0))
            .expect("consultar")
            .collect::<Result<Vec<_>, _>>()
            .expect("ler");
        linhas.extend(valores);
    }
    linhas
}

#[test]
fn reprojetar_roda_sobre_a_fixture_e_nao_muda_a_projecao() {
    let banco = comum::banco_de_teste();
    let antes = retrato(&banco.caminho);
    let mut inventario = Inventario::abrir(&banco.caminho).expect("abrir o inventário");
    inventario
        .reprojetar()
        .expect("reprojetar sobre a fixture compartilhada");
    drop(inventario);
    assert_eq!(retrato(&banco.caminho), antes);
}
