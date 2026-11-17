//! O salário no planejamento (spec 12, fase A): as três provas (D2) e a baixa dos descontos
//! em folha pelo contracheque (D3). Só junta o que o banco guarda com o extrato; as regras
//! de valor e conta são exatas, sem tolerância.
//!
//! Nenhum valor, nome ou conta vai a log.

use super::{PlanejamentoSqlite, banco};
use crate::conciliacao::{data_de_segundos, normalizar};
use crate::{
    BaixaPeloContracheque, Centavos, Competencia, Conciliacao, IdOcorrencia, MovimentoDoExtrato,
    Natureza, Planejamento as _, Resultado, TipoDeDocumento, TipoDeRubrica,
};

/// A regra que grava sozinha a conciliação das três provas.
pub const REGRA_TRES_PROVAS: &str = "tres-provas";

/// O documento, o crédito e a conta batem exatamente para esta ocorrência e este movimento?
///
/// 1. há um contracheque vinculado (à ocorrência ou ao recorrente) de que veio a composição
///    do mês (`Composicao::documento`, com `desde` na competência da ocorrência);
/// 2. o valor do crédito é exatamente o líquido dessa composição;
/// 3. a conta do crédito é a conta (par confirmado) da ocorrência.
pub(super) fn tres_provas(
    s: &PlanejamentoSqlite,
    id: &IdOcorrencia,
    movimento: &MovimentoDoExtrato,
) -> Resultado<bool> {
    if movimento.valor.0 <= 0 {
        return Ok(false);
    }
    let c = s.banco.conexao();
    let Some(composicao) = banco::ler_composicao(c, &id.recorrente, id.competencia)? else {
        return Ok(false);
    };
    let Some(sha) = composicao.documento.as_deref() else {
        return Ok(false);
    };
    if composicao.desde != id.competencia || composicao.liquido() != movimento.valor {
        return Ok(false);
    }
    let contracheque_vinculado = banco::ler_documentos(c, &id.recorrente, Some(&id.to_string()))?
        .iter()
        .any(|d| d.sha256 == sha && d.tipo == TipoDeDocumento::Contracheque);
    if !contracheque_vinculado {
        return Ok(false);
    }
    Ok(conta_exigida(s, id)?.is_some_and(|conta| {
        normalizar(&conta).trim() == normalizar(&movimento.conta_do_extrato).trim()
    }))
}

/// A conta do extrato (par confirmado) do apelido da ocorrência, se houver.
fn conta_exigida(s: &PlanejamentoSqlite, id: &IdOcorrencia) -> Resultado<Option<String>> {
    let c = s.banco.conexao();
    let hoje = data_de_segundos(s.agora());
    let pares = banco::ler_pares_de_conta(c)?;
    Ok(s.proximas(&id.recorrente, id.competencia, 1, hoje)?
        .into_iter()
        .find(|o| o.id == *id)
        .and_then(|o| o.conta)
        .and_then(|a| pares.get(a.trim()).cloned()))
}

/// O movimento do extrato de uma conciliação gravada, procurado no dia dela.
pub(super) fn movimento_do_extrato(
    s: &PlanejamentoSqlite,
    conciliacao: &Conciliacao,
) -> Option<MovimentoDoExtrato> {
    s.movimentos
        .movimentos(conciliacao.data, conciliacao.data)
        .into_iter()
        .find(|m| m.movimento == conciliacao.movimento)
}

/// As baixas que o contracheque da competência prova (D3).
pub(super) fn baixas(
    s: &PlanejamentoSqlite,
    competencia: Competencia,
) -> Resultado<Vec<BaixaPeloContracheque>> {
    let c = s.banco.conexao();
    let hoje = data_de_segundos(s.agora());
    let mut saida = Vec::new();
    for dono in banco::ler_recorrentes(c, None, false)?
        .iter()
        .filter(|r| r.natureza == Natureza::Entrada)
    {
        let Some(composicao) = banco::ler_composicao(c, &dono.id, competencia)? else {
            continue;
        };
        let Some(sha) = composicao.documento.as_deref() else {
            continue;
        };
        // Só o contracheque DO MÊS prova o desconto do mês: a composição das fixas, de um mês
        // anterior, não.
        if composicao.desde != competencia {
            continue;
        }
        // Várias rubricas podem seguir o mesmo compromisso (a mensalidade e um ajuste): cada uma
        // é uma baixa, mas a diferença se mede pela soma, e só a primeira a carrega.
        let mut ja_somados: Vec<&str> = Vec::new();
        for (indice, rubrica) in composicao.rubricas.iter().enumerate() {
            let (TipoDeRubrica::Desconto, Some(alvo)) = (rubrica.tipo, rubrica.recorrente.as_ref())
            else {
                continue;
            };
            let total_do_compromisso = Centavos(
                composicao
                    .rubricas
                    .iter()
                    .filter(|x| {
                        x.tipo == TipoDeRubrica::Desconto && x.recorrente.as_ref() == Some(alvo)
                    })
                    .map(|x| x.valor.0)
                    .sum(),
            );
            let primeira = !ja_somados.contains(&alvo.as_str());
            ja_somados.push(alvo.as_str());
            let Ok(ocorrencias) = s.proximas(alvo, competencia, 1, hoje) else {
                continue;
            };
            let Some(ocorrencia) = ocorrencias
                .into_iter()
                .find(|o| o.id.competencia == competencia)
            else {
                continue;
            };
            let movimento = format!("contracheque:{sha}:{indice}");
            let ja_baixada = banco::ler_conciliacoes(c, Some(alvo))?
                .remove(alvo)
                .unwrap_or_default()
                .iter()
                .any(|x| x.ocorrencia == ocorrencia.id && x.movimento == movimento && x.confirmada);
            let diferenca = ocorrencia
                .previsto
                .map(|p| Centavos(total_do_compromisso.0 - p.0))
                .filter(|d| primeira && d.0 != 0);
            saida.push(BaixaPeloContracheque {
                conciliacao: Conciliacao {
                    ocorrencia: ocorrencia.id.clone(),
                    movimento,
                    valor: rubrica.valor,
                    data: ocorrencia.vencimento,
                    confirmada: true,
                },
                rubrica: rubrica.nome.clone(),
                previsto: ocorrencia.previsto,
                diferenca,
                ja_baixada,
            });
        }
    }
    Ok(saida)
}
