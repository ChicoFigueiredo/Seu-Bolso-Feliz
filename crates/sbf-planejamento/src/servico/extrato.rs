//! A parte do serviço que olha o extrato: contas, assinaturas, candidatos e sugestões de
//! conciliação. O cálculo é do módulo [`crate::conciliacao`] (puro); aqui só se junta o que o
//! banco guarda com o que a [`FonteDeMovimentos`] devolve.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::{PlanejamentoSqlite, banco, carregar_dados};
use super::{ativos, candidatas};
use crate::conciliacao::{
    self, Alvo, Config, Evidencia, Indicio, credito_da_assinatura, data_de_segundos,
    movimentos_usados, pode_conciliar, tem_como_casar, valor_esperado,
};
use crate::projecao::{projetar_competencias_seguindo, versao_vigente};
use crate::{
    Alteracao, Assinatura, Autor, Competencia, Conciliacao, ContaDoPlanejamento, Data,
    ErroDoPlanejamento, Filtro, IdOcorrencia, MovimentoCandidato, Ocorrencia, ParDeConta, Periodo,
    Planejamento as _, Resultado,
};

/// Quantos dias para trás o extrato conta como histórico (~13 meses).
const DIAS_DO_HISTORICO: i64 = 400;

fn hoje_do_relogio(s: &PlanejamentoSqlite) -> Data {
    data_de_segundos(s.agora())
}

pub(super) fn contas(s: &PlanejamentoSqlite) -> Resultado<Vec<ContaDoPlanejamento>> {
    let c = s.banco.conexao();
    let pares = banco::ler_pares_de_conta(c)?;
    let mut apelidos: BTreeSet<String> = banco::ler_recorrentes(c, None, false)?
        .iter()
        .flat_map(|r| r.versoes.iter())
        .filter_map(|v| v.conta.as_deref())
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect();
    apelidos.extend(pares.keys().cloned());
    Ok(apelidos
        .into_iter()
        .map(|apelido| ContaDoPlanejamento {
            conta_do_extrato: pares.get(&apelido).cloned(),
            apelido,
        })
        .collect())
}

pub(super) fn contas_do_extrato(s: &PlanejamentoSqlite) -> Vec<String> {
    let hoje = hoje_do_relogio(s);
    s.movimentos
        .movimentos(hoje.somar_dias(-DIAS_DO_HISTORICO), hoje)
        .into_iter()
        .map(|m| m.conta_do_extrato)
        .filter(|c| !c.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn assinatura(
    s: &PlanejamentoSqlite,
    recorrente: &str,
) -> Resultado<Option<Assinatura>> {
    Ok(banco::ler_assinaturas(s.banco.conexao())?.remove(recorrente))
}

/// O alvo de conciliação de uma ocorrência: valor esperado, conta exigida e assinatura.
fn montar_alvo(
    s: &PlanejamentoSqlite,
    o: &Ocorrencia,
    pares: &HashMap<String, String>,
    assinaturas: &HashMap<String, Assinatura>,
) -> Resultado<Alvo> {
    let composicao = banco::ler_composicao(s.banco.conexao(), &o.id.recorrente, o.id.competencia)?;
    Ok(Alvo {
        esperado: valor_esperado(o, composicao.as_ref()),
        conta_exigida: o
            .conta
            .as_deref()
            .and_then(|a| pares.get(a.trim()))
            .cloned(),
        assinatura: assinaturas.get(&o.id.recorrente).cloned(),
        ocorrencia: o.clone(),
    })
}

/// Os movimentos que as conciliações já gravadas seguram. `so_confirmadas` deixa de fora as
/// sugestões; `exceto` é a ocorrência que está escolhendo.
fn usados(
    s: &PlanejamentoSqlite,
    so_confirmadas: bool,
    exceto: Option<&IdOcorrencia>,
) -> Resultado<HashSet<String>> {
    let todas = banco::ler_conciliacoes(s.banco.conexao(), None)?;
    Ok(movimentos_usados(
        todas.values().flatten(),
        so_confirmadas,
        exceto,
    ))
}

pub(super) fn candidatos(
    s: &PlanejamentoSqlite,
    id: &IdOcorrencia,
) -> Resultado<Vec<MovimentoCandidato>> {
    let hoje = hoje_do_relogio(s);
    let ocorrencia = s
        .proximas(&id.recorrente, id.competencia, 1, hoje)?
        .into_iter()
        .find(|o| o.id == *id)
        .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("ocorrência {id}")))?;
    let c = s.banco.conexao();
    let alvo = montar_alvo(
        s,
        &ocorrencia,
        &banco::ler_pares_de_conta(c)?,
        &banco::ler_assinaturas(c)?,
    )?;
    let cfg = Config::default();
    let janela = alvo
        .assinatura
        .as_ref()
        .map_or(0, |a| i64::from(a.janela_dias))
        .max(cfg.janela_dos_candidatos);
    let movimentos = s.movimentos.movimentos(
        ocorrencia.vencimento.somar_dias(-janela),
        ocorrencia.vencimento.somar_dias(janela),
    );
    Ok(conciliacao::candidatos(
        &alvo,
        &movimentos,
        &usados(s, true, Some(id))?,
        &cfg,
    ))
}

