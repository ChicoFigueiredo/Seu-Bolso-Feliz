//! Dinheiro (spec 10): assinatura que subiu de preço, cobrança recorrente sem documento,
//! tarifas somadas no ciclo, juros e multa pagos. Lê as movimentações do cache do
//! livro-caixa. Valores em centavos `i64`; a projeção anual é sempre estimativa.

use std::collections::{BTreeMap, HashMap};

use sbf_livro::Data;
use serde_json::Value;

use super::linhas_do_cache;
use crate::analise::{Analise, Contexto, Limites};
use crate::erro::Resultado;
use crate::tipos::{Area, Candidato, Evidencia, Impacto, Unidade};

pub const REAJUSTE: &str = "assinatura-reajuste";
pub const SEM_DOCUMENTO: &str = "cobranca-sem-documento";
pub const TARIFAS: &str = "tarifas-no-ciclo";
pub const JUROS: &str = "juros-e-multa";

#[derive(Debug, Clone)]
struct Mov {
    id: String,
    data: Data,
    valor: i64,
    tipo: String,
    grupo: String,
    descricao: String,
    tem_documento: bool,
}

fn texto<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

fn indice_do_mes(d: Data) -> i32 {
    d.ano * 12 + i32::from(d.mes)
}

fn reais(c: i64) -> String {
    format!("R$ {},{:02}", c.abs() / 100, c.abs() % 100)
}

pub(crate) fn rodar(ctx: &Contexto<'_>, lim: &Limites, a: &mut Analise) -> Resultado<()> {
    let Some(cache) = ctx.cache_do_livro else {
        return Ok(());
    };
    let Some(linhas) = linhas_do_cache(cache, "movimentacoes")? else {
        return Ok(());
    };
    let nomes: HashMap<String, String> = linhas_do_cache(cache, "fornecedores")?
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(id, v)| Some((id, texto(&v, "nome")?.to_owned())))
        .collect();
    let movs: Vec<Mov> = linhas
        .iter()
        .filter_map(|(id, v)| {
            let data = Data::de_iso(texto(v, "data")?)?;
            let descricao = texto(v, "descricao").unwrap_or_default().to_owned();
            let fornecedor = texto(v, "fornecedor");
            let grupo = match fornecedor {
                Some(f) => format!("f:{f}"),
                None if !descricao.is_empty() => format!("d:{}", descricao.to_lowercase()),
                None => String::new(),
            };
            Some(Mov {
                id: id.clone(),
                data,
                valor: v.get("valor").and_then(Value::as_i64)?.abs(),
                tipo: texto(v, "tipo")?.to_owned(),
                grupo,
                descricao,
                tem_documento: texto(v, "documento").is_some(),
            })
        })
        .collect();
    a.avaliadas
        .extend([REAJUSTE, SEM_DOCUMENTO, TARIFAS, JUROS]);
    recorrentes(ctx, lim, &movs, &nomes, a);
    no_ciclo(ctx, lim, &movs, a);
    Ok(())
}

fn nome_do_grupo(grupo: &str, movs: &[&Mov], nomes: &HashMap<String, String>) -> String {
    grupo
        .strip_prefix("f:")
        .and_then(|f| nomes.get(f).cloned())
        .or_else(|| movs.last().map(|m| m.descricao.clone()))
        .unwrap_or_else(|| grupo.to_owned())
}

fn ev(m: &Mov, nome: &str) -> Evidencia {
    Evidencia::nova(
        "movimentacao",
        m.id.clone(),
        format!("{nome}, {} ({})", reais(m.valor), m.data),
    )
}

