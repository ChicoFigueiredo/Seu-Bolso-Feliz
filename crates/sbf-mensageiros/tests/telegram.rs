//! Importação do export JSON do Telegram Desktop, com um export fictício montado no teste.

#![allow(
    clippy::field_reassign_with_default,
    clippy::unwrap_used,
    clippy::expect_used
)]

use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use sbf_mensageiros::coletor::{Pastas, coletar};
use sbf_mensageiros::config::{Checkpoints, Config};
use sbf_mensageiros::modelo::Checkpoint;
use sbf_mensageiros::telegram::ExportDoTelegram;
use sbf_mensageiros::{Erro, FonteDeMensagens};

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

const RESULT: &str = r#"{
  "about": "Export fictício",
  "personal_information": {"first_name": "Fulana"},
  "chats": {"about": "x", "list": [
    {"name": "Banco Ficticio", "type": "personal_chat", "id": 111, "messages": [
      {"id": 1, "type": "service", "date": "2024-03-01T09:00:00", "date_unixtime": "1709283600", "actor": "x"},
      {"id": 2, "type": "message", "date": "2024-03-01T10:20:30", "date_unixtime": "1709288430",
       "from": "Banco", "text": "Segue a fatura do cartão", "file": "files/fatura.pdf",
       "file_name": "fatura.pdf", "mime_type": "application/pdf"},
      {"id": 3, "type": "message", "date": "2024-03-02T08:00:00", "date_unixtime": "1709366400",
       "from": "Banco", "text": ["comprovante ", {"type": "bold", "text": "pix"}]},
      {"id": 4, "type": "message", "date": "2024-03-02T09:00:00", "date_unixtime": "1709370000",
       "from": "Banco", "text": "", "photo": "photos/foto.jpg", "photo_file_size": 10},
      {"id": 5, "type": "message", "date": "2024-03-02T09:30:00", "date_unixtime": "1709371800",
       "from": "Banco", "text": "", "file": "stickers/s.webp", "media_type": "sticker"},
      {"id": 6, "type": "message", "date": "2024-03-02T09:40:00", "date_unixtime": "1709372400",
       "from": "Banco", "text": "", "file": "(File not included. Change data exporting settings to download.)", "file_name": "boleto.pdf"},
      {"id": 7, "type": "message", "date": "2024-03-02T09:50:00", "date_unixtime": "1709373000",
       "from": "Banco", "text": "", "file": "../fora.pdf", "file_name": "fora.pdf"}
    ]},
    {"name": "Grupo da Pelada", "type": "private_supergroup", "id": 222, "messages": [
      {"id": 1, "type": "message", "date": "2024-03-03T10:00:00", "date_unixtime": "1709460000",
       "from": "Zé", "text": "fatura do campo", "file": "files/fatura.pdf", "file_name": "fatura.pdf"}
    ]}
  ]}
}"#;

fn export() -> (tempfile::TempDir, ExportDoTelegram) {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("files")).unwrap();
    std::fs::create_dir_all(d.path().join("photos")).unwrap();
    std::fs::write(
        d.path().join("files/fatura.pdf"),
        b"%PDF-1.4 fatura ficticia",
    )
    .unwrap();
    std::fs::write(d.path().join("photos/foto.jpg"), b"jpg ficticio").unwrap();
    std::fs::write(d.path().join("result.json"), RESULT).unwrap();
    // arquivo fora do export, para o teste do `..`
    std::fs::write(d.path().parent().unwrap().join("fora.pdf"), b"segredo").unwrap();
    let e = ExportDoTelegram::abrir(d.path()).unwrap();
    (d, e)
}

#[test]
fn lista_conversas_e_mensagens_na_ordem_sem_as_de_servico() {
    let (_d, mut e) = export();
    let conversas = e.listar_conversas().unwrap();
    assert_eq!(conversas.len(), 2);
    assert_eq!(conversas[0].id, "111");
    assert!(!conversas[0].grupo && conversas[1].grupo);
    let m = e
        .mensagens_novas("111", &Checkpoint::default(), 100)
        .unwrap();
    let ids: Vec<&str> = m.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["2", "3", "4", "5", "6", "7"]);
    assert_eq!(m[0].quando, 1_709_288_430);
    assert_eq!(m[1].texto.as_deref(), Some("comprovante pix"));
    assert!(m[3].anexo.is_none(), "figurinha não é achado");
    assert!(m[4].anexo.is_none(), "mídia não incluída no export");
    // depois do checkpoint da mensagem 3, só vêm as seguintes
    let depois = e.mensagens_novas("111", &Checkpoint::de(&m[1]), 2).unwrap();
    assert_eq!(
        depois.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["4", "5"]
    );
}

