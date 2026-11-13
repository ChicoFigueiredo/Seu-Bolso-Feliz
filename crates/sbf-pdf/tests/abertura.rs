//! A abertura em memória contra as cifras do spike: o texto sai igual ao do `pdftotext` sobre
//! o original sem senha; senha errada e corrompido são distintos.

mod comum;

use comum::*;
use sbf_pdf::{Candidata, Desfecho, Ferramentas, PdfProtegido, Via};
use zeroize::Zeroizing;

fn candidata(valor: &str) -> Candidata {
    Candidata::nova(Zeroizing::new(valor.to_owned()))
}

fn sem_qpdf() -> Ferramentas {
    Ferramentas {
        qpdf: None,
        ..Ferramentas::default()
    }
}

#[test]
fn cada_cifra_abre_em_memoria_com_o_texto_do_pdftotext() {
    let pasta = pasta();
    let base = pdf_ficticio();
    std::fs::write(pasta.path().join("referencia.pdf"), &base).expect("base");
    let referencia = pdftotext_de(&pasta.path().join("referencia.pdf"));
    assert!(referencia.contains("BANCO ALFA FICTICIO"), "{referencia}");
    assert!(
        referencia.contains("João da Conceição Fictício"),
        "{referencia}"
    );
    assert!(!referencia.contains("MARCA-PAGINA-4"));

    for cifra in Cifra::TODAS {
        let caminho = cifrar(
            pasta.path(),
            &format!("{cifra:?}"),
            &base,
            CPF_ULTIMOS_5,
            cifra,
        );
        let original = std::fs::read(&caminho).expect("ler");
        let pdf = PdfProtegido::ler(&caminho).expect("ler");
        assert!(pdf.precisa_de_senha(), "{cifra:?}");
        assert_eq!(
            pdf.tentar(&candidata("00000"), &sem_qpdf()),
            Desfecho::SenhaErrada,
            "{cifra:?}"
        );
        match pdf.tentar(&candidata(CPF_ULTIMOS_5), &sem_qpdf()) {
            Desfecho::Aberto(t) => {
                assert_eq!(t.via, Via::Lopdf, "{cifra:?}");
                assert!(!t.reparado, "{cifra:?}");
                assert_eq!(
                    t.texto, referencia,
                    "{cifra:?}: texto diferente do pdftotext"
                );
            }
            outro => panic!("{cifra:?}: {outro:?}"),
        }
        // A senha de dono também abre.
        assert!(matches!(
            pdf.tentar(&candidata("dono-ficticio-77"), &sem_qpdf()),
            Desfecho::Aberto(_)
        ));
        // O original nunca muda.
        assert_eq!(std::fs::read(&caminho).expect("ler"), original, "{cifra:?}");
    }
}

#[test]
fn o_fallback_do_qpdf_da_o_mesmo_texto() {
    let pasta = pasta();
    let base = pdf_ficticio();
    std::fs::write(pasta.path().join("referencia.pdf"), &base).expect("base");
    let referencia = pdftotext_de(&pasta.path().join("referencia.pdf"));
    let caminho = cifrar(pasta.path(), "aes256", &base, CPF_ULTIMOS_5, Cifra::Aes256);
    let ferramentas = Ferramentas::default();
    match sbf_pdf::extrair_pelo_qpdf(&caminho, &candidata(CPF_ULTIMOS_5), &ferramentas) {
        Desfecho::Aberto(t) => {
            assert_eq!(t.via, Via::Qpdf);
            assert_eq!(t.texto, referencia);
        }
        outro => panic!("{outro:?}"),
    }
    assert_eq!(
        sbf_pdf::extrair_pelo_qpdf(&caminho, &candidata("00000"), &ferramentas),
        Desfecho::SenhaErrada
    );
}

#[test]
fn corrompidos_nao_sao_senha_errada() {
    let pasta = pasta();
    let base = pdf_ficticio();
    let caminho = cifrar(pasta.path(), "aes256", &base, CPF_ULTIMOS_5, Cifra::Aes256);
    let cifrado = std::fs::read(&caminho).expect("ler");

    let lixo = b"%PDF-1.7\nlixo que nao e pdf nenhum\n%%EOF\n".to_vec();
    let mut cortado = cifrado.clone();
    cortado.truncate(3000.min(cifrado.len() / 2));
    for (nome, bytes) in [("lixo", lixo), ("cortado", cortado)] {
        let pdf = PdfProtegido::de_bytes(bytes);
        let desfecho = pdf.tentar(&candidata(CPF_ULTIMOS_5), &sem_qpdf());
        assert_eq!(desfecho, Desfecho::Corrompido, "{nome}");
    }
}

#[test]
fn pdf_sem_senha_abre_sem_candidata() {
    let pdf = PdfProtegido::de_bytes(pdf_ficticio());
    assert!(!pdf.precisa_de_senha());
    match pdf.abrir_sem_senha(&sem_qpdf()) {
        Desfecho::Aberto(t) => assert_eq!(t.via, Via::SemSenha),
        outro => panic!("{outro:?}"),
    }
}
