//! `gmail_coletar` (D15): aplica direto, com backup antes da primeira escrita da sessão e só
//! quando a coleta do Gmail foi ligada. O cliente do Gmail não existe aqui: o `sbf` injeta a
//! coleta, e o teste injeta uma de mentira.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use sbf_mcp::servidor::Servidor;
use sbf_mcp::{Nucleo, ParamsDeColetaDoGmail};
use serde_json::json;

fn cofre() -> (tempfile::TempDir, Nucleo) {
    let dir = tempfile::tempdir().unwrap();
    let banco = dir.path().join("inventario.sqlite");
    Connection::open(&banco)
        .unwrap()
        .execute_batch("CREATE TABLE achado (content_hash TEXT); INSERT INTO achado VALUES ('x');")
        .unwrap();
    let nucleo = Nucleo::novo(&banco, None).unwrap();
    (dir, nucleo)
}

#[test]
fn sem_a_coleta_ligada_a_ferramenta_nao_aparece_e_recusa() {
    let (_dir, nucleo) = cofre();
    assert!(!nucleo.tem_gmail());
    assert!(
        !Servidor::novo(nucleo.clone())
            .nomes_das_ferramentas()
            .iter()
            .any(|n| n == "gmail_coletar")
    );
    assert!(
        nucleo
            .gmail_coletar(&ParamsDeColetaDoGmail::default())
            .is_err()
    );
}

#[test]
fn com_a_coleta_ligada_aplica_direto_repassa_os_parametros_e_faz_backup() {
    let (dir, nucleo) = cofre();
    let recebidos = Arc::new(Mutex::new(Vec::new()));
    let r = recebidos.clone();
    let nucleo = nucleo.com_gmail(Arc::new(move |p| {
        r.lock()
            .unwrap()
            .push((p.consulta.clone(), p.desde.clone(), p.limite));
        Ok(json!({"mensagens": 3, "documentos_novos": 2}))
    }));
    assert!(nucleo.tem_gmail());
    assert!(
        Servidor::novo(nucleo.clone())
            .nomes_das_ferramentas()
            .iter()
            .any(|n| n == "gmail_coletar")
    );

    let resposta = nucleo
        .gmail_coletar(&ParamsDeColetaDoGmail {
            consulta: Some("from:banco".to_owned()),
            desde: Some("2026-10-01".to_owned()),
            limite: Some(10),
        })
        .unwrap();

    assert_eq!(resposta["documentos_novos"], 2);
    assert_eq!(
        recebidos.lock().unwrap().as_slice(),
        [(
            Some("from:banco".to_owned()),
            Some("2026-10-01".to_owned()),
            Some(10)
        )]
    );
    // Antes da primeira escrita da sessão, o inventário foi copiado.
    let copias: Vec<_> = std::fs::read_dir(dir.path().join("backups/mcp"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(copias.len(), 1, "{copias:?}");
    assert!(copias[0].starts_with("inventario-"));
    // A segunda coleta da sessão não copia de novo.
    nucleo
        .gmail_coletar(&ParamsDeColetaDoGmail::default())
        .unwrap();
    assert_eq!(
        std::fs::read_dir(dir.path().join("backups/mcp"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn a_falha_da_coleta_volta_como_erro_do_mcp() {
    let (_dir, nucleo) = cofre();
    let nucleo = nucleo.com_gmail(Arc::new(|_| {
        Err("faltam as credenciais do Gmail".to_owned())
    }));
    assert!(
        nucleo
            .gmail_coletar(&ParamsDeColetaDoGmail::default())
            .unwrap_err()
            .contains("credenciais")
    );
}
