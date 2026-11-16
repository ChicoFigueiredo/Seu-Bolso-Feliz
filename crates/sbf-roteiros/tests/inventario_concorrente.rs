//! O inventário recebido de dois lados ao mesmo tempo (a janela e o worker): um só cria o
//! achado, ninguém recebe erro de chave repetida.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use comum::inventario;
use rusqlite::Connection;
use sbf_roteiros::executor::{Destino, Recebido};
use sbf_roteiros::inventario::NoInventario;
use std::sync::{Arc, Barrier};

#[test]
fn receber_o_mesmo_conteudo_ao_mesmo_tempo_cria_um_achado_so() {
    for rodada in 0..6 {
        let d = tempfile::tempdir().unwrap();
        let (cofre, _conn) = inventario(d.path());
        let banco = cofre.join("inventario.sqlite");
        let largada = Arc::new(Barrier::new(6));
        let fios: Vec<_> = (0..6_i64)
            .map(|i| {
                let (cofre, banco, l) = (cofre.clone(), banco.clone(), Arc::clone(&largada));
                let arquivo = d.path().join(format!("a{i}.pdf"));
                std::fs::write(&arquivo, format!("%PDF-1.4 conteudo igual {rodada}")).unwrap();
                std::thread::spawn(move || {
                    let conn = Connection::open(&banco).unwrap();
                    conn.busy_timeout(std::time::Duration::from_secs(30))
                        .unwrap();
                    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
                    let mut dest = NoInventario::novo(&conn, &cofre, "r", 100 + i);
                    l.wait();
                    let r = dest.receber(&arquivo, "extrato.pdf");
                    dest.fechar("concluida").unwrap();
                    r
                })
            })
            .collect();
        let resultados: Vec<_> = fios.into_iter().map(|f| f.join().unwrap()).collect();
        for r in &resultados {
            assert!(r.is_ok(), "rodada {rodada}: {r:?}");
        }
        let novos = resultados
            .iter()
            .filter(|r| matches!(r, Ok(Recebido::Novo)))
            .count();
        assert_eq!(novos, 1, "rodada {rodada}: só um cria o achado");
        let conn = Connection::open(&banco).unwrap();
        let achados: i64 = conn
            .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
            .unwrap();
        assert_eq!(achados, 1);
        let sem_copia: i64 = conn
            .query_row(
                "SELECT count(*) FROM achado WHERE caminho_acervo IS NULL",
                [],
                |l| l.get(0),
            )
            .unwrap();
        assert_eq!(sem_copia, 0, "o arquivo foi para o cofre");
    }
}
