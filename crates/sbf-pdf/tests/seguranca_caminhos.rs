//! Achados A14 e A15 da revisão de segurança da fatia 1 (Informativos):
//!
//! - A14: o `caminho_acervo` do banco não lê arquivo fora do cofre (`..`, absoluto), e o
//!   caminho local da origem só vale absoluto; o `qpdf` só recebe caminho absoluto (um
//!   relativo começando com `-` ou `@` seria lido como opção);
//! - A15: uma `Estrategia` montada direto, sem o `FromStr`, com um número fora de 1 a 11,
//!   dá erro em vez de pânico.
//!
//! Arquivos e CPF fictícios (CPF de exemplo de documentação).

#![cfg(unix)]

mod comum;

use comum::*;
use sbf_inventario::protegidos::AlvoProtegido;
use sbf_pdf::rodada::localizar;
use sbf_pdf::{
    Candidata, Desfecho, ErroDeFonte, Estrategia, FalhaDeFerramenta, Ferramentas, FonteDeSegredos,
    extrair_pelo_qpdf,
};
use std::path::Path;
use zeroize::Zeroizing;

fn alvo(acervo: Option<&str>, locais: Vec<(String, Option<i64>)>) -> AlvoProtegido {
    AlvoProtegido {
        content_hash: "ab".repeat(32),
        caminho_acervo: acervo.map(str::to_owned),
        caminhos_locais: locais,
        file_size_bytes: 10,
        fornecedor: None,
        tipo_de_documento: None,
        falhas: 0,
    }
}

#[test]
fn caminho_do_acervo_nao_sai_do_cofre() {
    let pasta = pasta();
    let cofre = pasta.path().join("cofre");
    std::fs::create_dir_all(cofre.join("acervo/ab")).expect("acervo");
    std::fs::write(cofre.join("acervo/ab/bom.pdf"), b"%PDF").expect("bom");
    std::fs::write(pasta.path().join("fora.pdf"), b"%PDF fora").expect("fora");
    let fora_absoluto = pasta.path().join("fora.pdf").to_string_lossy().into_owned();

    assert_eq!(
        localizar(&cofre, &alvo(Some("acervo/ab/bom.pdf"), vec![])),
        Some(cofre.join("acervo/ab/bom.pdf"))
    );
    for hostil in [
        "../fora.pdf",
        "acervo/../../fora.pdf",
        fora_absoluto.as_str(),
    ] {
        assert_eq!(
            localizar(&cofre, &alvo(Some(hostil), vec![])),
            None,
            "{hostil}"
        );
    }
    // A origem local é um caminho absoluto gravado pela varredura; relativo não vale.
    assert_eq!(
        localizar(&cofre, &alvo(None, vec![(fora_absoluto.clone(), None)])),
        Some(Path::new(&fora_absoluto).to_path_buf())
    );
    assert_eq!(
        localizar(&cofre, &alvo(None, vec![("fora.pdf".to_owned(), None)])),
        None
    );
}

#[test]
fn qpdf_so_recebe_caminho_absoluto() {
    let candidata = Candidata::nova(Zeroizing::new("senha-ficticia".to_owned()));
    let desfecho = extrair_pelo_qpdf(
        Path::new("--show-encryption"),
        &candidata,
        &Ferramentas::default(),
    );
    assert_eq!(
        desfecho,
        Desfecho::FalhaNaExtracao(FalhaDeFerramenta::NaoEncontrada)
    );
}

struct Ficticia;
impl FonteDeSegredos for Ficticia {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        Ok(Zeroizing::new(CPF.to_owned()))
    }
    fn senha_propria(&self, _rotulo: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        Err(ErroDeFonte::SemSenhaPropria)
    }
}

#[test]
fn estrategia_fora_da_faixa_da_erro_sem_panico() {
    for estrategia in [
        Estrategia::CpfPrimeiros(0),
        Estrategia::CpfPrimeiros(12),
        Estrategia::CpfUltimos(0),
        Estrategia::CpfUltimos(12),
        Estrategia::CpfUltimos(255),
    ] {
        let resultado = std::panic::catch_unwind(|| estrategia.candidata(&Ficticia).err());
        assert_eq!(
            resultado.expect("entrou em pânico"),
            Some(ErroDeFonte::EstrategiaInvalida),
            "{estrategia}"
        );
    }
    assert_eq!(
        Estrategia::CpfUltimos(11)
            .candidata(&Ficticia)
            .expect("11")
            .expor(),
        CPF
    );
}