#[test]
fn caminho_de_midia_fora_do_export_e_recusado() {
    let (d, mut e) = export();
    let m = e
        .mensagens_novas("111", &Checkpoint::default(), 100)
        .unwrap();
    let fora = m.iter().find(|m| m.id == "7").unwrap();
    let r = e.baixar_anexo(fora, &d.path().join("x.pdf"));
    assert!(matches!(r, Err(Erro::Fonte(_))));
    assert!(!d.path().join("x.pdf").exists());
}

#[test]
fn export_de_uma_so_conversa_tambem_abre() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("result.json"),
        r#"{"name":"Fulano","type":"personal_chat","id":5,"messages":[
            {"id":1,"type":"message","date":"2024-03-01T10:00:00","date_unixtime":"1709287200","text":"boleto"}]}"#,
    )
    .unwrap();
    let mut e = ExportDoTelegram::abrir(d.path()).unwrap();
    assert_eq!(e.listar_conversas().unwrap()[0].id, "5");
    assert_eq!(
        e.mensagens_novas("5", &Checkpoint::default(), 9)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn pasta_sem_export_ou_json_estranho_dao_erro_claro() {
    let d = tempfile::tempdir().unwrap();
    assert!(matches!(
        ExportDoTelegram::abrir(d.path()),
        Err(Erro::NaoPronta)
    ));
    std::fs::write(d.path().join("result.json"), r#"{"outra":"coisa"}"#).unwrap();
    assert!(matches!(
        ExportDoTelegram::abrir(d.path()),
        Err(Erro::Formato(_))
    ));
    std::fs::write(d.path().join("result.json"), "nao e json").unwrap();
    assert!(matches!(
        ExportDoTelegram::abrir(d.path()),
        Err(Erro::Formato(_))
    ));
}

#[test]
fn importar_so_a_conversa_escolhida_para_o_inventario() {
    let (d, mut e) = export();
    let cofre = d.path().join("cofre");
    std::fs::create_dir_all(&cofre).unwrap();
    let conn = Connection::open(cofre.join("inventario.sqlite")).unwrap();
    conn.execute_batch(SQL_BASE).unwrap();
    let mut config = Config::default();
    config.ligado = true;
    config.telegram.ligada = true;
    config.telegram.incluir("111", true);
    let mut pontos = Checkpoints::default();
    let r = coletar(
        &mut e,
        &conn,
        &Pastas {
            cofre: cofre.clone(),
        },
        &config,
        &mut pontos,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(r.conversas, 1);
    let chaves: Vec<String> = conn
        .prepare("SELECT origin_key FROM origem ORDER BY origin_key")
        .unwrap()
        .query_map([], |l| l.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // 2: fatura.pdf + texto "fatura do cartão" (#texto); 3: texto; 4: foto. A 7 falha (fora do export).
    assert_eq!(
        chaves,
        [
            "telegram:111/2",
            "telegram:111/2#texto",
            "telegram:111/3",
            "telegram:111/4"
        ]
    );
    // a falha da 7 não perdeu as anteriores, e o checkpoint parou na 6
    assert_eq!(
        pontos
            .de(sbf_mensageiros::modelo::Plataforma::Telegram, "111")
            .id
            .as_deref(),
        Some("6")
    );
    assert_eq!(r.contadores.erros, 1);
    // reimportar o mesmo export não duplica
    let r2 = coletar(
        &mut e,
        &conn,
        &Pastas { cofre },
        &config,
        &mut Checkpoints::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(r2.contadores.novos, 0);
    let n: i64 = conn
        .query_row("SELECT count(*) FROM origem", [], |l| l.get(0))
        .unwrap();
    assert_eq!(n, 4);
}
