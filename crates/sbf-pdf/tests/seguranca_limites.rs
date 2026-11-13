//! Achado A10 da revisão de segurança da fatia 1: um anexo hostil não ocupa a memória sem
//! limite. O PDF acima de [`TAMANHO_MAXIMO_DO_PDF`] nem é lido, e a saída do `pdftotext`
//! é cortada em [`SAIDA_MAXIMA_DO_FILHO`] (o resto é drenado, para o filho não travar).
//! Arquivos fictícios, gerados aqui.

#![cfg(unix)]

mod comum;

use comum::*;
use sbf_pdf::abertura::TAMANHO_MAXIMO_DO_PDF;
use sbf_pdf::processo::SAIDA_MAXIMA_DO_FILHO;
use sbf_pdf::{Desfecho, Ferramentas, PdfProtegido};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

#[test]
fn pdf_grande_demais_nem_e_lido() {
    assert_eq!(TAMANHO_MAXIMO_DO_PDF, 50 * 1024 * 1024);
    let pasta = pasta();
    let caminho = pasta.path().join("grande.pdf");
    let arquivo = std::fs::File::create(&caminho).expect("criar");
    // Esparso: não ocupa o disco.
    arquivo.set_len(TAMANHO_MAXIMO_DO_PDF + 1).expect("tamanho");
    let erro = PdfProtegido::ler(&caminho).expect_err("devia recusar");
    assert_eq!(erro.kind(), std::io::ErrorKind::FileTooLarge);

    // No limite exato, lê (e é só um arquivo ilegível).
    std::fs::File::create(&caminho)
        .expect("criar")
        .set_len(1024)
        .expect("tamanho");
    assert!(PdfProtegido::ler(&caminho).is_ok());
}

#[test]
fn saida_enorme_do_pdftotext_e_cortada_sem_travar() {
    let pasta = pasta();
    let script = pasta.path().join("pdftotext-falante");
    // 40 MB de saída, muito acima do teto.
    std::fs::write(
        &script,
        "#!/bin/sh\ncat > /dev/null\nhead -c 40000000 /dev/zero | tr '\\000' a\n",
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let caminho = pasta.path().join("comum.pdf");
    std::fs::write(&caminho, pdf_ficticio()).expect("pdf");
    let f = Ferramentas {
        pdftotext: script,
        qpdf: None,
        limite_de_tempo: Duration::from_secs(20),
    };
    let inicio = Instant::now();
    let desfecho = PdfProtegido::ler(&caminho)
        .expect("ler")
        .abrir_sem_senha(&f);
    assert!(inicio.elapsed() < Duration::from_secs(15), "travou");
    match desfecho {
        Desfecho::Aberto(t) => {
            assert!(t.texto.len() <= SAIDA_MAXIMA_DO_FILHO, "{}", t.texto.len());
            assert!(t.texto.len() >= SAIDA_MAXIMA_DO_FILHO / 2);
        }
        outro => panic!("{outro:?}"),
    }
}
