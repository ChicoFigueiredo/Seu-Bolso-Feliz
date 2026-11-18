//! `sbf script --servir`: o processo filho que executa os scripts Luau das regras
//! (spec 13 §4, ADR-021). É interno: fica fora da ajuda principal.
//!
//! O pai (o `sbf` da janela, do worker, do MCP ou de `sbf regras`) monta o executor com
//! [`executor_de_script`] e fala com este processo em linhas JSON pela entrada e saída
//! padrão. O processo se endurece (Landlock e rlimits, melhor esforço) antes de ler o
//! primeiro pedido.

use std::process::ExitCode;
use std::sync::Arc;

use sbf_regras::script::ExecutorDeScript;
use sbf_script::ExecutorFilho;

/// O executor de scripts que os modos do `sbf` usam: um processo filho deste mesmo binário
/// (`sbf script --servir`).
///
/// # Errors
/// Não saber o caminho do próprio executável.
pub fn executor_de_script() -> std::io::Result<Arc<dyn ExecutorDeScript>> {
    Ok(Arc::new(ExecutorFilho::novo(
        std::env::current_exe()?,
        vec!["script".to_owned(), "--servir".to_owned()],
    )))
}

/// Liga ao planejamento o executor do Cálculo dos Modelos de compromisso: o mesmo processo
/// filho dos scripts das regras (`sbf script --servir`), subido só no primeiro cálculo. Sem
/// como achar o próprio executável, o planejamento segue sem cálculo (o previsto cai no último
/// valor conhecido, com aviso).
pub fn com_calculo(
    plano: sbf_planejamento::PlanejamentoSqlite,
) -> sbf_planejamento::PlanejamentoSqlite {
    match executor_de_script() {
        Ok(executor) => plano.com_calculo(executor),
        Err(_) => plano,
    }
}

pub fn rodar(args: &[String]) -> ExitCode {
    if args != ["--servir"] {
        eprintln!("sbf: `sbf script --servir` é interno; veja `sbf regras --ajuda`.");
        return ExitCode::from(2);
    }
    let entrada = std::io::stdin().lock();
    let saida = std::io::stdout().lock();
    match sbf_script::servir_endurecido(entrada, saida) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sbf: o servidor de scripts parou: {e}");
            ExitCode::from(1)
        }
    }
}
