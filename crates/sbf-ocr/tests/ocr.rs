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

// ---------------------------------------------------------------- imagens novas do inventário

use std::sync::atomic::AtomicBool;

use sbf_ocr::ler_imagens_novas;

/// Um instante qualquer (segundos desde 1970) para os testes do lote.
const AGORA: u64 = 1_800_000_000;

struct Acervo {
    pasta: tempfile::TempDir,
}

impl Acervo {
    fn novo() -> Self {
        let pasta = tempfile::tempdir().expect("pasta");
        let mut inv = Inventario::abrir(pasta.path().join("inventario.sqlite")).expect("abrir");
        inv.migrar(pasta.path().join("copia.sqlite"))
            .expect("migrar");
        Self { pasta }
    }

    fn conexao(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.pasta.path().join("inventario.sqlite")).expect("conexao")
    }

    /// Um achado com a imagem no acervo do cofre.
    fn achado(&self, hash: &str, ext: &str, tamanho: i64, bytes: &[u8]) {
        let relativo = format!("acervo/ab/{hash}.{ext}");
        let destino = self.pasta.path().join(&relativo);
        std::fs::create_dir_all(destino.parent().expect("pai")).expect("pasta");
        std::fs::write(&destino, bytes).expect("escrever");
        self.conexao()
            .execute(
                "INSERT INTO achado (content_hash, file_size_bytes, extensao, camada, caminho_acervo)
                 VALUES (?1, ?2, ?3, 'A', ?4)",
                rusqlite::params![hash, tamanho, ext, relativo],
            )
            .expect("achado");
    }
}

fn png() -> Vec<u8> {
    std::fs::read(fixture()).expect("fixture")
}

#[test]
fn o_lote_le_so_imagens_novas_pequenas_e_grava_ocr_tesseract() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let a = Acervo::novo();
    a.achado("nova", "png", 100, &png());
    a.achado("pdf", "pdf", 100, b"%PDF-1.4");
    a.achado(
        "enorme",
        "png",
        i64::try_from(LIMITE_DA_IMAGEM).expect("i64") + 1,
        &png(),
    );
    a.achado("com-texto", "png", 100, &png());
    a.conexao()
        .execute(
            "INSERT INTO texto (content_hash, metodo, conteudo) VALUES ('com-texto', 'ocr-por', 'x')",
            [],
        )
        .expect("texto");
    let c = a.conexao();

    let r = ler_imagens_novas(&c, a.pasta.path(), &ocr, 10, AGORA, &AtomicBool::new(false))
        .expect("lote");

    assert_eq!((r.lidas, r.sem_texto, r.falhas), (1, 0, 0));
    assert!(!r.indisponivel);
    let (metodo, conteudo): (String, String) = c
        .query_row(
            "SELECT metodo, conteudo FROM texto WHERE content_hash = 'nova'",
            [],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .expect("texto");
    assert_eq!(metodo, "ocr-tesseract");
    assert!(conteudo.contains("Valor pago"));
    // O pdf e a imagem enorme ficaram sem texto.
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM texto", [], |l| l.get(0))
        .expect("contar");
    assert_eq!(n, 2);
}

#[test]
fn o_lote_respeita_o_tamanho_e_diz_quantas_faltam() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let a = Acervo::novo();
    for h in ["i1", "i2", "i3"] {
        a.achado(h, "png", 100, &png());
    }
    let c = a.conexao();
    let r = ler_imagens_novas(&c, a.pasta.path(), &ocr, 2, AGORA, &AtomicBool::new(false))
        .expect("lote");
    assert_eq!(r.lidas, 2);
    assert_eq!(r.pendentes, 1);
}

