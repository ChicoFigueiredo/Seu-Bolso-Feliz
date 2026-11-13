//! Os índices da migração 2 que a leitura usa: a página das áreas grandes sai da ponta do
//! índice, sem ordenar a tabela inteira.

mod comum;

use rusqlite::Connection;

fn plano(conn: &Connection, sql: &str) -> String {
    let mut instrucao = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("plano");
    instrucao
        .query_map([], |l| l.get::<_, String>(3))
        .expect("linhas")
        .collect::<Result<Vec<_>, _>>()
        .expect("plano")
        .join(" | ")
}

#[test]
fn a_migracao_2_cria_os_indices_de_ordem_das_areas_grandes() {
    let banco = comum::banco_de_teste();
    let conn = Connection::open(&banco.caminho).expect("conexão");
    for indice in ["origem_local_mtime", "origem_gmail_date", "registro_data"] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?",
                [indice],
                |l| l.get(0),
            )
            .expect("consulta");
        assert_eq!(n, 1, "falta o índice {indice}");
    }
}

#[test]
fn arquivos_e_emails_ordenam_pelo_indice() {
    let banco = comum::banco_de_teste();
    let conn = Connection::open(&banco.caminho).expect("conexão");
    // Com estatísticas de uma tabela de verdade, o plano escolhe o índice de ordem.
    comum::popular_escala(&conn, 3_000, 6_000, 600, 40);
    conn.execute_batch("ANALYZE").expect("analyze");
    let arquivos = plano(
        &conn,
        "SELECT o.id FROM origem o LEFT JOIN achado a ON a.content_hash = o.content_hash
          WHERE o.origin_type IN ('local_file', 'onedrive', 'gdrive')
          ORDER BY o.local_mtime DESC, o.id DESC LIMIT 100",
    );
    assert!(arquivos.contains("origem_local_mtime"), "{arquivos}");
    assert!(
        !arquivos.contains("USE TEMP B-TREE FOR ORDER BY"),
        "{arquivos}"
    );
    let emails = plano(
        &conn,
        "SELECT o.id FROM origem o
          WHERE o.origin_type = 'gmail' AND o.mensagem_ref = 'eml' AND o.content_hash IS NOT NULL
          ORDER BY o.gmail_date DESC, o.id DESC LIMIT 100 OFFSET 50",
    );
    assert!(
        emails.contains("COVERING INDEX origem_gmail_date"),
        "{emails}"
    );
    assert!(!emails.contains("USE TEMP B-TREE FOR ORDER BY"), "{emails}");
}
