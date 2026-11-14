//! Apoio dos testes: todo arquivo é fictício e gerado aqui mesmo (PDF pelo lopdf, cifrado
//! com `qpdf --encrypt`; imagens pelo `image`; e-mail, zip e csv escritos à mão).
//!
//! **A senha fictícia vai no argv do `qpdf --encrypt` destes testes.** É aceitável (valor de
//! teste, processo de teste) e é o único lugar: o código de produção nunca põe senha em argv.
//!
//! As pastas ficam em `CARGO_TARGET_TMPDIR` (em `target/`, não oculta) e o PDF termina em
//! `.pdf`, porque o AppArmor do `qpdf` no Ubuntu só deixa ler e gravar `*.pdf` fora de pastas
//! ocultas.
#![allow(dead_code)]

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use sbf_previa::{Candidata, Conteudo, Pedido};
use std::path::{Path, PathBuf};
use std::process::Command;
use zeroize::Zeroizing;

pub const SENHA: &str = "senha-ficticia-Previa-42";

/// `CARGO_TARGET_TMPDIR`, salvo se algum componente do caminho for oculto (ex.:
/// `CARGO_TARGET_DIR=~/.cache/...`): ali o AppArmor do `qpdf` nega `open` ("Permission denied",
/// exit 2). Nesse caso usa `/tmp`, que ele permite para `*.pdf`.
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
        .prefix("sbf-previa-teste-")
        .tempdir_in(raiz_das_pastas())
        .expect("pasta de teste")
}

pub fn candidata(valor: &str) -> Candidata {
    Candidata::nova(Zeroizing::new(valor.to_owned()))
}

pub fn abrir(arquivo: &Path, candidatas: Vec<Candidata>) -> Conteudo {
    sbf_previa::abrir(Pedido {
        arquivo,
        extensao: "",
        candidatas,
    })
    .expect("abrir")
}

/// Um PDF fictício de `n` páginas A4; a página `k` tem um retângulo preto de altura
/// proporcional a `k` no alto da página e o texto `PAGINA k`.
pub fn pdf_ficticio(n: u32) -> Vec<u8> {
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
    for k in 1..=n {
        let ops = vec![
            Operation::new("g", vec![0.into()]),
            Operation::new(
                "re",
                vec![
                    0.into(),
                    (842 - 100 * k as i64).into(),
                    595.into(),
                    (100 * k as i64).into(),
                ],
            ),
            Operation::new("f", vec![]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![60.into(), 300.into()]),
            Operation::new(
                "Tj",
                vec![Object::string_literal(format!("PAGINA {k} ficticia"))],
            ),
            Operation::new("ET", vec![]),
        ];
        let content = Content { operations: ops }.encode().expect("conteúdo");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));
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
            "Count" => n as i64,
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

/// Grava `base` em `<pasta>/<nome>-base.pdf` e cifra (AES-256) com `qpdf --encrypt` em
/// `<pasta>/<nome>.pdf`.
pub fn cifrar(pasta: &Path, nome: &str, base: &[u8], senha: &str) -> PathBuf {
    let entrada = pasta.join(format!("{nome}-base.pdf"));
    let saida = pasta.join(format!("{nome}.pdf"));
    std::fs::write(&entrada, base).expect("base");
    let saida_qpdf = Command::new("qpdf")
        .arg("--encrypt")
        .arg(format!("--user-password={senha}"))
        .arg("--owner-password=dono-ficticio-77")
        .args(["--bits=256", "--"])
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

/// Uma imagem RGB fictícia `largura` x `altura`: metade de cima vermelha, de baixo azul.
pub fn imagem_ficticia(largura: u32, altura: u32) -> image::RgbImage {
    image::RgbImage::from_fn(largura, altura, |_, y| {
        if y < altura / 2 {
            image::Rgb([220, 20, 20])
        } else {
            image::Rgb([20, 20, 220])
        }
    })
}

pub fn gravar_imagem(caminho: &Path, largura: u32, altura: u32) {
    imagem_ficticia(largura, altura)
        .save(caminho)
        .expect("gravar imagem");
}

/// O pixel (x, y) de uma imagem RGBA.
pub fn pixel(imagem: &sbf_previa::Imagem, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * imagem.largura + x) * 4) as usize;
    [
        imagem.rgba[i],
        imagem.rgba[i + 1],
        imagem.rgba[i + 2],
        imagem.rgba[i + 3],
    ]
}

pub fn escuro(p: [u8; 4]) -> bool {
    u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 100
}