#[test]
fn imagem_que_falha_nao_e_tentada_de_novo_antes_do_recuo_e_volta_depois() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let a = Acervo::novo();
    a.achado("quebrada", "png", 100, b"isto nao e um png");
    let c = a.conexao();
    let lote = |agora: u64| {
        ler_imagens_novas(&c, a.pasta.path(), &ocr, 10, agora, &AtomicBool::new(false))
            .expect("lote")
    };

    let r1 = lote(AGORA);
    assert_eq!((r1.lidas, r1.falhas), (0, 1));
    assert_eq!(r1.causa, Some("formato"));
    let r2 = lote(AGORA + 60);
    assert_eq!((r2.lidas, r2.falhas, r2.pendentes), (0, 0, 0));
    assert_eq!(r2.causa, None);
    // Passada a 1ª espera (1 h), tenta de novo e falha: agora espera 6 h.
    assert_eq!(lote(AGORA + 3_600).falhas, 1);
    assert_eq!(lote(AGORA + 3_600 + 6 * 3_600 - 1).falhas, 0);
    // Depois de 6 h, 24 h; a 4ª falha desiste.
    assert_eq!(lote(AGORA + 3_600 + 6 * 3_600).falhas, 1);
    let t3 = AGORA + 3_600 + 6 * 3_600;
    assert_eq!(lote(t3 + 24 * 3_600 - 1).falhas, 0);
    assert_eq!(lote(t3 + 24 * 3_600).falhas, 1);
    assert_eq!(
        lote(t3 + 24 * 3_600 + 400 * 24 * 3_600).falhas,
        0,
        "desistiu"
    );
    let (tentativas, causa): (i64, String) = c
        .query_row(
            "SELECT tentativas, causa FROM ocr_falha WHERE content_hash = 'quebrada'",
            [],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .expect("falha registrada");
    assert_eq!((tentativas, causa.as_str()), (4, "formato"));
}

#[test]
fn imagem_sem_arquivo_falha_com_causa_e_tambem_recua() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let a = Acervo::novo();
    a.achado("sumiu", "png", 100, &png());
    std::fs::remove_file(a.pasta.path().join("acervo/ab/sumiu.png")).expect("remover");
    let c = a.conexao();
    let r = ler_imagens_novas(&c, a.pasta.path(), &ocr, 10, AGORA, &AtomicBool::new(false))
        .expect("lote");
    assert_eq!((r.falhas, r.causa), (1, Some("sem_arquivo")));
    let r = ler_imagens_novas(
        &c,
        a.pasta.path(),
        &ocr,
        10,
        AGORA + 1,
        &AtomicBool::new(false),
    )
    .expect("lote");
    assert_eq!((r.falhas, r.pendentes), (0, 0));
}

#[test]
fn falhas_recuadas_nao_contam_como_pendentes_nem_escondem_as_boas() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let a = Acervo::novo();
    a.achado("ruim", "png", 100, b"lixo");
    a.achado("boa", "png", 100, &png());
    let c = a.conexao();
    let r1 = ler_imagens_novas(&c, a.pasta.path(), &ocr, 1, AGORA, &AtomicBool::new(false))
        .expect("lote");
    let r2 = ler_imagens_novas(
        &c,
        a.pasta.path(),
        &ocr,
        1,
        AGORA + 1,
        &AtomicBool::new(false),
    )
    .expect("lote");
    // Uma foi lida e a outra falhou, nas duas voltas, e nada sobra.
    assert_eq!(r1.lidas + r2.lidas, 1);
    assert_eq!(r1.falhas + r2.falhas, 1);
    assert_eq!(r2.pendentes, 0);
}

#[test]
fn sem_tesseract_o_lote_e_indisponivel_e_nao_toca_no_banco() {
    let a = Acervo::novo();
    a.achado("nova", "png", 100, &png());
    let c = a.conexao();
    let ocr = Ocr::com_programa(PathBuf::from("/nao/existe/tesseract"));

    let r = ler_imagens_novas(&c, a.pasta.path(), &ocr, 10, AGORA, &AtomicBool::new(false))
        .expect("lote");

    assert!(r.indisponivel);
    assert_eq!((r.lidas, r.sem_texto, r.falhas), (0, 0, 0));
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM texto", [], |l| l.get(0))
        .expect("contar");
    assert_eq!(n, 0);
}

#[test]
fn o_lote_para_quando_cancelado() {
    let Some(ocr) = ocr_ou_pula() else { return };
    let a = Acervo::novo();
    a.achado("i1", "png", 100, &png());
    let c = a.conexao();
    let r = ler_imagens_novas(&c, a.pasta.path(), &ocr, 10, AGORA, &AtomicBool::new(true))
        .expect("lote");
    assert_eq!(r.lidas, 0);
}
