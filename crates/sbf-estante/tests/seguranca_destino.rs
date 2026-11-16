//! Achado A1 da revisão de segurança da fatia 1: `data_chave`, `ciclo_id` e `extensao`
//! hostis no banco não levam o `rclone copyto` para fora da base da estante. É a sonda S3
//! da revisão, como teste de regressão (banco, cofre e remoto fictícios, gerados aqui).

#![cfg(unix)]

mod comum;

use comum::{Doc, inserir, nomes, novo_banco};
use rusqlite::params;
use sbf_estante::{Estante, Rclone};
use std::path::Path;

const HASH: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

/// Um documento normal; depois o teste estraga os campos com `UPDATE`, como faria um script
/// com bug ou um banco restaurado de outra origem.
fn documento_estragado(c: &rusqlite::Connection, cofre: &Path, data: &str, ciclo: &str, ext: &str) {
    inserir(
        c,
        cofre,
        &Doc {
            hash: HASH,
            estado: "documento",
            tipo: Some("fatura"),
            data: Some("2026-10-25"),
            ciclo: Some("2026-10"),
            fornecedor: Some("Banco Alfa"),
            valor: Some(100),
            ext: "pdf",
            conteudo: Some(b"%PDF-1.4 ficticio"),
            nome_original: "fatura.pdf",
        },
    );
    c.execute(
        "INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?1, '2026-01-01', '2026-01-31')",
        [ciclo],
    )
    .expect("ciclo hostil");
    c.execute(
        "UPDATE achado SET data_chave = ?1, ciclo_id = ?2, extensao = ?3 WHERE content_hash = ?4",
        params![data, ciclo, ext, HASH],
    )
    .expect("estragar");
}

/// Todos os arquivos sob `raiz`, em caminho absoluto, que não estão sob `base`.
fn fora_da_base(raiz: &Path, base: &Path) -> Vec<String> {
    nomes(raiz)
        .into_iter()
        .filter(|n| !raiz.join(n).starts_with(base))
        .collect()
}

#[test]
fn data_e_ciclo_com_ponto_ponto_nao_saem_da_base() {
    let dir = tempfile::tempdir().expect("pasta");
    let cofre = dir.path().join("cofre");
    let drive = dir.path().join("drive");
    let base = drive.join("SBF/Estante");
    std::fs::create_dir_all(&base).expect("base");
    let c = novo_banco(&dir.path().join("inv.sqlite"));
    documento_estragado(&c, &cofre, "../../FORA", "../../..", "pdf");

    let estante = Estante::nova(Rclone::novo(), base.to_string_lossy()).expect("estante");
    let plano = estante.planejar(&c, &cofre).expect("planejar");
    assert_eq!(plano.enviar.len(), 1);
    let destino = &plano.enviar[0].destino;
    assert!(!destino.contains(".."), "{destino}");
    assert_eq!(
        destino,
        "a-classificar/faturas/banco-alfa_1,00_abcdef01.pdf"
    );

    let relatorio = estante.atualizar(&c, &cofre, &|_| {}).expect("atualizar");
    assert_eq!(relatorio.enviados, 1);
    assert!(fora_da_base(&drive, &base).is_empty());
    assert_eq!(
        nomes(&base),
        vec!["a-classificar/faturas/banco-alfa_1,00_abcdef01.pdf".to_owned()]
    );
}

#[test]
fn extensao_hostil_fica_sem_extensao() {
    for ext in [
        "../../x",
        "pdf/../../y",
        "PDF",
        "a.b",
        "abcdefghijk",
        "pd f",
    ] {
        let dir = tempfile::tempdir().expect("pasta");
        let cofre = dir.path().join("cofre");
        let base = dir.path().join("drive/SBF/Estante");
        let c = novo_banco(&dir.path().join("inv.sqlite"));
        documento_estragado(&c, &cofre, "2026-10-25", "2026-10", ext);
        let estante = Estante::nova(Rclone::novo(), base.to_string_lossy()).expect("estante");
        let plano = estante.planejar(&c, &cofre).expect("planejar");
        assert_eq!(
            plano.enviar[0].destino, "2026-10/faturas/2026-10-25_banco-alfa_1,00_abcdef01",
            "extensão {ext:?}"
        );
    }
}

#[test]
fn data_e_ciclo_fora_do_formato_sao_ignorados() {
    for (data, ciclo) in [
        ("2026-10-25/..", "2026-10/.."),
        ("25/10/2026", "out-2026"),
        ("2026-1-5", "2026-1"),
        ("", ""),
        ("/etc", "/tmp"),
    ] {
        let dir = tempfile::tempdir().expect("pasta");
        let cofre = dir.path().join("cofre");
        let base = dir.path().join("drive/SBF/Estante");
        let c = novo_banco(&dir.path().join("inv.sqlite"));
        documento_estragado(&c, &cofre, data, ciclo, "pdf");
        let estante = Estante::nova(Rclone::novo(), base.to_string_lossy()).expect("estante");
        let plano = estante.planejar(&c, &cofre).expect("planejar");
        assert_eq!(
            plano.enviar[0].destino, "a-classificar/faturas/banco-alfa_1,00_abcdef01.pdf",
            "data {data:?}, ciclo {ciclo:?}"
        );
    }
}
