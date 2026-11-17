//! Processo filho dos scripts: endurece o processo e serve o protocolo em stdin/stdout.
//! `--autoteste` tenta o que o isolamento deve impedir e imprime só "permitido"/"negado".

use std::io::{self, BufReader};
use std::process::ExitCode;

fn autoteste() {
    let relatorio = sbf_script::endurecer();
    println!("landlock {:?}", relatorio.landlock);
    println!("rlimits {:?}", relatorio.rlimits);
    let rotulo = |ok: bool| if ok { "permitido" } else { "negado" };
    println!(
        "abrir_arquivo {}",
        rotulo(std::fs::File::open("/etc/hostname").is_ok())
    );
    println!(
        "listar_diretorio {}",
        rotulo(std::fs::read_dir("/").is_ok())
    );
    println!(
        "criar_arquivo {}",
        rotulo(std::fs::File::create("sbf-script-autoteste.tmp").is_ok())
    );
    println!(
        "rede {}",
        rotulo(
            std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], 9)),
                std::time::Duration::from_millis(200)
            )
            .is_ok()
        )
    );
    println!(
        "novo_processo {}",
        rotulo(std::process::Command::new("/bin/true").spawn().is_ok())
    );
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--autoteste") {
        autoteste();
        return ExitCode::SUCCESS;
    }
    match sbf_script::servir_endurecido(BufReader::new(io::stdin().lock()), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
