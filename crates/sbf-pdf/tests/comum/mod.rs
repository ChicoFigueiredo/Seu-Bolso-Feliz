//! Apoio dos testes: PDF fictício gerado aqui mesmo (lopdf) e cifrado com `qpdf --encrypt`.
//!
//! **A senha fictícia vai no argv do `qpdf --encrypt` destes testes.** É aceitável (valor de
//! teste, processo de teste) e é o único lugar: o código de produção nunca põe senha em argv.
//!
//! As pastas ficam em `CARGO_TARGET_TMPDIR` (em `target/`, não oculta) e os arquivos terminam
//! em `.pdf`, porque o AppArmor do `qpdf` no Ubuntu só deixa ler e gravar `*.pdf` fora de
//! pastas ocultas.
#![allow(dead_code)]

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use std::path::{Path, PathBuf};
use std::process::Command;

/// CPF de exemplo de documentação (fictício) e as senhas que saem dele.
pub const CPF: &str = "52998224725";
pub const CPF_ULTIMOS_5: &str = "24725";
pub const CPF_PRIMEIROS_5: &str = "52998";
pub const SENHA_PROPRIA: &str = "senha-ficticia-Beta-90";

/// `CARGO_TARGET_TMPDIR`, salvo se algum componente do caminho for oculto (ex.:
/// `CARGO_TARGET_DIR=~/.cache/...`): ali o AppArmor do `qpdf` nega `open` ("Permission denied",
/// exit 2) e o teste cairia por causa do ambiente. Nesse caso usa `/tmp`, que ele permite
/// para `*.pdf`.
fn raiz_das_pastas() -> PathBuf {
    let alvo = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let oculto = alvo.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|n| n.starts_with('.') && n != "." && n != "..")
    });
    if oculto { PathBuf::from("/tmp") } else { alvo }
}

pub fn pasta() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("sbf-pdf-teste-")
        .tempdir_in(raiz_das_pastas())
        .expect("pasta de teste")
}

/// Uma linha de texto em Helvetica (WinAnsi) na posição (x, y).
fn linha(ops: &mut Vec<Operation>, x: i64, y: i64, texto: &[u8]) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new("Tf", vec!["F1".into(), 11.into()]));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new(
        "Tj",
        vec![Object::string_literal(texto.to_vec())],
    ));
    ops.push(Operation::new("ET", vec![]));
}

fn pagina(numero: u32) -> Vec<u8> {
    let mut ops = Vec::new();
    match numero {
        1 => {
            linha(&mut ops, 60, 780, b"BANCO ALFA FICTICIO - FATURA DO CARTAO");
            linha(
                &mut ops,
                60,
                760,
                b"Vencimento 10/12/2026 - Valor R$ 1.234,56",
            );
            // "Cliente: João da Conceição Fictício" em WinAnsi.
            linha(
                &mut ops,
                60,
                740,
                b"Cliente: Jo\xe3o da Concei\xe7\xe3o Fict\xedcio",
            );
            linha(
                &mut ops,
                60,
                720,
                b"Linha digitavel 23790.12345 60000.123456 78901.234567 8 99990000123456",
            );
            for (i, (data, descricao, valor)) in [
                (
                    &b"05/11/2026"[..],
                    &b"MERCADO EXEMPLO LTDA"[..],
                    &b"123,45"[..],
                ),
                (b"07/11/2026", b"POSTO FICTICIO", b"250,00"),
                (b"09/11/2026", b"FARMACIA DE TESTE", b"61,11"),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 680 - 18 * i as i64;
                linha(&mut ops, 60, y, data);
                linha(&mut ops, 160, y, descricao);
                linha(&mut ops, 460, y, valor);
            }
        }
        4 => linha(&mut ops, 60, 780, b"MARCA-PAGINA-4"),
        n => linha(
            &mut ops,
            60,
            780,
            format!("Pagina {n} da fatura ficticia, sem segredo").as_bytes(),
        ),
    }
    Content { operations: ops }.encode().expect("conteúdo")
}

/// Um PDF fictício de 4 páginas; a página 4 só tem `MARCA-PAGINA-4`.
pub fn pdf_ficticio() -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let mut kids = Vec::new();
    for n in 1..=4 {
        let content_id = doc.add_object(Stream::new(dictionary! {}, pagina(n)));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => 4,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut saida = Vec::new();
    doc.save_to(&mut saida).expect("salvar");
    saida
}

/// As cifras do spike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cifra {
    Rc4_40,
    Rc4_128,
    Aes128,
    Aes256,
    Aes256ComObjectStreams,
}

impl Cifra {
    pub const TODAS: [Cifra; 5] = [
        Cifra::Rc4_40,
        Cifra::Rc4_128,
        Cifra::Aes128,
        Cifra::Aes256,
        Cifra::Aes256ComObjectStreams,
    ];
}

/// Grava `base` em `<pasta>/<nome>-base.pdf` e cifra com `qpdf --encrypt` em
/// `<pasta>/<nome>.pdf`. A senha de dono é outra, também fictícia.
pub fn cifrar(pasta: &Path, nome: &str, base: &[u8], senha: &str, cifra: Cifra) -> PathBuf {
    let entrada = pasta.join(format!("{nome}-base.pdf"));
    let saida = pasta.join(format!("{nome}.pdf"));
    std::fs::write(&entrada, base).expect("base");
    let mut qpdf = Command::new("qpdf");
    if matches!(cifra, Cifra::Rc4_40 | Cifra::Rc4_128) {
        qpdf.arg("--allow-weak-crypto");
    }
    qpdf.arg("--encrypt")
        .arg(format!("--user-password={senha}"))
        .arg("--owner-password=dono-ficticio-77");
    match cifra {
        Cifra::Rc4_40 => qpdf.args(["--bits=40"]),
        Cifra::Rc4_128 => qpdf.args(["--bits=128", "--use-aes=n"]),
        Cifra::Aes128 => qpdf.args(["--bits=128", "--use-aes=y"]),
        Cifra::Aes256 => qpdf.args(["--bits=256"]),
        Cifra::Aes256ComObjectStreams => qpdf.args(["--bits=256"]),
    };
    // As opções de cifra terminam no `--`; `--object-streams` vem depois dele.
    qpdf.arg("--");
    if cifra == Cifra::Aes256ComObjectStreams {
        qpdf.arg("--object-streams=generate");
    }
    let saida_qpdf = qpdf
        .arg(&entrada)
        .arg(&saida)
        .output()
        .expect("qpdf instalado");
    let status = saida_qpdf.status;
    assert!(
        status.code() == Some(0) || status.code() == Some(3),
        "qpdf --encrypt: {status}: {}",
        String::from_utf8_lossy(&saida_qpdf.stderr)
    );
    saida
}

/// A referência: `pdftotext -layout -l 3 <arquivo> -` sobre o PDF sem senha, como o
/// `e3-extrair.ts` roda hoje.
pub fn pdftotext_de(caminho: &Path) -> String {
    let saida = Command::new("pdftotext")
        .args(["-layout", "-l", "3"])
        .arg(caminho)
        .arg("-")
        .output()
        .expect("pdftotext instalado");
    assert!(saida.status.success());
    String::from_utf8_lossy(&saida.stdout).into_owned()
}

pub fn sha256(bytes: &[u8]) -> String {
    sbf_pdf::texto::impressao_canonica(&String::from_utf8_lossy(bytes))
}
