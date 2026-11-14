//! O ciclo do worker: interruptor, conversas incluídas e o export do Telegram, sem rede.

#![allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]

use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_mensageiros::Erro;
use sbf_mensageiros::ciclo::{ARQUIVO_DA_CONFIG, rodar};
use sbf_mensageiros::config::Config;

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

fn sem_chaveiro() -> Result<sbf_chaveiro::Chaveiro, Erro> {
    Err(Erro::Chaveiro("o teste não abre chaveiro".into()))
}

fn banco(d: &std::path::Path) -> (std::path::PathBuf, Connection, std::path::PathBuf) {
    let cofre = d.join("cofre");
    std::fs::create_dir_all(&cofre).unwrap();
    let conn = Connection::open(cofre.join("inventario.sqlite")).unwrap();
    conn.execute_batch(SQL_BASE).unwrap();
    let dados = d.join("dados");
    std::fs::create_dir_all(&dados).unwrap();
    (cofre, conn, dados)
}

#[test]
fn desligado_ou_sem_conversa_incluida_nao_toca_em_nada() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn, dados) = banco(d.path());
    let nao = AtomicBool::new(false);
    // sem arquivo de configuração
    let r = rodar(&conn, &dados, &cofre, &sem_chaveiro, &nao).unwrap();
    assert_eq!(r.falhas, 0);
    // ligado e plataformas ligadas, mas nenhuma conversa incluída, com a ponte e o export
    // apontando para o que não existe: nem tenta
    let mut c = Config::default();
    c.ligado = true;
    c.whatsapp.ligada = true;
    c.telegram.ligada = true;
    c.export_do_telegram = Some(d.path().join("nao-existe"));
    c.ponte_do_whatsapp = Some(d.path().join("nao-existe"));
    c.salvar(&dados.join(ARQUIVO_DA_CONFIG)).unwrap();
    let r = rodar(&conn, &dados, &cofre, &sem_chaveiro, &nao).unwrap();
    assert_eq!((r.mensagens, r.falhas), (0, 0));
}

#[test]
fn telegram_do_export_entra_e_whatsapp_sem_chaveiro_falha_sem_derrubar_o_outro() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn, dados) = banco(d.path());
    let export = d.path().join("export");
    std::fs::create_dir_all(export.join("files")).unwrap();
    std::fs::write(export.join("files/boleto.pdf"), b"%PDF-1.4 ficticio").unwrap();
    std::fs::write(
        export.join("result.json"),
        r#"{"name":"Condominio","type":"personal_chat","id":9,"messages":[
           {"id":1,"type":"message","date":"2024-03-01T10:00:00","date_unixtime":"1709287200",
            "text":"boleto de março","file":"files/boleto.pdf","file_name":"boleto.pdf"}]}"#,
    )
    .unwrap();
    // uma "ponte" qualquer que existe, para o WhatsApp tentar e falhar no chaveiro
    let ponte = d.path().join("ponte");
    std::fs::write(&ponte, "#!/bin/sh\n").unwrap();
    let mut c = Config::default();
    c.ligado = true;
    c.telegram.ligada = true;
    c.telegram.incluir("9", true);
    c.export_do_telegram = Some(export);
    c.whatsapp.ligada = true;
    c.whatsapp.incluir("x", true);
    c.ponte_do_whatsapp = Some(ponte);
    c.salvar(&dados.join(ARQUIVO_DA_CONFIG)).unwrap();
    let nao = AtomicBool::new(false);
    let r = rodar(&conn, &dados, &cofre, &sem_chaveiro, &nao).unwrap();
    // anexo + texto financeiro: duas origens; o WhatsApp falhou uma vez
    assert_eq!((r.mensagens, r.novos, r.falhas), (1, 2, 1));
    // segunda rodada: checkpoint em disco, nada novo
    let r2 = rodar(&conn, &dados, &cofre, &sem_chaveiro, &nao).unwrap();
    assert_eq!((r2.mensagens, r2.novos), (0, 0));
}