/// As conciliações gravadas da ocorrência, cada uma com o movimento do extrato (procurado no
/// dia da conciliação) e se a conta bate com o par confirmado do apelido.
pub(super) fn conciliacoes_da_ocorrencia(
    s: &PlanejamentoSqlite,
    id: &IdOcorrencia,
) -> Resultado<Vec<crate::ConciliacaoComMovimento>> {
    let c = s.banco.conexao();
    let gravadas: Vec<_> = banco::ler_conciliacoes(c, Some(&id.recorrente))?
        .remove(&id.recorrente)
        .unwrap_or_default()
        .into_iter()
        .filter(|x| x.ocorrencia == *id)
        .collect();
    if gravadas.is_empty() {
        return Ok(Vec::new());
    }
    let hoje = hoje_do_relogio(s);
    let pares = banco::ler_pares_de_conta(c)?;
    let conta_exigida = s
        .proximas(&id.recorrente, id.competencia, 1, hoje)?
        .into_iter()
        .find(|o| o.id == *id)
        .and_then(|o| o.conta)
        .and_then(|a| pares.get(a.trim()).cloned());
    Ok(gravadas
        .into_iter()
        .map(|conciliacao| {
            let movimento = s
                .movimentos
                .movimentos(conciliacao.data, conciliacao.data)
                .into_iter()
                .find(|m| m.movimento == conciliacao.movimento);
            let mesma_conta = movimento
                .as_ref()
                .zip(conta_exigida.as_ref())
                .is_some_and(|(m, conta)| &m.conta_do_extrato == conta);
            let automatica =
                banco::autor_da_conciliacao(c, &id.to_string(), &conciliacao.movimento)
                    .ok()
                    .flatten()
                    .is_some_and(|a| matches!(a, Autor::Regra { .. }));
            crate::ConciliacaoComMovimento {
                conciliacao,
                movimento,
                mesma_conta,
                automatica,
            }
        })
        .collect())
}

pub(super) fn buscar(
    s: &mut PlanejamentoSqlite,
    periodo: &Periodo,
    hoje: Data,
) -> Resultado<usize> {
    let sugestoes = sugerir_do_periodo(s, periodo, hoje)?;
    let mut n = sugestoes.len();
    for conciliacao in sugestoes {
        gravar_sugestao_ou_automatica(s, conciliacao, Autor::Importacao)?;
    }
    // Uma sugestão que já estava lá e agora fecha as três provas (o contracheque chegou
    // depois) também é promovida.
    let controle = s.controle(periodo, Filtro::Todos, None, hoje)?;
    for o in controle
        .linhas
        .iter()
        .chain(controle.atrasadas_de_antes.iter())
        .filter(|o| o.sugestao_de_conciliacao && o.conciliado.is_none() && o.e_entrada())
    {
        let Some(sugestao) = s.sugestao_de_conciliacao(&o.id)? else {
            continue;
        };
        if promover_se_tres_provas(s, &sugestao)? {
            n += 1;
        }
    }
    Ok(n)
}

/// Grava a sugestão; se as três provas batem e ninguém desfez este par antes, grava confirmada
/// por `Autor::Regra`.
fn gravar_sugestao_ou_automatica(
    s: &mut PlanejamentoSqlite,
    conciliacao: Conciliacao,
    autor: Autor,
) -> Resultado<()> {
    if promover_se_tres_provas(s, &conciliacao)? {
        return Ok(());
    }
    s.aplicar(Alteracao::Conciliar { conciliacao }, autor)?;
    Ok(())
}

/// As três provas batem e o par nunca foi desfeito: grava confirmada, `Autor::Regra`.
fn promover_se_tres_provas(s: &mut PlanejamentoSqlite, c: &Conciliacao) -> Resultado<bool> {
    if s.conciliacao_desfeita(&c.ocorrencia, &c.movimento)? {
        return Ok(false);
    }
    let Some(movimento) = super::salario::movimento_do_extrato(s, c) else {
        return Ok(false);
    };
    if !super::salario::tres_provas(s, &c.ocorrencia, &movimento)? {
        return Ok(false);
    }
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                confirmada: true,
                ..c.clone()
            },
        },
        Autor::Regra {
            regra: super::salario::REGRA_TRES_PROVAS.to_owned(),
        },
    )?;
    Ok(true)
}

