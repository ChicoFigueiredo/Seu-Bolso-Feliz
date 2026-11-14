//! O modo `sbf mcp --inventario <pasta-do-cofre> [--livro <livro-caixa.sqlite>]`: o servidor
//! MCP local por stdio (ADR-012, ADR-015 item 4). O stdout é só do protocolo; mensagens vão
//! para o stderr. O livro-caixa vem do cache local da estação, se existir (só leitura).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sbf_mcp::Nucleo;

const USO: &str = "uso: sbf mcp --inventario <pasta-do-cofre> [--livro <livro-caixa.sqlite>]";

/// O cache do livro mais recente em `<pasta de dados>/livro-caixa/*/livro-caixa.sqlite`.
fn livro_da_estacao() -> Option<PathBuf> {
    let pasta = sbf_estacao::pasta_de_dados_padrao().ok()?;
    let raiz = pasta.join(sbf_estacao::SUBPASTA_DO_LIVRO_CAIXA);
    std::fs::read_dir(raiz)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().join("livro-caixa.sqlite"))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max()
        .map(|(_, p)| p)
}

pub fn rodar(args: &[String]) -> ExitCode {
    let mut pasta: Option<&String> = None;
    let mut livro: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match (args[i].as_str(), args.get(i + 1)) {
            ("--inventario", Some(v)) => pasta = Some(v),
            ("--livro", Some(v)) => livro = Some(PathBuf::from(v)),
            _ => {
                eprintln!("sbf mcp: argumento inesperado.\n{USO}");
                return ExitCode::from(2);
            }
        }
        i += 2;
    }
    let Some(pasta) = pasta else {
        eprintln!("sbf mcp: falta --inventario.\n{USO}");
        return ExitCode::from(2);
    };
    let livro = livro.or_else(livro_da_estacao);
    let nucleo = match Nucleo::novo(
        &Path::new(pasta).join("inventario.sqlite"),
        livro.as_deref(),
    ) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("sbf mcp: {e}");
            return ExitCode::from(3);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sbf mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(sbf_mcp::servir_stdio(nucleo)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf mcp: {e}");
            ExitCode::FAILURE
        }
    }
}
