//! Achados A8 e A9 da revisão de segurança da fatia 1:
//!
//! - A8: um instantâneo em claro que sobrou em `<cofre>/tmp/copia-*` (SIGKILL, queda de
//!   energia) é apagado na próxima execução;
//! - A9: a restauração não sobrescreve um arquivo do acervo que já esteja no destino, e uma
//!   chamada ao rclone travada não trava a ação para sempre.
//!
//! Cofre, banco e remoto fictícios, gerados aqui.

#![cfg(unix)]

mod comum;

use comum::{Doc, escrever_executavel, inserir, novo_banco};
use sbf_estante::copia::faxinar_sobras;
use sbf_estante::{CopiaDeSeguranca, Erro, Estante, Rclone, Segredo};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

const SENHA: &str = "senha-ficticia-do-teste-Zq7!";

fn sobra(cofre: &Path, nome: &str, idade: Duration) {
    let pasta = cofre.join("tmp").join(nome);
    fs::create_dir_all(&pasta).expect("pasta");
    fs::write(
        pasta.join("inventario.sqlite"),
        b"SQLite format 3\0 ficticio",
    )
    .expect("sobra");
    let quando = SystemTime::now() - idade;
    fs::File::open(&pasta)
        .expect("abrir pasta")
        .set_modified(quando)
        .expect("data");
}

#[test]
fn sobras_velhas_de_copia_sao_apagadas() {
    let cofre = tempfile::tempdir().expect("cofre");
    let dia = Duration::from_secs(86_400);
    sobra(cofre.path(), "copia-velha", 2 * dia);
    sobra(cofre.path(), "copia-recente", Duration::from_secs(60));
    sobra(cofre.path(), "ocr-alheio", 2 * dia);

    // Na abertura do app, com a idade padrão: só a velha sai (a recente pode ser de uma
    // cópia rodando agora em outra janela).
    assert_eq!(faxinar_sobras(cofre.path(), dia).expect("faxina"), 1);
    let tmp = cofre.path().join("tmp");
    assert!(!tmp.join("copia-velha").exists());
    assert!(tmp.join("copia-recente").exists());
    assert!(tmp.join("ocr-alheio").exists(), "só mexe no que é da cópia");

    // Idade zero: tudo o que é da cópia.
    assert_eq!(
        faxinar_sobras(cofre.path(), Duration::ZERO).expect("faxina"),
        1
    );
    assert!(!tmp.join("copia-recente").exists());
    // Sem `tmp/`: nada a fazer.
    let vazio = tempfile::tempdir().expect("vazio");
    assert_eq!(faxinar_sobras(vazio.path(), dia).expect("faxina"), 0);
}

#[test]
fn fazer_copia_apaga_a_sobra_velha() {
    let cofre = tempfile::tempdir().expect("cofre");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    sobra(cofre.path(), "copia-morta", Duration::from_secs(3 * 86_400));
    let remoto = tempfile::tempdir().expect("remoto");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    copia
        .fazer(&c, cofre.path(), &Segredo::novo(SENHA))
        .expect("fazer");
    let restos: Vec<_> = fs::read_dir(cofre.path().join("tmp"))
        .expect("tmp")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert!(restos.is_empty(), "{restos:?}");
}

#[test]
fn restauracao_nao_sobrescreve_acervo_existente() {
    let cofre = tempfile::tempdir().expect("cofre");
    let c = novo_banco(&cofre.path().join("inventario.sqlite"));
    inserir(
        &c,
        cofre.path(),
        &Doc {
            hash: "aaaaaaaa11111111",
            estado: "documento",
            tipo: Some("fatura"),
            data: Some("2026-10-25"),
            ciclo: Some("2026-10"),
            fornecedor: Some("Loja Ficticia"),
            valor: Some(1000),
            ext: "pdf",
            conteudo: Some(b"%PDF-1.4 da copia"),
            nome_original: "a.pdf",
        },
    );
    let remoto = tempfile::tempdir().expect("remoto");
    let copia =
        CopiaDeSeguranca::nova(Rclone::novo(), remoto.path().to_string_lossy()).expect("copia");
    let senha = Segredo::novo(SENHA);
    copia.fazer(&c, cofre.path(), &senha).expect("fazer");

    let volta = tempfile::tempdir().expect("volta");
    let alvo = volta.path().join("c");
    let ja_la = alvo.join("acervo/aa/aaaaaaaa11111111.pdf");
    fs::create_dir_all(ja_la.parent().expect("pai")).expect("pasta");
    fs::write(&ja_la, b"do usuario, nao mexer").expect("existente");
    copia.restaurar(&senha, &alvo).expect("restaurar");
    assert_eq!(fs::read(&ja_la).expect("ler"), b"do usuario, nao mexer");
}

#[test]
fn rclone_travado_tem_limite_de_tempo() {
    let pasta = tempfile::tempdir().expect("pasta");
    let script = pasta.path().join("rclone-travado");
    escrever_executavel(&script, "#!/bin/sh\nexec sleep 30\n");
    let rclone = Rclone::com_binario(&script).com_prazo(Duration::from_millis(500));
    let estante = Estante::nova(rclone, pasta.path().to_string_lossy()).expect("estante");
    let inicio = Instant::now();
    let resultado = estante.listar();
    assert!(inicio.elapsed() < Duration::from_secs(5), "travou");
    assert!(
        matches!(resultado, Err(Erro::TempoEsgotado { .. })),
        "{resultado:?}"
    );
}
