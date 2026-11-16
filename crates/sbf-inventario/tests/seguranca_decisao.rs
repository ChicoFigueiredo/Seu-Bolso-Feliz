//! Achado A4 da revisão de segurança da fatia 1: o log de decisões continua só de acréscimo
//! (INV-07) mesmo para uma conexão de fora do app, sem os PRAGMAs dele. Banco em memória,
//! dados fictícios.
#![allow(clippy::unwrap_used)]

use rusqlite::Connection;
use sbf_inventario::migracoes::escada;

/// Uma conexão crua, como a de um script ou do `sqlite3`: `recursive_triggers` desligado.
fn banco_migrado_por_conexao_crua() -> Connection {
    let mut conexao = Connection::open_in_memory().unwrap();
    bancada_armazem::migrar(&mut conexao, &escada()).unwrap();
    conexao
        .pragma_update(None, "recursive_triggers", 0)
        .unwrap();
    conexao
        .execute(
            "INSERT INTO decisao (id, alvo_tipo, alvo_id, campo, depois, autor)
             VALUES (1, 'achado', 'h', 'estado', '\"documento\"', 'usuario')",
            [],
        )
        .unwrap();
    conexao
}

fn depois_da_decisao_1(conexao: &Connection) -> String {
    conexao
        .query_row("SELECT depois FROM decisao WHERE id = 1", [], |l| l.get(0))
        .unwrap()
}

#[test]
fn insert_or_replace_de_conexao_crua_e_recusado() {
    let conexao = banco_migrado_por_conexao_crua();
    let recursivo: i64 = conexao
        .query_row("PRAGMA recursive_triggers", [], |l| l.get(0))
        .unwrap();
    assert_eq!(recursivo, 0, "o teste precisa do PRAGMA desligado");

    let resultado = conexao.execute(
        "INSERT OR REPLACE INTO decisao (id, alvo_tipo, alvo_id, campo, depois, autor)
         VALUES (1, 'achado', 'h', 'estado', '\"descartado\"', 'usuario')",
        [],
    );
    assert!(resultado.is_err(), "o REPLACE reescreveu uma decisão");
    assert_eq!(depois_da_decisao_1(&conexao), "\"documento\"");
}

#[test]
fn replace_e_upsert_tambem_sao_recusados() {
    let conexao = banco_migrado_por_conexao_crua();
    for sql in [
        "REPLACE INTO decisao (id, alvo_tipo, alvo_id, campo, depois, autor)
         VALUES (1, 'achado', 'h', 'estado', '\"x\"', 'usuario')",
        "INSERT INTO decisao (id, alvo_tipo, alvo_id, campo, depois, autor)
         VALUES (1, 'achado', 'h', 'estado', '\"x\"', 'usuario')
         ON CONFLICT (id) DO UPDATE SET depois = excluded.depois",
    ] {
        assert!(conexao.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(depois_da_decisao_1(&conexao), "\"documento\"");
}

#[test]
fn insert_normal_continua_funcionando() {
    let conexao = banco_migrado_por_conexao_crua();
    conexao
        .execute(
            "INSERT INTO decisao (alvo_tipo, alvo_id, campo, depois, autor)
             VALUES ('achado', 'h2', 'estado', '\"documento\"', 'usuario')",
            [],
        )
        .unwrap();
    conexao
        .execute(
            "INSERT INTO decisao (id, alvo_tipo, alvo_id, campo, depois, autor)
             VALUES (50, 'achado', 'h3', 'estado', '\"documento\"', 'usuario')",
            [],
        )
        .unwrap();
    let total: i64 = conexao
        .query_row("SELECT COUNT(*) FROM decisao", [], |l| l.get(0))
        .unwrap();
    assert_eq!(total, 3);
}
