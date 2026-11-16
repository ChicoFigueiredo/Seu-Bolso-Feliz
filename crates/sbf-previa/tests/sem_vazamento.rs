//! A prévia de PDF (comum e protegido) e de imagem não escreve nada em disco (conferido com
//! `strace -f`, que segue o `pdfinfo` e o `pdftoppm`), e a senha candidata não aparece no
//! argv nem no ambiente de nenhum processo (conferido por `/proc`).

mod comum;

use comum::*;
use sbf_previa::Conteudo;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn contem(palheiro: &[u8], agulha: &[u8]) -> bool {
    palheiro.windows(agulha.len()).any(|j| j == agulha)
}

#[derive(Default)]
struct Visto {
    vazamentos: Vec<(String, &'static str)>,
    viu_pdftoppm: bool,
}

/// Varre todo `/proc/<pid>/cmdline` e `/proc/<pid>/environ` legível até mandarem parar.
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
                        if arquivo == "cmdline" && contem(&bytes, b"pdftoppm\0-f\0") {
                            visto.viu_pdftoppm = true;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
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

#[test]
fn a_senha_nao_aparece_em_argv_nem_ambiente_de_nenhum_processo() {
    let pasta = pasta();
    let caminho = cifrar(pasta.path(), "protegido", &pdf_ficticio(3), SENHA);

    // Controle positivo: um processo com a senha no argv é visto pelo vigia.
    let vigia = Vigia::iniciar(SENHA);
    std::process::Command::new("sh")
        .args(["-c", "sleep 0.3", SENHA])
        .status()
        .expect("sh");
    assert!(
        !vigia.parar().vazamentos.is_empty(),
        "o vigia não enxerga argv: o método não vale"
    );

    let vigia = Vigia::iniciar(SENHA);
    let conteudo = abrir(&caminho, vec![candidata("errada"), candidata(SENHA)]);
    let Conteudo::Paginas(p) = &conteudo else {
        panic!("{conteudo:?}")
    };
    // Desenha várias vezes, grande, para o vigia pegar o pdftoppm vivo.
    for _ in 0..5 {
        for n in 1..=3 {
            p.desenhar(n, 2400).expect("desenhar");
        }
    }
    let visto = vigia.parar();
    assert!(visto.viu_pdftoppm, "o vigia não pegou o pdftoppm vivo");
    assert!(
        visto.vazamentos.is_empty(),
        "senha visível em {:?}",
        visto.vazamentos
    );
}

/// Roda sob `strace` (pelo teste de fora): abre os arquivos de `SBF_PREVIA_ALVO`.
#[test]
#[ignore = "só roda sob strace, chamado por nenhum_arquivo_e_escrito_na_previa"]
fn interno_abre_sob_strace() {
    let Some(alvo) = std::env::var_os("SBF_PREVIA_ALVO") else {
        return;
    };
    let alvo = Path::new(&alvo);
    let comum = abrir(&alvo.join("comum.pdf"), vec![]);
    let Conteudo::Paginas(p) = &comum else {
        panic!("{comum:?}")
    };
    p.desenhar(2, 800).expect("desenhar");
    let protegido = abrir(&alvo.join("protegido.pdf"), vec![candidata(SENHA)]);
    let Conteudo::Paginas(p) = &protegido else {
        panic!("{protegido:?}")
    };
    p.desenhar(3, 800).expect("desenhar");
    sbf_previa::miniatura(&protegido, 220).expect("miniatura");
    let imagem = abrir(&alvo.join("foto.jpg"), vec![]);
    assert!(matches!(imagem, Conteudo::Imagem(_)));
    sbf_previa::miniatura(&imagem, 220).expect("miniatura");
    let heic = alvo.join("foto.heic");
    if heic.exists() {
        assert!(matches!(abrir(&heic, vec![]), Conteudo::Imagem(_)));
    }
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
                "mkdir",
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
fn nenhum_arquivo_e_escrito_na_previa_de_pdf_e_imagem() {
    if !tem_strace() {
        eprintln!("UNVERIFIED: strace não está instalado; a prévia sem disco fica sem conferência");
        return;
    }
    let pasta = pasta();
    let alvo = pasta.path().join("alvo");
    std::fs::create_dir(&alvo).expect("alvo");
    std::fs::write(alvo.join("comum.pdf"), pdf_ficticio(3)).expect("comum");
    cifrar(&alvo, "protegido", &pdf_ficticio(3), SENHA);
    gravar_imagem(&alvo.join("foto.jpg"), 1200, 800);
    // HEIC (um AVIF, mesmo contêiner) pelo ffmpeg, se ele grava AVIF aqui.
    let _ = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(alvo.join("foto.jpg"))
        .args([
            "-frames:v",
            "1",
            "-c:v",
            "libaom-av1",
            "-still-picture",
            "1",
            "-f",
            "avif",
        ])
        .arg(alvo.join("foto.heic"))
        .status();
    let mut antes: Vec<String> = std::fs::read_dir(&alvo)
        .expect("listar")
        .map(|e| e.expect("e").file_name().to_string_lossy().into_owned())
        .collect();
    antes.sort();

    let log = pasta.path().join("strace.log");
    let exe = std::env::current_exe().expect("exe");
    let status = std::process::Command::new("strace")
        .args([
            "-f",
            "-qq",
            "-e",
            "trace=execve,open,openat,creat,memfd_create,rename,renameat,renameat2,unlink,unlinkat,link,linkat,mkdir,mkdirat",
            "-o",
        ])
        .arg(&log)
        .arg(&exe)
        .args(["--exact", "interno_abre_sob_strace", "--ignored", "--test-threads=1", "-q"])
        .env("SBF_PREVIA_ALVO", &alvo)
        .status()
        .expect("strace");
    assert!(status.success(), "o teste interno falhou sob strace");
    let texto = std::fs::read_to_string(&log).expect("log do strace");
    assert!(texto.contains("pdftoppm"), "o strace não seguiu o pdftoppm");
    let achadas = escritas(&texto);
    assert!(
        achadas.is_empty(),
        "escrita em disco durante a prévia:\n{}",
        achadas.join("\n")
    );
    let mut depois: Vec<String> = std::fs::read_dir(&alvo)
        .expect("listar")
        .map(|e| e.expect("e").file_name().to_string_lossy().into_owned())
        .collect();
    depois.sort();
    assert_eq!(antes, depois, "apareceu arquivo ao lado do original");
    if let Some(destino) = std::env::var_os("SBF_PREVIA_GUARDAR_STRACE") {
        let _ = std::fs::copy(&log, Path::new(&destino).join("strace-previa.log"));
    }
}

/// O crate não loga nem imprime: nenhum caminho para a senha ou o conteúdo sair por aí.
#[test]
fn o_crate_nao_loga() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entrada in std::fs::read_dir(src).expect("src") {
        let caminho = entrada.expect("entrada").path();
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

/// Roda sob `strace` (pelo teste de fora): converte o rtf de `SBF_PREVIA_ALVO`.
#[test]
#[ignore = "só roda sob strace, chamado por office_so_escreve_na_pasta_em_memoria"]
fn interno_converte_sob_strace() {
    let Some(alvo) = std::env::var_os("SBF_PREVIA_ALVO") else {
        return;
    };
    let conteudo = abrir(&Path::new(&alvo).join("carta.rtf"), vec![]);
    let Conteudo::Paginas(p) = &conteudo else {
        panic!("{conteudo:?}")
    };
    p.desenhar(1, 400).expect("desenhar");
}

/// O LibreOffice só escreve na pasta da conversão em `$XDG_RUNTIME_DIR` (tmpfs) e em
/// `/dev/shm` (memória compartilhada); nada em `$HOME`, `/tmp` ou ao lado do original. A
/// exceção é o socket de instância única (`/tmp/OSL_PIPE_<uid>_SingleOfficeIPC_<hash>`), que o
/// LibreOffice cria sempre em `/tmp`: não guarda conteúdo.
#[test]
fn office_so_escreve_na_pasta_em_memoria() {
    let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") else {
        eprintln!("UNVERIFIED: sem XDG_RUNTIME_DIR");
        return;
    };
    if !tem_strace() || sbf_processo::localizar("soffice").is_none() {
        eprintln!("UNVERIFIED: sem strace ou sem soffice");
        return;
    }
    let pasta = pasta();
    let alvo = pasta.path().join("alvo");
    std::fs::create_dir(&alvo).expect("alvo");
    std::fs::write(
        alvo.join("carta.rtf"),
        r"{\rtf1\ansi{\fonttbl\f0\fswiss Helvetica;}\f0\pard Carta ficticia.\par}",
    )
    .expect("rtf");
    let log = pasta.path().join("strace-office.log");
    let exe = std::env::current_exe().expect("exe");
    let status = std::process::Command::new("strace")
        .args([
            "-f",
            "-qq",
            // -y: o caminho de cada descritor (o `unlinkat(fd, ...)` da faxina da pasta).
            "-y",
            "-e",
            "trace=execve,open,openat,creat,rename,renameat,renameat2,unlink,unlinkat,link,linkat,mkdir,mkdirat",
            "-o",
        ])
        .arg(&log)
        .arg(&exe)
        .args(["--exact", "interno_converte_sob_strace", "--ignored", "--test-threads=1", "-q"])
        .env("SBF_PREVIA_ALVO", &alvo)
        .status()
        .expect("strace");
    assert!(status.success(), "o teste interno falhou sob strace");
    let texto = std::fs::read_to_string(&log).expect("log do strace");
    assert!(texto.contains("soffice"), "o strace não seguiu o soffice");
    let xdg_texto = Path::new(&xdg).display().to_string();
    let permitida = format!("\"{xdg_texto}/sbf-previa");
    let permitida_fd = format!("<{xdg_texto}/sbf-previa");
    let fora: Vec<String> = escritas(&texto)
        .into_iter()
        .filter(|l| {
            !l.contains(&permitida) && !l.contains(&permitida_fd) && !l.contains("\"/dev/shm/")
        })
        // O socket de instância única do LibreOffice (`bind` em `/tmp`, fixo no código dele,
        // ignora o TMPDIR): um nome de socket, sem conteúdo; o nome é um hash do perfil.
        .filter(|l| !l.contains("\"/tmp/OSL_PIPE_"))
        // Falhou (não criou nada), como um mkdir de pasta que já existe.
        .filter(|l| !l.contains("= -1 "))
        .collect();
    if let Some(destino) = std::env::var_os("SBF_PREVIA_GUARDAR_STRACE") {
        let _ = std::fs::copy(&log, Path::new(&destino).join("strace-office.log"));
    }
    assert!(
        fora.is_empty(),
        "escrita fora da pasta em memória:\n{}",
        fora.join("\n")
    );
    let nomes: Vec<_> = std::fs::read_dir(&alvo).expect("ler").flatten().collect();
    assert_eq!(nomes.len(), 1, "{nomes:?}");
}