/// As sugestões do período, sem gravar: só ocorrências sem conciliação nem sugestão.
fn sugerir_do_periodo(
    s: &PlanejamentoSqlite,
    periodo: &Periodo,
    hoje: Data,
) -> Resultado<Vec<Conciliacao>> {
    let controle = s.controle(periodo, Filtro::Todos, None, hoje)?;
    let c = s.banco.conexao();
    let (pares, assinaturas) = (banco::ler_pares_de_conta(c)?, banco::ler_assinaturas(c)?);
    let mut alvos = Vec::new();
    for o in controle
        .linhas
        .iter()
        .chain(controle.atrasadas_de_antes.iter())
        .filter(|o| pode_conciliar(o))
    {
        let alvo = montar_alvo(s, o, &pares, &assinaturas)?;
        if tem_como_casar(&alvo) {
            alvos.push(alvo);
        }
    }
    let cfg = Config::default();
    let (Some(menor), Some(maior)) = (
        alvos.iter().map(|a| a.ocorrencia.vencimento).min(),
        alvos.iter().map(|a| a.ocorrencia.vencimento).max(),
    ) else {
        return Ok(Vec::new());
    };
    let janela = alvos
        .iter()
        .filter_map(|a| a.assinatura.as_ref())
        .map(|a| i64::from(a.janela_dias))
        .max()
        .unwrap_or(0)
        .max(cfg.janela_dias);
    let movimentos = s
        .movimentos
        .movimentos(menor.somar_dias(-janela), maior.somar_dias(janela));
    Ok(conciliacao::sugerir(
        &alvos,
        &movimentos,
        &usados(s, false, None)?,
        &cfg,
        None,
    ))
}

pub(super) fn pares_sugeridos(s: &PlanejamentoSqlite) -> Resultado<Vec<ParDeConta>> {
    let hoje = hoje_do_relogio(s);
    let de = hoje.somar_dias(-DIAS_DO_HISTORICO);
    let movimentos = s.movimentos.movimentos(de, hoje);
    let conta_do: HashMap<&str, &str> = movimentos
        .iter()
        .filter(|m| !m.conta_do_extrato.trim().is_empty())
        .map(|m| (m.movimento.as_str(), m.conta_do_extrato.as_str()))
        .collect();
    let c = s.banco.conexao();
    let dados = carregar_dados(c)?;
    let (pares, assinaturas) = (banco::ler_pares_de_conta(c)?, banco::ler_assinaturas(c)?);
    let cfg = Config::default();
    let mut evidencias: Vec<Evidencia> = Vec::new();
    for rec in ativos(&dados) {
        let (excecoes, conciliacoes) = (dados.excecoes_de(&rec.id), dados.conciliacoes_de(&rec.id));
        for x in conciliacoes {
            let apelido = versao_vigente(rec, x.ocorrencia.competencia)
                .and_then(|v| v.conta.as_deref())
                .map(str::trim)
                .filter(|a| !a.is_empty());
            if let (Some(apelido), Some(conta)) = (apelido, conta_do.get(x.movimento.as_str())) {
                evidencias.push(Evidencia {
                    apelido: apelido.to_owned(),
                    conta_do_extrato: (*conta).to_owned(),
                    indicio: Indicio::Conciliacao {
                        confirmada: x.confirmada,
                    },
                });
            }
        }
        let Some(assinatura) = assinaturas.get(&rec.id) else {
            continue;
        };
        let competencias = candidatas(
            Competencia::de_data(de),
            Competencia::de_data(hoje),
            excecoes,
        );
        for o in projetar_competencias_seguindo(
            rec,
            excecoes,
            conciliacoes,
            competencias,
            hoje,
            dados.inicio,
            dados.seguidas_de(&rec.id),
        )
        .into_iter()
        .filter(|o| !o.suprimida())
        {
            let Some(apelido) = o.conta.as_deref().map(str::trim).filter(|a| !a.is_empty()) else {
                continue;
            };
            let apelido = apelido.to_owned();
            let alvo = Alvo {
                ocorrencia: o,
                esperado: None,
                conta_exigida: None,
                assinatura: Some(assinatura.clone()),
            };
            if let Some(m) = credito_da_assinatura(&alvo, &movimentos, &cfg)
                && !m.conta_do_extrato.trim().is_empty()
            {
                evidencias.push(Evidencia {
                    apelido,
                    conta_do_extrato: m.conta_do_extrato.clone(),
                    indicio: Indicio::Assinatura,
                });
            }
        }
    }
    let ja_pareados: HashSet<String> = pares.keys().cloned().collect();
    let contas_ja_usadas: HashSet<String> = pares.values().cloned().collect();
    Ok(conciliacao::pares_sugeridos(
        &evidencias,
        &ja_pareados,
        &contas_ja_usadas,
    ))
}
