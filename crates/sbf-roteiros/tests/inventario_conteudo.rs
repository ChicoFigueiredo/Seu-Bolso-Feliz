//! A baixa por conteúdo (spec 18): o contracheque do dia 30 substitui o do dia 20 no mesmo lugar
//! do portal, então "já baixei" se decide pelo CONTEÚDO. Um arquivo baixado é descartado se o
//! sha256 dele, ou a impressão do texto extraído, já existe no inventário. A **Baixa forçada**
//! (`forcando(true)`) baixa de novo mesmo assim.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use rusqlite::Connection;
use sbf_roteiros::executor::{Destino, Recebido};
use sbf_roteiros::inventario::NoInventario;
use std::path::{Path, PathBuf};

const SQL_IMPRESSAO: &str = include_str!("../../sbf-inventario/sql/0005-impressao-do-texto.sql");

/// O inventário de teste já com a migração 5 (a impressão do texto), como o real.
fn inventario(pasta: &Path) -> (PathBuf, Connection) {
    let (cofre, conn) = comum::inventario(pasta);
    conn.execute_batch(SQL_IMPRESSAO).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    (cofre, conn)
}

/// Um PDF fictício, legível pelo `lopdf` e pelo `pdftotext`, com o texto dado. `marca` só muda
/// os bytes (um comentário no fim): o texto extraído é o mesmo.
fn pdf(linhas: &[&str], marca: &str) -> Vec<u8> {
    let mut fluxo = String::from("BT /F1 12 Tf 50 700 Td 14 TL\n");
    for l in linhas {
        fluxo.push_str(&format!("({l}) Tj T*\n"));
    }
    fluxo.push_str("ET");
    let objetos = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_owned(),
        format!("<< /Length {} >>\nstream\n{fluxo}\nendstream", fluxo.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    let mut saida = String::from("%PDF-1.4\n");
    let mut posicoes = Vec::new();
    for (i, o) in objetos.iter().enumerate() {
        posicoes.push(saida.len());
        saida.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = saida.len();
    saida.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objetos.len() + 1
    ));
    for p in &posicoes {
        saida.push_str(&format!("{p:010} 00000 n \n"));
    }
    saida.push_str(&format!(
        "trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref}\n%%EOF\n% {marca}\n",
        objetos.len() + 1
    ));
    saida.into_bytes()
}

const CONTRACHEQUE: [&str; 4] = [
    "Contracheque de exemplo - Fulano de Tal - competencia 2026-10",
    "Matricula CX-0001 orgao exemplo.test cargo analista ficticio",
    "Vencimentos 1000000 descontos 200000 liquido 800000 centavos",
    "Documento ficticio apenas para teste automatizado do SBF",
];

const OUTRO_CONTRACHEQUE: [&str; 4] = [
    "Contracheque de exemplo - Fulano de Tal - competencia 2026-11",
    "Matricula CX-0001 orgao exemplo.test cargo analista ficticio",
    "Vencimentos 1100000 descontos 210000 liquido 890000 centavos",
    "Documento ficticio apenas para teste automatizado do SBF",
];

fn pdftotext_existe() -> bool {
    sbf_pdf::Ferramentas::default().pdftotext.is_file()
}

fn escrever(pasta: &Path, nome: &str, bytes: &[u8]) -> PathBuf {
    let c = pasta.join(nome);
    std::fs::write(&c, bytes).unwrap();
    c
}

fn contar(conn: &Connection, tabela: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {tabela}"), [], |l| l.get(0))
        .unwrap()
}

#[test]
fn o_mesmo_arquivo_baixado_duas_vezes_e_descartado_na_segunda() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let bytes = b"data;valor\n2026-10-20;100\n";

    let a = escrever(d.path(), "primeiro.csv", bytes);
    let mut primeira = NoInventario::novo(&conn, &cofre, "contracheque", 1);
    assert_eq!(primeira.receber(&a, "extrato.csv").unwrap(), Recebido::Novo);
    primeira.fechar("concluida").unwrap();
    assert_eq!(primeira_contagem(&conn), (1, 1));

    let b = escrever(d.path(), "segundo.csv", bytes);
    let mut segunda = NoInventario::novo(&conn, &cofre, "contracheque", 2);
    let r = segunda.receber(&b, "extrato.csv").unwrap();
    assert_eq!(r, Recebido::Descartado);
    assert_eq!(r.nota(), Some("já estava no inventário"));
    assert_eq!(segunda.descartados(), 1);
    assert!(!b.exists(), "o arquivo baixado é apagado");
    segunda.fechar("concluida").unwrap();
    // nenhum achado, nenhuma origem, nenhuma varredura nova
    assert_eq!(primeira_contagem(&conn), (1, 1));
    assert_eq!(contar(&conn, "varredura"), 1);
}

