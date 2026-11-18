//! O OCR de verdade, com o tesseract do sistema, sobre uma imagem fictícia de app em tela
//! escura (`tests/fixtures/comprovante-escuro.png`): "Comprovante de pagamento", "Valor pago
//! R$ 1.234,56". Onde não há tesseract, os testes que o exigem dizem isso e passam.

use std::path::{Path, PathBuf};

use sbf_inventario::banco::Inventario;
use sbf_ocr::{DesfechoDoOcr, FalhaDoOcr, LIMITE_DA_IMAGEM, METODO_OCR, Ocr, ocr_do_achado};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/comprovante-escuro.png")
}

fn ocr_ou_pula() -> Option<Ocr> {
    let ocr = Ocr::do_sistema();
    if ocr.disponivel() {
        Some(ocr)
    } else {
        eprintln!("sem tesseract neste sistema: teste pulado");
        None
    }
}

#[test]
fn imagem_escura_com_texto_claro_sai_com_o_texto() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let texto = ocr.ler(&fixture()).expect("ler");
    assert!(texto.contains("Valor pago"), "texto: {texto}");
    assert!(texto.contains("1.234,56"), "texto: {texto}");
    assert!(texto.contains("Comprovante de pagamento"), "texto: {texto}");
}

#[test]
fn sem_tesseract_a_falha_e_indisponivel_e_nao_um_erro_de_leitura() {
    let ocr = Ocr::com_programa(PathBuf::from("/nao/existe/tesseract"));
    assert!(!ocr.disponivel());
    assert_eq!(ocr.ler(&fixture()), Err(FalhaDoOcr::Indisponivel));
}

#[test]
fn arquivo_que_nao_e_imagem_e_ilegivel() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let pasta = tempfile::tempdir().expect("pasta");
    let falso = pasta.path().join("falso.png");
    std::fs::write(&falso, b"isto nao e um png").expect("escrever");
    assert_eq!(ocr.ler(&falso), Err(FalhaDoOcr::ImagemIlegivel));
}

#[test]
fn imagem_acima_do_limite_nao_e_lida() {
    let pasta = tempfile::tempdir().expect("pasta");
    let grande = pasta.path().join("grande.png");
    let f = std::fs::File::create(&grande).expect("criar");
    f.set_len(LIMITE_DA_IMAGEM + 1).expect("tamanho");
    let ocr = Ocr::com_programa(PathBuf::from("/nao/existe/tesseract"));
    // O limite vem antes de procurar o programa: nem abre o arquivo.
    assert_eq!(ocr.ler(&grande), Err(FalhaDoOcr::ImagemGrande));
}

struct Cofre {
    _pasta: tempfile::TempDir,
    banco: PathBuf,
}

fn cofre_com_achado(hash: &str) -> Cofre {
    let pasta = tempfile::tempdir().expect("pasta");
    let banco = pasta.path().join("inventario.sqlite");
    let mut inv = Inventario::abrir(&banco).expect("abrir");
    inv.migrar(pasta.path().join("copia.sqlite"))
        .expect("migrar");
    let c = rusqlite::Connection::open(&banco).expect("conexao");
    c.execute(
        "INSERT INTO achado (content_hash, file_size_bytes, camada) VALUES (?1, 10, 'A')",
        [hash],
    )
    .expect("achado");
    Cofre {
        _pasta: pasta,
        banco,
    }
}

#[test]
fn o_texto_do_ocr_fica_em_texto_com_o_metodo_ocr_tesseract() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let cofre = cofre_com_achado("h1");
    let c = rusqlite::Connection::open(&cofre.banco).expect("conexao");

    let d = ocr_do_achado(&c, "h1", &fixture(), &ocr).expect("ocr");

    assert!(matches!(d, DesfechoDoOcr::Lido(ref t) if t.contains("Valor pago")));
    let (metodo, conteudo, impressao): (String, String, Option<String>) = c
        .query_row(
            "SELECT t.metodo, t.conteudo, a.canonical_fingerprint
               FROM texto t JOIN achado a USING (content_hash)",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .expect("texto");
    assert_eq!(metodo, METODO_OCR);
    assert_eq!(metodo, "ocr-tesseract");
    assert!(conteudo.contains("1.234,56"));
    assert!(impressao.is_some());
}

#[test]
fn texto_que_o_inventario_ja_tinha_e_reaproveitado_sem_rodar_o_ocr() {
    let cofre = cofre_com_achado("h2");
    let c = rusqlite::Connection::open(&cofre.banco).expect("conexao");
    c.execute(
        "INSERT INTO texto (content_hash, metodo, conteudo) VALUES ('h2', 'ocr-por', 'texto antigo')",
        [],
    )
    .expect("texto");
    // Nem precisa de tesseract: o texto já existe.
    let ocr = Ocr::com_programa(PathBuf::from("/nao/existe/tesseract"));

    let d = ocr_do_achado(&c, "h2", &fixture(), &ocr).expect("ocr");

    assert_eq!(d, DesfechoDoOcr::JaHavia("texto antigo".to_owned()));
}

#[test]
fn sem_tesseract_nada_e_gravado_e_o_desfecho_e_indisponivel() {
    let cofre = cofre_com_achado("h3");
    let c = rusqlite::Connection::open(&cofre.banco).expect("conexao");
    let ocr = Ocr::com_programa(PathBuf::from("/nao/existe/tesseract"));

    let d = ocr_do_achado(&c, "h3", &fixture(), &ocr).expect("ocr");

    assert_eq!(d, DesfechoDoOcr::Indisponivel);
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM texto", [], |l| l.get(0))
        .expect("contar");
    assert_eq!(
        n, 0,
        "sem tesseract não se grava nada: tenta de novo depois"
    );
}
