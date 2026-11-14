//! O `inventario.sqlite` compartilhado entre o worker e a janela: WAL, `busy_timeout` e
//! transações curtas. Duas threads escrevem sem parar enquanto uma terceira lê; nenhuma
//! recebe `SQLITE_BUSY` e nada se perde. Mais um segundo **processo** (o próprio binário de
//! teste relançado) escrevendo no mesmo banco.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use sbf_coleta::pastas::{Opcoes, Varredor};

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");
const SQL_TRIAGEM: &str = include_str!("../../sbf-inventario/sql/0003-triagem.sql");

fn preparar(banco: &Path) {
    let c = sbf_worker::banco::abrir(banco).unwrap();
    c.execute_batch(SQL_BASE).unwrap();
    c.execute_batch(SQL_TRIAGEM).unwrap();
    c.execute(
        "INSERT INTO fonte (id, tipo, descricao) VALUES ('conexao:teste', 'conexao', 'x')",
        [],
    )
    .unwrap();
}

/// A escrita da triagem: muitas transações pequenas.
fn escrever_sugestoes(banco: &Path, de: i64, n: i64) {
    let c = sbf_worker::banco::abrir(banco).unwrap();
    for i in de..de + n {
        c.execute(
            "INSERT INTO sugestao_ia (alvo_tipo, alvo_id, campo, valor_json, confianca, justificativa, modelo, prompt_versao)
             VALUES ('registro', ?1, 'categoria', '\"X\"', 0.9, 'teste', 'falso', 'triagem-v2')",
            [i.to_string()],
        )
        .unwrap();
    }
}

#[test]
fn varredura_triagem_e_leitor_ao_mesmo_tempo_sem_busy_e_sem_perder_nada() {
    let d = tempfile::tempdir().unwrap();
    let cofre = d.path().join("cofre");
    let disco = d.path().join("disco");
    std::fs::create_dir_all(&cofre).unwrap();
    std::fs::create_dir_all(&disco).unwrap();
    let banco = cofre.join("inventario.sqlite");
    preparar(&banco);
    for i in 0..300 {
        std::fs::create_dir_all(disco.join(format!("p{}", i % 10))).unwrap();
        std::fs::write(
            disco.join(format!("p{}/boleto-{i}.pdf", i % 10)),
            format!("documento ficticio numero {i}"),
        )
        .unwrap();
    }
    let fim = Arc::new(AtomicBool::new(false));

    // O leitor (a janela): consultas repetidas, conexão só de leitura.
    let leitor = {
        let (banco, fim) = (banco.clone(), fim.clone());
        std::thread::spawn(move || {
            let c = sbf_inventario::banco::abrir_leitura(&banco).unwrap();
            let mut lidas = 0_u64;
            while !fim.load(Ordering::Relaxed) {
                let _: i64 = c
                    .query_row("SELECT COUNT(*) FROM origem", [], |l| l.get(0))
                    .unwrap();
                let _: i64 = c
                    .query_row("SELECT COUNT(*) FROM sugestao_ia", [], |l| l.get(0))
                    .unwrap();
                lidas += 1;
            }
            lidas
        })
    };
    // A triagem (outra conexão): 400 transações pequenas.
    let triagem = {
        let banco = banco.clone();
        std::thread::spawn(move || escrever_sugestoes(&banco, 0, 400))
    };
    // O segundo processo: o próprio binário de teste, relançado só para escrever.
    let filho = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "filho_escritor", "--nocapture", "--ignored"])
        .env("SBF_TESTE_BANCO", &banco)
        .env("SBF_TESTE_DE", "1000")
        .spawn()
        .unwrap();
    // O worker: a varredura das pastas (transações curtas por lote de arquivos).
    let c = sbf_worker::banco::abrir(&banco).unwrap();
    let mut v = Varredor::novo(
        &c,
        "disco:teste",
        &disco,
        Opcoes {
            cofre: cofre.clone(),
            entrada: true,
            ..Opcoes::default()
        },
    )
    .unwrap();
    assert!(v.varrer(&AtomicBool::new(false)).unwrap());
    let r = v.finalizar("concluida").unwrap();

    triagem.join().unwrap();
    let saida = filho.wait_with_output();
    let status = saida.unwrap().status;
    assert!(status.success(), "o processo filho recebeu erro de banco");
    fim.store(true, Ordering::Relaxed);
    let lidas = leitor.join().unwrap();
    assert!(lidas > 0);

    assert_eq!(r.novos, 300, "{r:?}");
    assert_eq!(r.erros, 0);
    let ver = Connection::open(&banco).unwrap();
    let n = |sql: &str| -> i64 { ver.query_row(sql, [], |l| l.get(0)).unwrap() };
    assert_eq!(n("SELECT COUNT(*) FROM origem"), 300);
    assert_eq!(n("SELECT COUNT(*) FROM achado"), 300);
    assert_eq!(n("SELECT COUNT(*) FROM sugestao_ia"), 400 + 400, "thread + processo");
    assert_eq!(
        ver.query_row("PRAGMA journal_mode", [], |l| l.get::<_, String>(0)).unwrap(),
        "wal"
    );
}

/// Só roda relançado pelo teste acima (`--ignored` + variáveis de ambiente).
#[test]
#[ignore = "é o processo filho do teste de concorrência"]
fn filho_escritor() {
    let Some(banco) = std::env::var_os("SBF_TESTE_BANCO") else {
        return;
    };
    let de: i64 = std::env::var("SBF_TESTE_DE").unwrap().parse().unwrap();
    escrever_sugestoes(Path::new(&banco), de, 400);
    std::thread::sleep(Duration::from_millis(10));
}