fn primeira_contagem(conn: &Connection) -> (i64, i64) {
    (contar(conn, "achado"), contar(conn, "origem"))
}

#[test]
fn o_mesmo_arquivo_ja_visto_por_outra_fonte_tambem_e_descartado() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let bytes = b"a;b\n1;2\n";
    let a = escrever(d.path(), "a.csv", bytes);
    // o Gmail já trouxe o mesmo arquivo
    let mut gmail = NoInventario::novo(&conn, &cofre, "gmail-antigo", 1);
    gmail.receber(&a, "a.csv").unwrap();
    gmail.fechar("concluida").unwrap();

    let b = escrever(d.path(), "b.csv", bytes);
    let mut dest = NoInventario::novo(&conn, &cofre, "outro", 7);
    assert_eq!(dest.receber(&b, "b.csv").unwrap(), Recebido::Descartado);
    assert!(!b.exists());
    assert_eq!(primeira_contagem(&conn), (1, 1));
}

#[test]
fn o_mesmo_texto_com_bytes_diferentes_e_descartado() {
    if !pdftotext_existe() {
        eprintln!("sem pdftotext: teste ignorado");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let a = escrever(d.path(), "a.pdf", &pdf(&CONTRACHEQUE, "emitido 1"));
    let b = escrever(d.path(), "b.pdf", &pdf(&CONTRACHEQUE, "emitido 2"));
    assert_ne!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());

    let mut primeira = NoInventario::novo(&conn, &cofre, "contracheque", 1);
    assert_eq!(
        primeira.receber(&a, "contracheque.pdf").unwrap(),
        Recebido::Novo
    );
    primeira.fechar("concluida").unwrap();
    // o texto e a impressão do achado novo ficam guardados: é com eles que a próxima compara
    assert_eq!(contar(&conn, "texto"), 1);
    let com_impressao: i64 = conn
        .query_row(
            "SELECT count(*) FROM impressao_do_texto WHERE impressao IS NOT NULL",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert_eq!(com_impressao, 1);

    let mut segunda = NoInventario::novo(&conn, &cofre, "contracheque", 2);
    let r = segunda.receber(&b, "contracheque.pdf").unwrap();
    assert_eq!(r, Recebido::Descartado, "mesmo texto, bytes diferentes");
    assert_eq!(segunda.descartados(), 1);
    assert!(!b.exists());
    assert_eq!(primeira_contagem(&conn), (1, 1));
}

#[test]
fn texto_diferente_nao_e_descartado() {
    if !pdftotext_existe() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let a = escrever(d.path(), "a.pdf", &pdf(&CONTRACHEQUE, "x"));
    let b = escrever(d.path(), "b.pdf", &pdf(&OUTRO_CONTRACHEQUE, "x"));
    let mut dest = NoInventario::novo(&conn, &cofre, "contracheque", 1);
    assert_eq!(dest.receber(&a, "20.pdf").unwrap(), Recebido::Novo);
    // o do dia 30 NÃO é o do dia 20: conteúdo diferente entra
    let mut outra = NoInventario::novo(&conn, &cofre, "contracheque", 2);
    assert_eq!(outra.receber(&b, "30.pdf").unwrap(), Recebido::Novo);
    assert_eq!(outra.descartados(), 0);
    assert_eq!(primeira_contagem(&conn), (2, 2));
}

#[test]
fn texto_curto_demais_nao_vira_duplicado_por_texto() {
    if !pdftotext_existe() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let curto = ["Recibo curto de exemplo CX-0001"];
    let a = escrever(d.path(), "a.pdf", &pdf(&curto, "1"));
    let b = escrever(d.path(), "b.pdf", &pdf(&curto, "2"));
    let mut dest = NoInventario::novo(&conn, &cofre, "r", 1);
    assert_eq!(dest.receber(&a, "a.pdf").unwrap(), Recebido::Novo);
    let mut outra = NoInventario::novo(&conn, &cofre, "r", 2);
    assert_eq!(
        outra.receber(&b, "a.pdf").unwrap(),
        Recebido::Novo,
        "menos de 200 caracteres: só vale o sha256"
    );
}

#[test]
fn pdf_ilegivel_cai_no_sha256_sem_erro() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let a = escrever(d.path(), "a.pdf", b"%PDF-1.4 isto nao e um pdf");
    let b = escrever(d.path(), "b.pdf", b"%PDF-1.4 isto nao e um pdf");
    let mut dest = NoInventario::novo(&conn, &cofre, "r", 1);
    assert_eq!(dest.receber(&a, "a.pdf").unwrap(), Recebido::Novo);
    let mut outra = NoInventario::novo(&conn, &cofre, "r", 2);
    assert_eq!(outra.receber(&b, "a.pdf").unwrap(), Recebido::Descartado);
    assert_eq!(contar(&conn, "texto"), 0);
}

