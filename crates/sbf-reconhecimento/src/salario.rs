//! Achar o recorrente do salário e comparar o contracheque com o que está no planejamento.
//!
//! A regra é a do `sbf planejamento contracheque` (que usa este módulo): o recorrente é o
//! único de natureza Entrada cujo nome normalizado contém "salario"; zero ou mais de um
//! pede que a pessoa diga qual. Só lê o planejamento.

use sbf_planejamento::{
    AlvoDoDocumento, Centavos, Competencia, Composicao, Natureza, Planejamento, Recorrente,
};

/// Por que não saiu um recorrente do salário.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDoSalario {
    /// O `--recorrente` pedido não existe.
    #[error("o recorrente não existe")]
    Inexistente,
    /// Nenhum recorrente de entrada com "salario" no nome.
    #[error("não achei o recorrente do salário (use --recorrente)")]
    Nenhum,
    /// Mais de um.
    #[error("há mais de um recorrente de salário (use --recorrente)")]
    Varios,
    /// Falha do planejamento.
    #[error("{0}")]
    Planejamento(String),
}

fn falha(e: impl std::fmt::Display) -> ErroDoSalario {
    ErroDoSalario::Planejamento(e.to_string())
}

/// O recorrente pedido (`id`) ou, sem `id`, o único recorrente do salário.
///
/// # Errors
/// [`ErroDoSalario`].
pub fn recorrente_do_salario(
    plan: &dyn Planejamento,
    id: Option<&str>,
) -> Result<Recorrente, ErroDoSalario> {
    if let Some(id) = id {
        return plan
            .recorrente(id)
            .map_err(falha)?
            .ok_or(ErroDoSalario::Inexistente);
    }
    let mut candidatos: Vec<_> = plan
        .recorrentes(false)
        .map_err(falha)?
        .into_iter()
        .filter(|r| {
            r.natureza == Natureza::Entrada
                && sbf_planilha::texto::normalizar(&r.nome).contains("salario")
        })
        .collect();
    match candidatos.len() {
        1 => Ok(candidatos.remove(0)),
        0 => Err(ErroDoSalario::Nenhum),
        _ => Err(ErroDoSalario::Varios),
    }
}

/// O valor digitado do recorrente na competência: o da última versão que já valia, ou o da
/// primeira se nenhuma valia ainda.
#[must_use]
pub fn valor_digitado(recorrente: &Recorrente, em: Competencia) -> Option<Centavos> {
    recorrente
        .versoes
        .iter()
        .rev()
        .find(|v| v.desde <= em)
        .or_else(|| recorrente.versoes.first())
        .and_then(|v| v.valor)
}

/// A composição vigente em `composicao.desde` já é exatamente esta (mesmo início e mesmas
/// rubricas)?
///
/// # Errors
/// Falha do planejamento.
pub fn composicao_ja_vigente(
    plan: &dyn Planejamento,
    composicao: &Composicao,
) -> Result<bool, ErroDoSalario> {
    Ok(plan
        .composicao(&composicao.recorrente, composicao.desde)
        .map_err(falha)?
        .is_some_and(|a| a.desde == composicao.desde && a.rubricas == composicao.rubricas))
}

/// O documento (pelo sha256) já está ligado a este alvo?
///
/// # Errors
/// Falha do planejamento.
pub fn ja_vinculado(
    plan: &dyn Planejamento,
    alvo: &AlvoDoDocumento,
    sha256: &str,
) -> Result<bool, ErroDoSalario> {
    Ok(plan
        .documentos(alvo)
        .map_err(falha)?
        .iter()
        .any(|d| d.sha256 == sha256))
}
