//! O modo `sbf mcp --inventario <pasta-do-cofre> [--livro <livro-caixa.sqlite>]`: o servidor
//! MCP local por stdio (ADR-012, ADR-015 item 4). O stdout é só do protocolo; mensagens vão
//! para o stderr. O livro-caixa vem do cache local da estação, se existir (só leitura).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sbf_mcp::Nucleo;

const USO: &str = "uso: sbf mcp --inventario <pasta-do-cofre> [--livro <livro-caixa.sqlite>] [--planejamento <planejamento.sqlite>]";

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

/// Abre o serviço de planejamento (spec 11) no arquivo dado. Ponto único de ligação: o
/// MCP só conhece o trait `Planejamento`; quando o serviço real for integrado ao binário,
/// é aqui que ele entra.
fn abrir_planejamento(arquivo: &Path) -> Result<Box<dyn sbf_planejamento::Planejamento>, String> {
    let banco = sbf_planejamento::Banco::abrir(arquivo).map_err(|e| e.to_string())?;
    let cofre = arquivo.parent().unwrap_or(Path::new("."));
    Ok(Box::new(sbf_planejamento::PlanejamentoSqlite::novo(
        banco,
        saldos_do_cofre(cofre),
    )))
}

/// Os saldos da Pluggy guardados no cofre; sem eles, nenhum saldo (o resumo marca a lacuna).
pub fn saldos_do_cofre(cofre: &Path) -> Box<dyn sbf_planejamento::FonteDeSaldos> {
    match sbf_planilha::saldos::SaldosDaPluggy::ler(cofre) {
        Ok(s) => Box::new(s),
        Err(_) => Box::new(sbf_planejamento::SemSaldos),
    }
}

pub fn rodar(args: &[String]) -> ExitCode {
    let mut pasta: Option<&String> = None;
    let mut livro: Option<PathBuf> = None;
    let mut planejamento: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match (args[i].as_str(), args.get(i + 1)) {
            ("--inventario", Some(v)) => pasta = Some(v),
            ("--livro", Some(v)) => livro = Some(PathBuf::from(v)),
            ("--planejamento", Some(v)) => planejamento = Some(PathBuf::from(v)),
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
    // O índice local só liga se o modelo já está baixado e o worker já indexou (o MCP nunca
    // baixa modelo). O cliente MCP nasce mascarado (ADR-015).
    let nucleo = match sbf_chaveiro::pasta_de_dados_do_app()
        .ok()
        .and_then(|p| crate::rag::busca_local(&p))
    {
        Some(b) => nucleo.com_busca(std::sync::Arc::new(b), sbf_ia::NivelDeConfianca::Mascarado),
        None => nucleo,
    };
    // As ferramentas do controle mensal só ligam se há planejamento (`--planejamento` ou
    // `<cofre>/planejamento.sqlite`).
    let planejamento = planejamento
        .or_else(|| Some(Path::new(pasta).join("planejamento.sqlite")).filter(|p| p.is_file()));
    let nucleo = match planejamento {
        Some(arquivo) if arquivo.is_file() => {
            nucleo.com_planejamento(std::sync::Arc::new(move || abrir_planejamento(&arquivo)))
        }
        Some(arquivo) => {
            eprintln!(
                "sbf mcp: não achei {} (controle mensal desligado)",
                arquivo.display()
            );
            nucleo
        }
        None => nucleo,
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