#[test]
fn forcando_baixa_de_novo_o_mesmo_arquivo_como_nova_chegada() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let bytes = b"data;valor\n2026-10-20;100\n";
    let a = escrever(d.path(), "a.csv", bytes);
    let mut primeira = NoInventario::novo(&conn, &cofre, "contracheque", 1);
    primeira.receber(&a, "extrato.csv").unwrap();
    primeira.fechar("concluida").unwrap();

    let b = escrever(d.path(), "b.csv", bytes);
    let mut forcada = NoInventario::novo(&conn, &cofre, "contracheque", 2).forcando(true);
    let r = forcada.receber(&b, "extrato.csv").unwrap();
    assert_eq!(r, Recebido::Repetido, "o conteúdo já era conhecido");
    assert_eq!(forcada.descartados(), 0);
    forcada.fechar("concluida").unwrap();
    // o mesmo achado, uma nova chegada (origem da execução 2)
    assert_eq!(primeira_contagem(&conn), (1, 2));
    let chaves: Vec<String> = conn
        .prepare("SELECT origin_key FROM origem ORDER BY id")
        .unwrap()
        .query_map([], |l| l.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        chaves,
        [
            "roteiro:contracheque/1/extrato.csv",
            "roteiro:contracheque/2/extrato.csv"
        ]
    );
    // a mesma execução não repete a chegada
    let c = escrever(d.path(), "c.csv", bytes);
    let mut de_novo = NoInventario::novo(&conn, &cofre, "contracheque", 2).forcando(true);
    assert_eq!(
        de_novo.receber(&c, "extrato.csv").unwrap(),
        Recebido::Repetido
    );
    assert_eq!(primeira_contagem(&conn), (1, 2));
}

#[test]
fn forcando_o_mesmo_texto_com_bytes_diferentes_entra() {
    if !pdftotext_existe() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let a = escrever(d.path(), "a.pdf", &pdf(&CONTRACHEQUE, "1"));
    let b = escrever(d.path(), "b.pdf", &pdf(&CONTRACHEQUE, "2"));
    let mut dest = NoInventario::novo(&conn, &cofre, "contracheque", 1);
    dest.receber(&a, "c.pdf").unwrap();
    let mut forcada = NoInventario::novo(&conn, &cofre, "contracheque", 2).forcando(true);
    assert_eq!(forcada.receber(&b, "c.pdf").unwrap(), Recebido::Novo);
    assert_eq!(primeira_contagem(&conn), (2, 2));
    assert!(b.exists(), "o arquivo forçado não é apagado pelo destino");
}

#[test]
fn nao_forcando_e_o_padrao() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let a = escrever(d.path(), "a.csv", b"1;2\n");
    let b = escrever(d.path(), "b.csv", b"1;2\n");
    NoInventario::novo(&conn, &cofre, "r", 1)
        .receber(&a, "a.csv")
        .unwrap();
    let r = NoInventario::novo(&conn, &cofre, "r", 2)
        .forcando(false)
        .receber(&b, "a.csv")
        .unwrap();
    assert_eq!(r, Recebido::Descartado);
}
