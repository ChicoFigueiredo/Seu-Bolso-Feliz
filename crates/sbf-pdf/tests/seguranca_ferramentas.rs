//! Achados A6 e A7 da revisão de segurança da fatia 1: o `pdftotext` e o `qpdf` são
//! resolvidos por caminho absoluto nas pastas do sistema, nunca pelo `PATH`; um executável
//! em pasta gravável por outros é recusado; e o filho não herda o ambiente do app.
//! PDF fictício, gerado aqui.

#![cfg(unix)]

mod comum;

use comum::*;
use sbf_pdf::ferramenta::{PASTAS_DO_SISTEMA, confiavel, localizar};
use sbf_pdf::{Desfecho, FalhaDeFerramenta, Ferramentas, PdfProtegido};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Script que registra que rodou (e o ambiente que recebeu) e então chama a ferramenta real.
fn embrulho(pasta: &Path, ferramenta: &str) -> (PathBuf, PathBuf) {
    let script = pasta.join(format!("{ferramenta}-embrulho"));
    let registro = pasta.join(format!("{ferramenta}-ambiente.txt"));
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n/usr/bin/env > '{}'\nexec /usr/bin/{ferramenta} \"$@\"\n",
            registro.display()
        ),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    (script, registro)
}

fn pdf_comum(pasta: &Path) -> PdfProtegido {
    let caminho = pasta.join("comum.pdf");
    std::fs::write(&caminho, pdf_ficticio()).expect("pdf");
    PdfProtegido::ler(&caminho).expect("ler")
}

#[test]
fn padrao_e_caminho_absoluto_numa_pasta_do_sistema() {
    let f = Ferramentas::default();
    let qpdf = f.qpdf.clone().expect("qpdf");
    for caminho in [&f.pdftotext, &qpdf] {
        assert!(caminho.is_absolute(), "{caminho:?}");
        assert!(
            PASTAS_DO_SISTEMA.iter().any(|p| caminho.starts_with(p)),
            "{caminho:?}"
        );
    }
    assert_eq!(localizar("nao-existe-sbf-ficticio"), None);
}

/// Roda o teste interno com um `PATH` que tem um `pdftotext` e um `qpdf` falsos na frente.
#[test]
fn path_do_usuario_nao_e_consultado() {
    let pasta = pasta();
    let falsa = pasta.path().join("bin-falso");
    std::fs::create_dir(&falsa).expect("pasta");
    let marca = pasta.path().join("falso-rodou");
    for nome in ["pdftotext", "qpdf"] {
        let p = falsa.join(nome);
        std::fs::write(&p, format!("#!/bin/sh\ntouch '{}'\n", marca.display())).expect("falso");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let path = format!(
        "{}:{}",
        falsa.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let status = std::process::Command::new(std::env::current_exe().expect("exe"))
        .args(["--exact", "interno_com_path_falso", "--ignored", "-q"])
        .env("PATH", path)
        .env("SBF_TESTE_PASTA_FALSA", &falsa)
        .env("SBF_TESTE_PASTA", pasta.path())
        .status()
        .expect("teste interno");
    assert!(status.success(), "o teste interno falhou");
    assert!(!marca.exists(), "o executável falso do PATH rodou");
}

#[test]
#[ignore = "chamado por path_do_usuario_nao_e_consultado"]
fn interno_com_path_falso() {
    let Some(falsa) = std::env::var_os("SBF_TESTE_PASTA_FALSA").map(PathBuf::from) else {
        return;
    };
    let pasta = PathBuf::from(std::env::var_os("SBF_TESTE_PASTA").expect("pasta"));
    let f = Ferramentas::default();
    assert!(!f.pdftotext.starts_with(&falsa), "{:?}", f.pdftotext);
    assert!(!f.qpdf.as_deref().expect("qpdf").starts_with(&falsa));
    assert!(matches!(
        pdf_comum(&pasta).abrir_sem_senha(&f),
        Desfecho::Aberto(_)
    ));
}

#[test]
fn ferramenta_em_pasta_gravavel_por_outros_e_recusada() {
    let pasta = pasta();
    let aberta = pasta.path().join("aberta");
    std::fs::create_dir(&aberta).expect("pasta");
    let (script, registro) = embrulho(&aberta, "pdftotext");
    std::fs::set_permissions(&aberta, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    assert!(!confiavel(&script));
    let f = Ferramentas {
        pdftotext: script,
        ..Ferramentas::default()
    };
    assert_eq!(
        pdf_comum(pasta.path()).abrir_sem_senha(&f),
        Desfecho::FalhaNaExtracao(FalhaDeFerramenta::NaoEncontrada)
    );
    assert!(!registro.exists(), "o executável inseguro rodou");

    // O próprio arquivo gravável por outros também não vale.
    std::fs::set_permissions(&aberta, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let (script, _) = embrulho(&aberta, "qpdf");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    assert!(!confiavel(&script));
}

#[test]
fn caminho_relativo_configurado_e_recusado() {
    let pasta = pasta();
    let f = Ferramentas {
        pdftotext: PathBuf::from("pdftotext"),
        ..Ferramentas::default()
    };
    assert!(!confiavel(Path::new("pdftotext")));
    assert_eq!(
        pdf_comum(pasta.path()).abrir_sem_senha(&f),
        Desfecho::FalhaNaExtracao(FalhaDeFerramenta::NaoEncontrada)
    );
}

#[test]
fn filho_recebe_so_o_ambiente_minimo() {
    let pasta = pasta();
    let (script, registro) = embrulho(pasta.path(), "pdftotext");
    assert!(confiavel(&script), "{script:?}");
    let f = Ferramentas {
        pdftotext: script,
        ..Ferramentas::default()
    };
    let desfecho = pdf_comum(pasta.path()).abrir_sem_senha(&f);
    assert!(matches!(desfecho, Desfecho::Aberto(_)), "{desfecho:?}");
    let ambiente = std::fs::read_to_string(&registro).expect("o embrulho rodou");
    let nomes: BTreeSet<&str> = ambiente
        .lines()
        .filter_map(|l| l.split_once('=').map(|(n, _)| n))
        .collect();
    // O que o `sh` acrescenta sozinho (PWD, SHLVL, _) e o que o app passa.
    let permitidos: BTreeSet<&str> = [
        "PATH", "LANG", "LC_ALL", "LC_CTYPE", "PWD", "SHLVL", "_", "OLDPWD",
    ]
    .into_iter()
    .collect();
    assert!(
        nomes.is_subset(&permitidos),
        "variáveis a mais: {:?}",
        nomes.difference(&permitidos).collect::<Vec<_>>()
    );
    assert!(ambiente.contains("PATH=/usr/bin:/bin\n"), "{ambiente}");
}
