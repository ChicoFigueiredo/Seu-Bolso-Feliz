//! Achar o recorrente do salário e comparar o contracheque com o que está no planejamento.
//!
//! A regra é a do `sbf planejamento contracheque` (que usa este módulo): o recorrente é o
//! único de natureza Entrada cujo nome normalizado contém "salario"; zero ou mais de um
//! pede que a pessoa diga qual. Só lê o planejamento.

use sbf_planejamento::{
    AlvoDoDocumento, Autor, Centavos, Competencia, Composicao, Natureza, Planejamento, Recorrente,
    Rubrica,
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
/// rubricas)? As ligações das rubricas aos compromissos de desconto em folha não contam: ligar
/// uma rubrica não muda o que o contracheque diz.
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
        .is_some_and(|a| {
            let sem_ligacao = |rubricas: &[Rubrica]| -> Vec<Rubrica> {
                rubricas
                    .iter()
                    .map(|r| Rubrica {
                        recorrente: None,
                        ..r.clone()
                    })
                    .collect()
            };
            a.desde == composicao.desde
                && sem_ligacao(&a.rubricas) == sem_ligacao(&composicao.rubricas)
        }))
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

/// O documento está ligado a este alvo por uma regra que agiu sozinha (`Autor::Regra`) e o
/// usuário não o desfez: é o vínculo que o cartão mostra como "Vinculado automaticamente".
///
/// # Errors
/// Falha do planejamento.
pub fn vinculado_por_regra(
    plan: &dyn Planejamento,
    alvo: &AlvoDoDocumento,
    sha256: &str,
) -> Result<bool, ErroDoSalario> {
    Ok(plan.documentos(alvo).map_err(falha)?.iter().any(|d| {
        d.sha256 == sha256
            && d.alvo == *alvo
            && !d.desvinculado
            && matches!(d.autor, Autor::Regra { .. })
    }))
}

/// O usuário já desvinculou este documento deste alvo alguma vez: a regra não o refaz.
///
/// # Errors
/// Falha do planejamento.
pub fn vinculo_desfeito(
    plan: &dyn Planejamento,
    alvo: &AlvoDoDocumento,
    sha256: &str,
) -> Result<bool, ErroDoSalario> {
    plan.documento_desvinculado(alvo, sha256).map_err(falha)
}

/// A ocorrência da competência existe para o recorrente (o contracheque de outubro tem o
/// Salário de 10/2026 onde ligar).
///
/// # Errors
/// Falha do planejamento.
pub fn ocorrencia_existe(
    plan: &dyn Planejamento,
    recorrente: &str,
    competencia: Competencia,
    hoje: sbf_planejamento::Data,
) -> Result<bool, ErroDoSalario> {
    Ok(plan
        .proximas(recorrente, competencia, 1, hoje)
        .map_err(falha)?
        .iter()
        .any(|o| o.id.competencia == competencia))
}