/// Reajuste e cobrança sem documento: por fornecedor, uma cobrança por mês, em meses
/// seguidos que terminam no mês mais recente.
fn recorrentes(
    ctx: &Contexto<'_>,
    lim: &Limites,
    movs: &[Mov],
    nomes: &HashMap<String, String>,
    a: &mut Analise,
) {
    let mut grupos: BTreeMap<&str, Vec<&Mov>> = BTreeMap::new();
    for m in movs
        .iter()
        .filter(|m| m.tipo == "saida" && !m.grupo.is_empty())
    {
        grupos.entry(&m.grupo).or_default().push(m);
    }
    let limite_de_recencia = ctx.hoje.somar_dias(-62);
    for (grupo, mut ms) in grupos {
        ms.sort_by_key(|m| (m.data, m.id.clone()));
        // Uma cobrança por mês: a última do mês.
        let mut por_mes: BTreeMap<i32, Vec<&Mov>> = BTreeMap::new();
        for m in &ms {
            por_mes.entry(indice_do_mes(m.data)).or_default().push(m);
        }
        let Some((&ultimo, _)) = por_mes.iter().next_back() else {
            continue;
        };
        // Os meses seguidos que terminam no último, do mais novo para o mais velho.
        let corrida: Vec<&Vec<&Mov>> = (0..).map_while(|i| por_mes.get(&(ultimo - i))).collect();
        let Some(mais_nova) = corrida.first().and_then(|m| m.last()) else {
            continue;
        };
        if mais_nova.data < limite_de_recencia || corrida.len() < 3 {
            continue;
        }
        let nome = nome_do_grupo(grupo, &ms, nomes);
        let valor = |i: usize| corrida[i].last().map_or(0, |m| m.valor);
        let (v0, v1, v2) = (valor(0), valor(1), valor(2));
        let ultimas = |n: usize| -> Vec<&Mov> {
            corrida
                .iter()
                .take(n)
                .filter_map(|m| m.last().copied())
                .collect()
        };

        // Preço subiu: os dois meses anteriores estáveis (até 2%) e o último acima do limite.
        let estavel = (v1 - v2).abs() * 100 <= 2 * v2.max(1);
        let salto = v0 - v1;
        if estavel
            && salto >= lim.reajuste_minimo_centavos
            && salto * 100 >= lim.reajuste_minimo_pct * v1
        {
            let tres = ultimas(3);
            a.candidatos.push(Candidato {
                regra: REAJUSTE,
                chave: format!("reajuste:{grupo}"),
                area: Area::Dinheiro,
                titulo: format!("{nome} subiu de preço"),
                detalhe: format!(
                    "Era {} nos dois meses anteriores e agora é {} (+{}).",
                    reais(v1),
                    reais(v0),
                    reais(salto)
                ),
                evidencia: tres.iter().map(|m| ev(m, &nome)).collect(),
                impacto: Some(Impacto {
                    valor: salto * 12,
                    unidade: Unidade::CentavosBrl,
                    periodo: "ano".into(),
                    estimativa: true,
                }),
                assinatura: tres
                    .iter()
                    .map(|m| m.id.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            });
        }

        // Cobrança todo mês, de valor parecido, sem nenhum documento ligado.
        if corrida.len() >= lim.meses_sem_documento {
            let tomadas: Vec<&Mov> = corrida
                .iter()
                .take(lim.meses_sem_documento)
                .flat_map(|m| m.iter().copied())
                .collect();
            let parecidos = tomadas
                .iter()
                .all(|m| (m.valor - v0).abs() * 100 <= 5 * v0.max(1));
            if parecidos && tomadas.iter().all(|m| !m.tem_documento) {
                a.candidatos.push(Candidato {
                    regra: SEM_DOCUMENTO,
                    chave: format!("sem-documento:{grupo}"),
                    area: Area::Dinheiro,
                    titulo: format!("{nome} cobra todo mês e nenhum documento comprova"),
                    detalhe: format!(
                        "{} meses seguidos de cerca de {}, sem fatura ou comprovante ligado. \
                         Vale conferir se você ainda usa.",
                        lim.meses_sem_documento,
                        reais(v0)
                    ),
                    evidencia: tomadas.iter().map(|m| ev(m, &nome)).collect(),
                    impacto: Some(Impacto {
                        valor: v0 * 12,
                        unidade: Unidade::CentavosBrl,
                        periodo: "ano".into(),
                        estimativa: true,
                    }),
                    assinatura: tomadas
                        .iter()
                        .map(|m| m.id.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                });
            }
        }
    }
}

/// Tarifas e juros/multa somados no ciclo atual. O valor é o apurado (não é estimativa).
fn no_ciclo(ctx: &Contexto<'_>, lim: &Limites, movs: &[Mov], a: &mut Analise) {
    let ciclo = ctx.ciclo();
    let do_ciclo = |f: &dyn Fn(&Mov) -> bool| -> Vec<&Mov> {
        let mut v: Vec<&Mov> = movs
            .iter()
            .filter(|m| ciclo.contem(m.data) && f(m))
            .collect();
        v.sort_by_key(|m| (m.data, m.id.clone()));
        v
    };
    let monta = |regra: &'static str, titulo: &str, lista: &[&Mov], rotulo: &str| -> Candidato {
        let soma: i64 = lista.iter().map(|m| m.valor).sum();
        Candidato {
            regra,
            chave: format!("{regra}:{}", ciclo.nome()),
            area: Area::Dinheiro,
            titulo: titulo.to_owned(),
            detalhe: format!(
                "{} lançamentos somam {} no ciclo {}.",
                lista.len(),
                reais(soma),
                ciclo.rotulo()
            ),
            evidencia: lista.iter().take(20).map(|m| ev(m, rotulo)).collect(),
            impacto: Some(Impacto {
                valor: soma,
                unidade: Unidade::CentavosBrl,
                periodo: "ciclo".into(),
                estimativa: false,
            }),
            assinatura: format!(
                "{}|{soma}|{}",
                lista.len(),
                lista.last().map_or("", |m| m.id.as_str())
            ),
        }
    };
    let tarifas = do_ciclo(&|m| m.tipo == "tarifa");
    let soma: i64 = tarifas.iter().map(|m| m.valor).sum();
    if tarifas.len() >= lim.tarifas_minimo_cobrancas && soma >= lim.tarifas_minimo_centavos {
        a.candidatos.push(monta(
            TARIFAS,
            "Tarifas bancárias somam mais que o normal",
            &tarifas,
            "Tarifa",
        ));
    }
    let juros = do_ciclo(&|m| {
        m.tipo == "juros" || (m.tipo == "saida" && m.descricao.to_lowercase().contains("multa"))
    });
    if !juros.is_empty() && juros.iter().map(|m| m.valor).sum::<i64>() > 0 {
        a.candidatos.push(monta(
            JUROS,
            "Você pagou juros ou multa neste ciclo",
            &juros,
            "Juros ou multa",
        ));
    }
}
