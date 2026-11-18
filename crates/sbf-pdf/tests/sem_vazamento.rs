//! INV-09, a parte de segurança: a senha não aparece no argv nem no ambiente de nenhum
//! processo durante a abertura (conferido por `/proc`), e nenhum arquivo é criado ou aberto
//! para escrita durante a abertura (conferido com `strace`).

mod comum;

use comum::*;
use sbf_pdf::{Candidata, Desfecho, Ferramentas, PdfProtegido, Via};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zeroize::Zeroizing;

fn candidata(valor: &str) -> Candidata {
    Candidata::nova(Zeroizing::new(valor.to_owned()))
}

fn contem(palheiro: &[u8], agulha: &[u8]) -> bool {
    palheiro.windows(agulha.len()).any(|j| j == agulha)
}

#[derive(Default)]
struct Visto {
    /// (pid, arquivo) onde a senha apareceu.
    vazamentos: Vec<(String, &'static str)>,
    /// Viu um processo com estes argumentos vivo.
    viu_pdftotext: bool,
    viu_qpdf: bool,
}

/// Varre todo `/proc/<pid>/cmdline` e `/proc/<pid>/environ` legível, sem parar, até mandarem
/// parar.
struct Vigia {
    parar: Arc<AtomicBool>,
    visto: Arc<Mutex<Visto>>,
    linha: Option<std::thread::JoinHandle<()>>,
}

impl Vigia {
    fn iniciar(senha: &str) -> Self {
        let parar = Arc::new(AtomicBool::new(false));
        let visto = Arc::new(Mutex::new(Visto::default()));
        let senha = senha.as_bytes().to_vec();
        let (p, v) = (parar.clone(), visto.clone());
        let linha = std::thread::spawn(move || {
            while !p.load(Ordering::SeqCst) {
                let Ok(entradas) = std::fs::read_dir("/proc") else {
                    return;
                };
                for entrada in entradas.flatten() {
                    let nome = entrada.file_name().to_string_lossy().into_owned();
                    if !nome.bytes().all(|b| b.is_ascii_digit()) {
                        continue;
                    }
                    for arquivo in ["cmdline", "environ"] {
                        let Ok(bytes) = std::fs::read(entrada.path().join(arquivo)) else {
                            continue;
                        };
                        let mut visto = v.lock().expect("trava");
                        if contem(&bytes, &senha) {
                            visto.vazamentos.push((nome.clone(), arquivo));
                        }
                        if arquivo == "cmdline" {
                            if contem(&bytes, b"\x00-layout\x00-l\x003\x00-\x00-\x00") {
                                visto.viu_pdftotext = true;
                            }
                            if contem(&bytes, b"--password-file=-\0--decrypt") {
                                visto.viu_qpdf = true;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        Self {
            parar,
            visto,
            linha: Some(linha),
        }
    }

    fn parar(mut self) -> Visto {
        self.parar.store(true, Ordering::SeqCst);
        if let Some(l) = self.linha.take() {
            l.join().expect("vigia");
        }
        std::mem::take(&mut *self.visto.lock().expect("trava"))
    }
}

/// Um script que espera um pouco antes de virar a ferramenta (exec, mesmo pid), para o vigia
/// pegar o filho vivo com certeza.
fn lento(pasta: &Path, ferramenta: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let caminho = pasta.join(format!("{ferramenta}-lento"));
    std::fs::write(
        &caminho,
        format!("#!/bin/sh\nsleep 0.3\nexec {ferramenta} \"$@\"\n"),
    )
    .expect("script");
    std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    caminho
}

/// Senha fictícia só dos testes sob `strace`. Eles rodam em paralelo com o vigia de argv, e
/// o preparo do PDF cifrado chama o `qpdf` com a senha nos argumentos: com a mesma senha, o
/// vigia flagraria o preparo de outro teste (corrida vista na CI em 2026-11-18).
const SENHA_DO_STRACE: &str = "senha-ficticia-Strace-17";

#[test]
fn a_senha_nao_aparece_em_argv_nem_ambiente_de_nenhum_processo() {
    let pasta = pasta();
    let caminho = cifrar(
        pasta.path(),
        "fatura",
        &pdf_ficticio(),
        SENHA_PROPRIA,
        Cifra::Aes256,
    );

    // Controle positivo: um processo com a senha no argv é visto pelo vigia.
    let vigia = Vigia::iniciar(SENHA_PROPRIA);
    let mut controle = std::process::Command::new("sh")
        .args(["-c", "sleep 0.3", SENHA_PROPRIA])
        .spawn()
        .expect("sh");
    controle.wait().expect("controle");
    let visto = vigia.parar();
    assert!(
        !visto.vazamentos.is_empty(),
        "o vigia não enxerga argv: o método não vale"
    );

    let ferramentas = Ferramentas {
        pdftotext: lento(pasta.path(), "pdftotext"),
        qpdf: Some(lento(pasta.path(), "qpdf")),
        limite_de_tempo: Duration::from_secs(30),
    };
    let vigia = Vigia::iniciar(SENHA_PROPRIA);
    let pdf = PdfProtegido::ler(&caminho).expect("ler");
    match pdf.tentar(&candidata(SENHA_PROPRIA), &ferramentas) {
        Desfecho::Aberto(t) => assert_eq!(t.via, Via::Lopdf),
        outro => panic!("{outro:?}"),
    }
    match sbf_pdf::extrair_pelo_qpdf(&caminho, &candidata(SENHA_PROPRIA), &ferramentas) {
        Desfecho::Aberto(t) => assert_eq!(t.via, Via::Qpdf),
        outro => panic!("{outro:?}"),
    }
    let visto = vigia.parar();
    assert!(visto.viu_pdftotext, "o vigia não pegou o pdftotext vivo");
    assert!(visto.viu_qpdf, "o vigia não pegou o qpdf vivo");
    assert!(
        visto.vazamentos.is_empty(),
        "senha visível em {:?}",
        visto.vazamentos
    );
}

/// Roda sob `strace` (pelo teste de fora): abre o PDF do `SBF_PDF_ALVO` e nada mais.
#[test]
#[ignore = "só roda sob strace, chamado por nenhum_arquivo_e_criado_na_abertura"]
fn interno_abre_sob_strace() {
    let Some(alvo) = std::env::var_os("SBF_PDF_ALVO") else {
        return;
    };
    let ferramentas = Ferramentas {
        qpdf: std::env::var_os("SBF_PDF_QPDF").and_then(|_| Ferramentas::default().qpdf),
        ..Ferramentas::default()
    };
    let pdf = PdfProtegido::ler(Path::new(&alvo)).expect("ler");
    let desfecho = if std::env::var_os("SBF_PDF_QPDF").is_some() {
        sbf_pdf::extrair_pelo_qpdf(Path::new(&alvo), &candidata(SENHA_DO_STRACE), &ferramentas)
    } else {
        pdf.tentar(&candidata(SENHA_DO_STRACE), &ferramentas)
    };
    assert!(matches!(desfecho, Desfecho::Aberto(_)), "{desfecho:?}");
}

fn tem_strace() -> bool {
    std::process::Command::new("strace")
        .arg("-V")
        .output()
        .is_ok_and(|s| s.status.success())
}

/// As linhas do `strace` que criam, escrevem, renomeiam ou apagam arquivo.
fn escritas(log: &str) -> Vec<String> {
    log.lines()
        .filter(|l| {
            let abre_para_escrever = (l.contains("openat(") || l.contains("open("))
                && ["O_WRONLY", "O_RDWR", "O_CREAT", "O_TRUNC", "O_TMPFILE"]
                    .iter()
                    .any(|f| l.contains(f));
            let outra = [
                "creat(",
                "memfd_create(",
                "rename",
                "unlink",
                "link(",
                "linkat(",
            ]
            .iter()
            .any(|s| l.contains(s));
            (abre_para_escrever || outra)
                && !l.contains("\"/dev/null\"")
                && !l.contains("\"/dev/tty\"")
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn nenhum_arquivo_e_criado_na_abertura() {
    if !tem_strace() {
        eprintln!("UNVERIFIED: strace não está instalado; o INV-09 (disco) fica sem conferência");
        return;
    }
    let pasta = pasta();
    let caminho = cifrar(
        pasta.path(),
        "fatura",
        &pdf_ficticio(),
        SENHA_DO_STRACE,
        Cifra::Aes256,
    );
    let original = std::fs::read(&caminho).expect("ler");
    let exe = std::env::current_exe().expect("exe");
    for (nome, com_qpdf) in [("lopdf", false), ("qpdf", true)] {
        let log = pasta.path().join(format!("strace-{nome}.log"));
        let mut c = std::process::Command::new("strace");
        c.args(["-f", "-qq", "-e", "trace=open,openat,creat,memfd_create,rename,renameat,renameat2,unlink,unlinkat,link,linkat", "-o"])
            .arg(&log)
            .arg(&exe)
            .args(["--exact", "interno_abre_sob_strace", "--ignored", "--test-threads=1", "-q"])
            .env("SBF_PDF_ALVO", &caminho);
        if com_qpdf {
            c.env("SBF_PDF_QPDF", "1");
        }
        let status = c.status().expect("strace");
        assert!(
            status.success(),
            "{nome}: o teste interno falhou sob strace"
        );
        let texto = std::fs::read_to_string(&log).expect("log do strace");
        assert!(
            texto.contains("pdftotext") || texto.contains("execve") || texto.contains("openat"),
            "{nome}: log vazio"
        );
        let achadas = escritas(&texto);
        assert!(
            achadas.is_empty(),
            "{nome}: escrita em disco durante a abertura:\n{}",
            achadas.join("\n")
        );
        // Nenhum arquivo novo apareceu na pasta do original, além do log.
        let mut nomes: Vec<String> = std::fs::read_dir(pasta.path())
            .expect("listar")
            .map(|e| {
                e.expect("entrada")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|n| !n.starts_with("strace-"))
            .collect();
        nomes.sort();
        assert_eq!(nomes, ["fatura-base.pdf", "fatura.pdf"], "{nome}");
    }
    assert_eq!(
        std::fs::read(&caminho).expect("ler"),
        original,
        "o original mudou"
    );
    // Para inspecionar os logs: SBF_PDF_GUARDAR_STRACE=<pasta>.
    if let Some(destino) = std::env::var_os("SBF_PDF_GUARDAR_STRACE") {
        for nome in ["strace-lopdf.log", "strace-qpdf.log"] {
            let _ = std::fs::copy(pasta.path().join(nome), Path::new(&destino).join(nome));
        }
    }
}

/// O crate não loga nem imprime: nenhum caminho para a senha sair por aí.
#[test]
fn o_crate_nao_loga() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entrada in std::fs::read_dir(src).expect("src") {
        let caminho = entrada.expect("entrada").path();
        if caminho.is_dir() {
            continue;
        }
        let texto = std::fs::read_to_string(&caminho).expect("ler");
        for proibido in [
            "println!",
            "eprintln!",
            "print!(",
            "eprint!(",
            "dbg!",
            "log::",
            "tracing::",
        ] {
            assert!(
                !texto.contains(proibido),
                "{proibido} em {}",
                caminho.display()
            );
        }
    }
}
