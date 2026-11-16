//! As 8 perguntas do §11 sobre os registros do inventário (spec 06, "Relatórios do §11").
//!
//! - **Marca:** todo valor sai com [`Fonte::Inventario`]; a janela põe "a conferir".
//! - **Gasto:** movimento entre contas não conta como gasto ([`crate::movimento`]).
//! - **Fonte:** [`Fonte::LivroCaixa`] devolve [`ErroDeConsulta::NaoSuportado`] na fatia 1.
//!   O código das perguntas lê os gastos por [`FonteDeGastos`]: o livro-caixa implementa o
//!   mesmo trait na fatia 2 e as 8 perguntas passam a servir as duas fontes (ADR-012: uma
//!   implementação por regra).
//! - **Vocabulário:** nenhum texto daqui usa os termos do livro-caixa ("Resultado do mês",
//!   "Disponível agora", "Livre após compromissos"). O teste `leitura_relatorios` confere.
//! - "O que parece cortável?" aponta para Pontos de atenção (fatia 6): esta fatia não
//!   julga gasto.
//!
//! Dono: agente de leitura (INV-18).

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::types::Value;

use crate::consulta::{
    Celula, Centavos, ErroDeConsulta, Filtro, Fonte, Linha, Painel, Pergunta, ResultadoDeConsulta,
    Secao, Tabela,
};
use crate::movimento;
use crate::referencia::{Ref, Tipo};
use crate::relacoes::{SQL_CATEGORIA_DO_REGISTRO, recorte};
use crate::visao_geral::ciclo_da_data;
use crate::vistas::{
    self, Contexto, Selecao, celula_contagem, celula_data_opt, celula_link, celula_link_opt,
    celula_texto, celula_texto_opt, celula_valor, celula_valor_total, coluna, coluna_num,
    consultar, mes_anterior, rotulo_do_registro, texto,
};

/// Quantos ciclos entram na janela padrão, além do atual.
const CICLOS_ANTERIORES_NA_JANELA: usize = 12;
/// Quantos fornecedores aparecem nos rankings.
const TOPO_DE_FORNECEDORES: usize = 10;
/// Um fornecedor "se repete" quando tem gasto em pelo menos tantos ciclos da janela.
const CICLOS_PARA_REPETIR: usize = 3;
/// Categorias que contam como assinatura.
const CATEGORIAS_DE_ASSINATURA: [&str; 2] = ["assinaturas", "telefonia"];

pub const AVISO_MOVIMENTO: &str = "Movimento entre contas (transferência entre contas, investimento e pagamento de fatura) não conta como gasto.";

/// A pergunta, como o usuário a lê.
pub fn titulo_da_pergunta(pergunta: Pergunta) -> &'static str {
    match pergunta {
        Pergunta::QuantoSaiu => "Quanto saiu?",
        Pergunta::MaioresFornecedores => "Quais são os maiores fornecedores?",
        Pergunta::OQueSeRepete => "O que se repete todo ciclo?",
        Pergunta::OQueAumentou => "O que aumentou?",
        Pergunta::OndeSeConcentra => "Onde o dinheiro se concentra?",
        Pergunta::NaoClassificado => "O que ainda não foi classificado?",
        Pergunta::PossiveisDuplicidades => "O que parece duplicado?",
        Pergunta::OQueFaltaParaConfiar => "O que falta para a análise ficar confiável?",
    }
}

// ---------------------------------------------------------------------------
// A fonte dos gastos
// ---------------------------------------------------------------------------

/// Um gasto agrupado: o que saiu num ciclo, para um fornecedor, uma categoria e uma conta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gasto {
    pub ciclo: String,
    pub fornecedor: Option<String>,
    /// `Sem categoria` quando não há.
    pub categoria: String,
    pub conta: Option<String>,
    /// Positivo: o que saiu.
    pub centavos: Centavos,
    pub vezes: i64,
}

/// De onde vêm os gastos que as perguntas somam.
pub trait FonteDeGastos {
    /// Os gastos agrupados dos ciclos `de` a `ate`, inclusive. Nunca inclui movimento
    /// entre contas.
    fn gastos(&self, de: &str, ate: &str, fontes: &[String]) -> ResultadoDeConsulta<Vec<Gasto>>;
}

/// Os gastos dos registros do inventário.
#[derive(Debug, Clone, Copy)]
pub struct GastosDoInventario<'a> {
    pub ctx: Contexto<'a>,
}

impl FonteDeGastos for GastosDoInventario<'_> {
    fn gastos(&self, de: &str, ate: &str, fontes: &[String]) -> ResultadoDeConsulta<Vec<Gasto>> {
        let mut sel =
            Selecao::nova().onde("r.ciclo_id BETWEEN ? AND ?", vec![texto(de), texto(ate)]);
        if !fontes.is_empty() {
            sel = sel.onde(
                format!("r.fonte_id IN ({})", vec!["?"; fontes.len()].join(",")),
                fontes.iter().map(|f| texto(f.as_str())).collect(),
            );
        }
        let sql = format!(
            "SELECT r.ciclo_id, {fornecedor}, {categoria}, r.conta_provavel, -sum(r.valor_centavos), count(*)
             FROM registro r WHERE {gasto}{sel}
             GROUP BY 1, 2, 3, 4",
            fornecedor = self.ctx.sql_fornecedor_do_registro("r."),
            categoria = SQL_CATEGORIA_DO_REGISTRO,
            gasto = movimento::sql_e_gasto("r."),
            sel = sel.sql_e(),
        );
        consultar(self.ctx.conn, &sql, sel.params(), |l| {
            Ok(Gasto {
                ciclo: l.get(0)?,
                fornecedor: l.get(1)?,
                categoria: l.get(2)?,
                conta: l.get(3)?,
                centavos: l.get(4)?,
                vezes: l.get(5)?,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// A janela de ciclos
// ---------------------------------------------------------------------------

/// O intervalo de ciclos de um relatório: o do filtro, ou os 12 anteriores ao atual e o atual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Janela {
    pub de: String,
    pub ate: String,
    pub atual: String,
    /// As conexões escolhidas no filtro (vazio = todas).
    pub fontes: Vec<String>,
}

impl Janela {
    pub fn do_filtro(filtro: &Filtro, hoje: &str) -> Janela {
        let atual = ciclo_da_data(hoje).unwrap_or_else(|| hoje.get(..7).unwrap_or(hoje).to_owned());
        match &filtro.ciclos {
            Some((de, ate)) => Janela {
                de: de.clone(),
                ate: ate.clone(),
                atual,
                fontes: filtro.fontes.clone(),
            },
            None => {
                let mut de = atual.clone();
                for _ in 0..CICLOS_ANTERIORES_NA_JANELA {
                    de = mes_anterior(&de);
                }
                Janela {
                    de,
                    ate: atual.clone(),
                    atual,
                    fontes: filtro.fontes.clone(),
                }
            }
        }
    }

    /// O recorte de um total da janela: o mesmo intervalo de ciclos e as mesmas conexões.
    fn recorte(&self, base: &str) -> String {
        let filtro = Filtro {
            ciclos: Some((self.de.clone(), self.ate.clone())),
            fontes: self.fontes.clone(),
            ..Filtro::default()
        };
        vistas::recorte_com_filtro(base, &filtro)
    }
}

fn ref_de_todos_os_registros() -> Ref {
    Ref::nova(Tipo::Registro, "*")
}

fn percentual(parte: Centavos, total: Centavos) -> String {
    if total <= 0 {
        return "—".to_owned();
    }
    #[allow(clippy::cast_precision_loss)]
    let p = 100.0 * parte as f64 / total as f64;
    format!("{p:.0}%")
}

fn secao(titulo: &str, nota: Option<String>, tabela: Tabela) -> Secao {
    Secao {
        titulo: titulo.to_owned(),
        nota,
        tabela,
    }
}

fn painel(secoes: Vec<Secao>) -> Painel {
    Painel {
        secoes,
        avisos: vec![AVISO_MOVIMENTO.to_owned()],
    }
}

// ---------------------------------------------------------------------------
// Agregações em Rust sobre os gastos agrupados
// ---------------------------------------------------------------------------

fn total_por_ciclo(gastos: &[Gasto]) -> BTreeMap<String, Centavos> {
    let mut m = BTreeMap::new();
    for g in gastos {
        *m.entry(g.ciclo.clone()).or_insert(0) += g.centavos;
    }
    m
}

#[derive(Debug, Default, Clone)]
struct DoFornecedor {
    centavos: Centavos,
    vezes: i64,
    ciclos: BTreeSet<String>,
}

fn total_por_fornecedor(gastos: &[Gasto]) -> BTreeMap<Option<String>, DoFornecedor> {
    let mut m: BTreeMap<Option<String>, DoFornecedor> = BTreeMap::new();
    for g in gastos {
        let e = m.entry(g.fornecedor.clone()).or_default();
        e.centavos += g.centavos;
        e.vezes += g.vezes;
        e.ciclos.insert(g.ciclo.clone());
    }
    m
}

fn ordenar_por_valor<T: Clone>(
    mapa: &BTreeMap<Option<String>, T>,
    valor: impl Fn(&T) -> Centavos,
) -> Vec<(Option<String>, T)> {
    let mut v: Vec<(Option<String>, T)> =
        mapa.iter().map(|(k, t)| (k.clone(), t.clone())).collect();
    v.sort_by(|a, b| valor(&b.1).cmp(&valor(&a.1)).then_with(|| a.0.cmp(&b.0)));
    v
}

fn celula_de_fornecedor(nome: &Option<String>) -> Celula {
    match nome {
        Some(n) if !n.is_empty() => celula_link(Tipo::Fornecedor, n, n),
        _ => Celula::Texto("(sem fornecedor)".to_owned()),
    }
}

fn alvo_de_fornecedor(nome: &Option<String>) -> Option<Ref> {
    nome.as_deref()
        .filter(|n| !n.is_empty())
        .map(|n| Ref::nova(Tipo::Fornecedor, n))
}

// ---------------------------------------------------------------------------
// As perguntas
// ---------------------------------------------------------------------------

fn quanto_saiu(
    fonte: &dyn FonteDeGastos,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    let gastos = fonte.gastos(&j.de, &j.ate, &filtro.fontes)?;
    let por_ciclo = total_por_ciclo(&gastos);
    let ciclo_atual = Ref::nova(Tipo::Ciclo, j.atual.as_str());
    let atual = por_ciclo.get(&j.atual).copied().unwrap_or(0);
    let tabela_atual = Tabela {
        colunas: vec![coluna("Ciclo"), coluna("Período"), coluna_num("Saiu")],
        linhas: vec![Linha {
            alvo: Some(ciclo_atual.clone()),
            celulas: vec![
                celula_link(Tipo::Ciclo, &j.atual, &format!("Ciclo {}", j.atual)),
                Celula::Texto(format!(
                    "{} a {}, até hoje",
                    vistas::inicio_do_ciclo(&j.atual),
                    vistas::fim_do_ciclo(&j.atual)
                )),
                celula_valor_total(atual, ciclo_atual, Some(&j.atual), recorte::SAIDAS),
            ],
        }],
    };
    let mut ultimos: Vec<(&String, &Centavos)> = por_ciclo.iter().collect();
    ultimos.reverse();
    let tabela_ultimos = Tabela {
        colunas: vec![coluna("Ciclo"), coluna_num("Saiu"), coluna_num("Gastos")],
        linhas: ultimos
            .into_iter()
            .take(6)
            .map(|(ciclo, centavos)| {
                let de = Ref::nova(Tipo::Ciclo, ciclo.as_str());
                let vezes: i64 = gastos
                    .iter()
                    .filter(|g| &g.ciclo == ciclo)
                    .map(|g| g.vezes)
                    .sum();
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        celula_link(Tipo::Ciclo, ciclo, &format!("Ciclo {ciclo}")),
                        celula_valor_total(*centavos, de.clone(), Some(ciclo), recorte::SAIDAS),
                        celula_contagem(vezes, de, Some(ciclo), recorte::SAIDAS),
                    ],
                }
            })
            .collect(),
    };
    Ok(painel(vec![
        secao(
            "Quanto saiu no ciclo atual?",
            Some(format!("O ciclo {} ainda está em andamento.", j.atual)),
            tabela_atual,
        ),
        secao(
            "Quanto saiu nos últimos ciclos?",
            Some(format!(
                "Os 6 últimos ciclos da janela {} a {}.",
                j.de, j.ate
            )),
            tabela_ultimos,
        ),
    ]))
}

fn maiores_fornecedores(
    fonte: &dyn FonteDeGastos,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    let gastos = fonte.gastos(&j.de, &j.ate, &filtro.fontes)?;
    let total: Centavos = gastos.iter().map(|g| g.centavos).sum();
    let ordenados = ordenar_por_valor(&total_por_fornecedor(&gastos), |f| f.centavos);
    let tabela = Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna_num("Saiu"),
            coluna_num("% do total"),
            coluna_num("Gastos"),
        ],
        linhas: ordenados
            .into_iter()
            .take(TOPO_DE_FORNECEDORES)
            .map(|(nome, f)| {
                let alvo = alvo_de_fornecedor(&nome);
                let (saiu, vezes) = match &alvo {
                    Some(de) => (
                        celula_valor_total(
                            f.centavos,
                            de.clone(),
                            None,
                            &j.recorte(recorte::SAIDAS),
                        ),
                        celula_contagem(f.vezes, de.clone(), None, &j.recorte(recorte::SAIDAS)),
                    ),
                    None => (
                        celula_valor(f.centavos),
                        Celula::Contagem {
                            n: f.vezes,
                            itens: None,
                        },
                    ),
                };
                Linha {
                    alvo,
                    celulas: vec![
                        celula_de_fornecedor(&nome),
                        saiu,
                        Celula::Texto(percentual(f.centavos, total)),
                        vezes,
                    ],
                }
            })
            .collect(),
    };
    Ok(painel(vec![secao(
        "Quais são os maiores fornecedores?",
        Some(format!("Ciclos {} a {}.", j.de, j.ate)),
        tabela,
    )]))
}

fn o_que_se_repete(
    fonte: &dyn FonteDeGastos,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    let gastos = fonte.gastos(&j.de, &j.ate, &filtro.fontes)?;
    let ciclos_da_janela = total_por_ciclo(&gastos).len().max(1);
    let por_fornecedor = total_por_fornecedor(&gastos);
    let mut repetem: Vec<(Option<String>, DoFornecedor)> = por_fornecedor
        .iter()
        .filter(|(_, f)| f.ciclos.len() >= CICLOS_PARA_REPETIR)
        .map(|(k, f)| (k.clone(), f.clone()))
        .collect();
    repetem.sort_by(|a, b| {
        b.1.ciclos
            .len()
            .cmp(&a.1.ciclos.len())
            .then(b.1.centavos.cmp(&a.1.centavos))
            .then(a.0.cmp(&b.0))
    });
    let tabela = Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna_num("Ciclos com gasto"),
            coluna_num("Média por ciclo"),
            coluna_num("Total"),
            coluna("Último ciclo"),
        ],
        linhas: repetem
            .iter()
            .map(|(nome, f)| {
                let alvo = alvo_de_fornecedor(nome);
                let n = i64::try_from(f.ciclos.len()).unwrap_or(1).max(1);
                let ultimo = f.ciclos.iter().next_back().cloned().unwrap_or_default();
                let total = match &alvo {
                    Some(de) => celula_valor_total(
                        f.centavos,
                        de.clone(),
                        None,
                        &j.recorte(recorte::SAIDAS),
                    ),
                    None => celula_valor(f.centavos),
                };
                Linha {
                    alvo: alvo.clone(),
                    celulas: vec![
                        celula_de_fornecedor(nome),
                        Celula::Texto(format!("{} de {ciclos_da_janela}", f.ciclos.len())),
                        celula_valor(f.centavos / n),
                        total,
                        celula_link(Tipo::Ciclo, &ultimo, &ultimo),
                    ],
                }
            })
            .collect(),
    };

    // Assinaturas: gastos nas categorias de assinatura, pelo ciclo mais recente de cada fornecedor.
    let mut por_ciclo_da_assinatura: BTreeMap<Option<String>, BTreeMap<String, Centavos>> =
        BTreeMap::new();
    for g in gastos
        .iter()
        .filter(|g| CATEGORIAS_DE_ASSINATURA.contains(&g.categoria.to_lowercase().as_str()))
    {
        *por_ciclo_da_assinatura
            .entry(g.fornecedor.clone())
            .or_default()
            .entry(g.ciclo.clone())
            .or_insert(0) += g.centavos;
    }
    let assinaturas: BTreeMap<Option<String>, (String, Centavos)> = por_ciclo_da_assinatura
        .into_iter()
        .filter_map(|(nome, ciclos)| {
            ciclos
                .into_iter()
                .next_back()
                .map(|(ciclo, centavos)| (nome, (ciclo, centavos)))
        })
        .collect();
    let tabela_assinaturas = Tabela {
        colunas: vec![
            coluna("Assinatura"),
            coluna("Último ciclo"),
            coluna_num("Por ciclo"),
        ],
        linhas: assinaturas
            .iter()
            .map(|(nome, (ciclo, centavos))| {
                let alvo = alvo_de_fornecedor(nome);
                let valor = match &alvo {
                    Some(de) => {
                        celula_valor_total(*centavos, de.clone(), Some(ciclo), recorte::SAIDAS)
                    }
                    None => celula_valor(*centavos),
                };
                Linha {
                    alvo,
                    celulas: vec![
                        celula_de_fornecedor(nome),
                        celula_link(Tipo::Ciclo, ciclo, ciclo),
                        valor,
                    ],
                }
            })
            .collect(),
    };
    Ok(painel(vec![
        secao(
            "O que se repete todo ciclo?",
            Some(format!(
                "Fornecedores com gasto em pelo menos {CICLOS_PARA_REPETIR} ciclos, de {} a {}.",
                j.de, j.ate
            )),
            tabela,
        ),
        secao(
            "Quais assinaturas eu tenho?",
            Some("Gastos nas categorias Assinaturas e Telefonia.".to_owned()),
            tabela_assinaturas,
        ),
    ]))
}

fn o_que_aumentou(
    fonte: &dyn FonteDeGastos,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    let anterior = mes_anterior(&j.atual);
    let gastos = fonte.gastos(&anterior, &j.atual, &filtro.fontes)?;
    let mut atual: BTreeMap<Option<String>, Centavos> = BTreeMap::new();
    let mut antes: BTreeMap<Option<String>, Centavos> = BTreeMap::new();
    for g in &gastos {
        let mapa = if g.ciclo == j.atual {
            &mut atual
        } else {
            &mut antes
        };
        *mapa.entry(g.fornecedor.clone()).or_insert(0) += g.centavos;
    }
    let mut subiu: Vec<(Option<String>, Centavos, Centavos)> = Vec::new();
    let mut novos: Vec<(Option<String>, Centavos)> = Vec::new();
    for (nome, valor) in &atual {
        match antes.get(nome) {
            Some(a) if *valor > *a => subiu.push((nome.clone(), *a, *valor)),
            Some(_) => {}
            None => novos.push((nome.clone(), *valor)),
        }
    }
    subiu.sort_by(|a, b| (b.2 - b.1).cmp(&(a.2 - a.1)).then(a.0.cmp(&b.0)));
    novos.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let celula_do_ciclo =
        |nome: &Option<String>, valor: Centavos, ciclo: &str| match alvo_de_fornecedor(nome) {
            Some(de) => celula_valor_total(valor, de, Some(ciclo), recorte::SAIDAS),
            None => celula_valor(valor),
        };
    let tabela_subiu = Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna_num(&format!("Ciclo {anterior}")),
            coluna_num(&format!("Ciclo {}", j.atual)),
            coluna_num("Aumento"),
        ],
        linhas: subiu
            .iter()
            .map(|(nome, a, b)| Linha {
                alvo: alvo_de_fornecedor(nome),
                celulas: vec![
                    celula_de_fornecedor(nome),
                    celula_do_ciclo(nome, *a, &anterior),
                    celula_do_ciclo(nome, *b, &j.atual),
                    Celula::Texto(format!("+{}", percentual(*b - *a, *a))),
                ],
            })
            .collect(),
    };
    let tabela_novos = Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna_num(&format!("Ciclo {}", j.atual)),
        ],
        linhas: novos
            .iter()
            .map(|(nome, v)| Linha {
                alvo: alvo_de_fornecedor(nome),
                celulas: vec![
                    celula_de_fornecedor(nome),
                    celula_do_ciclo(nome, *v, &j.atual),
                ],
            })
            .collect(),
    };
    Ok(painel(vec![
        secao(
            "O que aumentou?",
            Some(format!(
                "Ciclo {} contra o ciclo {anterior}. O ciclo {} ainda está em andamento.",
                j.atual, j.atual
            )),
            tabela_subiu,
        ),
        secao(
            "Apareceu neste ciclo",
            Some(format!("Fornecedores sem gasto no ciclo {anterior}.")),
            tabela_novos,
        ),
    ]))
}

fn onde_se_concentra(
    fonte: &dyn FonteDeGastos,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    let gastos = fonte.gastos(&j.de, &j.ate, &filtro.fontes)?;
    let total: Centavos = gastos.iter().map(|g| g.centavos).sum();
    let mut por_categoria: BTreeMap<String, Centavos> = BTreeMap::new();
    for g in &gastos {
        *por_categoria.entry(g.categoria.clone()).or_insert(0) += g.centavos;
    }
    let mut categorias: Vec<(String, Centavos)> = por_categoria.into_iter().collect();
    categorias.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let tabela_categorias = Tabela {
        colunas: vec![
            coluna("Categoria"),
            coluna_num("Saiu"),
            coluna_num("% do total"),
        ],
        linhas: categorias
            .iter()
            .map(|(categoria, v)| Linha {
                alvo: None,
                celulas: vec![
                    celula_texto(categoria),
                    celula_valor_total(
                        *v,
                        ref_de_todos_os_registros(),
                        None,
                        &j.recorte(&format!("{}|categoria={categoria}", recorte::SAIDAS)),
                    ),
                    Celula::Texto(percentual(*v, total)),
                ],
            })
            .collect(),
    };
    let ordenados = ordenar_por_valor(&total_por_fornecedor(&gastos), |f| f.centavos);
    let tabela_fornecedores = Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna_num("Saiu"),
            coluna_num("% do total"),
        ],
        linhas: ordenados
            .into_iter()
            .take(TOPO_DE_FORNECEDORES)
            .map(|(nome, f)| {
                let alvo = alvo_de_fornecedor(&nome);
                let valor = match &alvo {
                    Some(de) => celula_valor_total(
                        f.centavos,
                        de.clone(),
                        None,
                        &j.recorte(recorte::SAIDAS),
                    ),
                    None => celula_valor(f.centavos),
                };
                Linha {
                    alvo,
                    celulas: vec![
                        celula_de_fornecedor(&nome),
                        valor,
                        Celula::Texto(percentual(f.centavos, total)),
                    ],
                }
            })
            .collect(),
    };
    Ok(painel(vec![
        secao(
            "Por categoria",
            Some(format!("Ciclos {} a {}.", j.de, j.ate)),
            tabela_categorias,
        ),
        secao(
            "Por fornecedor",
            Some(format!(
                "Os {TOPO_DE_FORNECEDORES} maiores, ciclos {} a {}.",
                j.de, j.ate
            )),
            tabela_fornecedores,
        ),
    ]))
}

fn nao_classificado(
    ctx: &Contexto<'_>,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    let mut sel = Selecao::nova()
        .onde(movimento::sql_nao_e_movimento("r."), vec![])
        .onde(
            format!(
                "coalesce(nullif(r.fornecedor_normalizado, ''), nullif({}, ''), nullif(r.supplier_name_raw, '')) IS NULL
                 OR {SQL_CATEGORIA_DO_REGISTRO} = 'Sem categoria'",
                ctx.dialeto.contraparte("r.")
            ),
            vec![],
        )
        .onde(
            "r.ciclo_id BETWEEN ? AND ?",
            vec![texto(j.de.as_str()), texto(j.ate.as_str())],
        );
    if !filtro.fontes.is_empty() {
        sel = sel.onde(
            format!(
                "r.fonte_id IN ({})",
                vec!["?"; filtro.fontes.len()].join(",")
            ),
            filtro.fontes.iter().map(|f| texto(f.as_str())).collect(),
        );
    }
    let n = vistas::escalar(
        ctx.conn,
        &format!("SELECT count(*) FROM registro r WHERE 1 = 1{}", sel.sql_e()),
        sel.params(),
    )?;
    let soma = vistas::escalar(
        ctx.conn,
        &format!(
            "SELECT coalesce(sum(r.valor_centavos), 0) FROM registro r WHERE 1 = 1{}",
            sel.sql_e()
        ),
        sel.params(),
    )?;
    let a_triar = vistas::escalar(
        ctx.conn,
        "SELECT count(*) FROM achado WHERE estado = 'novo'",
        &[],
    )?;
    let todos = ref_de_todos_os_registros();
    let resumo = Tabela {
        colunas: vec![
            coluna("Indicador"),
            coluna_num("Quantidade"),
            coluna_num("Valor"),
        ],
        linhas: vec![
            Linha {
                alvo: None,
                celulas: vec![
                    celula_texto("Registros sem fornecedor ou sem categoria"),
                    celula_contagem(
                        n,
                        todos.clone(),
                        None,
                        &j.recorte(recorte::NAO_CLASSIFICADOS),
                    ),
                    celula_valor_total(soma, todos, None, &j.recorte(recorte::NAO_CLASSIFICADOS)),
                ],
            },
            Linha {
                alvo: None,
                celulas: vec![
                    celula_texto("Achados ainda a triar"),
                    celula_contagem(
                        a_triar,
                        Ref::nova(Tipo::Achado, "*"),
                        None,
                        recorte::A_TRIAR,
                    ),
                    Celula::Vazia,
                ],
            },
        ],
    };
    let recentes = vistas::tabela_registros(&vistas::registros(ctx, &sel.com_limite(20))?);
    Ok(painel(vec![
        secao(
            "O que ainda não foi classificado?",
            Some(format!("Ciclos {} a {}.", j.de, j.ate)),
            resumo,
        ),
        secao(
            "Os mais recentes",
            Some("Os 20 registros mais recentes sem fornecedor ou sem categoria.".to_owned()),
            recentes,
        ),
    ]))
}

fn possiveis_duplicidades(
    ctx: &Contexto<'_>,
    filtro: &Filtro,
    j: &Janela,
) -> ResultadoDeConsulta<Painel> {
    // (a) Achados diferentes com o mesmo texto normalizado.
    let mesmos_textos = consultar(
        ctx.conn,
        "SELECT a.canonical_fingerprint, a.content_hash,
                (SELECT filename FROM origem o WHERE o.content_hash = a.content_hash ORDER BY o.id LIMIT 1),
                a.supplier_name_raw, a.data_chave, a.valor_centavos
         FROM achado a
         WHERE a.estado <> 'descartado' AND a.canonical_fingerprint IN (
             SELECT canonical_fingerprint FROM achado
              WHERE estado <> 'descartado' AND canonical_fingerprint IS NOT NULL
              GROUP BY 1 HAVING count(*) > 1)
         ORDER BY a.canonical_fingerprint, a.data_chave, a.content_hash LIMIT 200",
        &[],
        |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<String>>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, Option<String>>(4)?,
                l.get::<_, Option<Centavos>>(5)?,
            ))
        },
    )?;
    let mut grupo = 0;
    let mut ultimo = String::new();
    let mut linhas_a = Vec::new();
    for (impressao, hash, nome, fornecedor, data, valor) in mesmos_textos {
        if impressao != ultimo {
            grupo += 1;
            ultimo = impressao;
        }
        linhas_a.push(Linha {
            alvo: Some(Ref::nova(Tipo::Achado, hash.as_str())),
            celulas: vec![
                Celula::Texto(format!("Grupo {grupo}")),
                celula_link(Tipo::Achado, &hash, &vistas::rotulo_do_achado(&nome, &hash)),
                celula_link_opt(Tipo::Fornecedor, &fornecedor),
                celula_data_opt(&data),
                valor.map_or(Celula::Vazia, celula_valor),
            ],
        });
    }
    let tabela_a = Tabela {
        colunas: vec![
            coluna("Grupo"),
            coluna("Achado"),
            coluna("Fornecedor"),
            coluna("Data"),
            coluna_num("Valor"),
        ],
        linhas: linhas_a,
    };

    // (b) Relações já marcadas entre achados.
    let relacoes = consultar(
        ctx.conn,
        "SELECT r.a, r.b, r.tipo, r.confianca, r.motivo,
                (SELECT filename FROM origem o WHERE o.content_hash = r.a ORDER BY o.id LIMIT 1),
                (SELECT filename FROM origem o WHERE o.content_hash = r.b ORDER BY o.id LIMIT 1)
         FROM relacao r ORDER BY r.tipo, r.a, r.b LIMIT 200",
        &[],
        |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<f64>>(3)?,
                l.get::<_, Option<String>>(4)?,
                l.get::<_, Option<String>>(5)?,
                l.get::<_, Option<String>>(6)?,
            ))
        },
    )?;
    let tabela_b = Tabela {
        colunas: vec![
            coluna("Achado"),
            coluna("Relacionado com"),
            coluna("Relação"),
            coluna_num("Confiança"),
            coluna("Motivo"),
        ],
        linhas: relacoes
            .into_iter()
            .map(|(a, b, tipo, confianca, motivo, nome_a, nome_b)| Linha {
                alvo: Some(Ref::nova(Tipo::Achado, a.as_str())),
                celulas: vec![
                    celula_link(Tipo::Achado, &a, &vistas::rotulo_do_achado(&nome_a, &a)),
                    celula_link(Tipo::Achado, &b, &vistas::rotulo_do_achado(&nome_b, &b)),
                    celula_texto(&tipo),
                    confianca.map_or(Celula::Vazia, |c| {
                        Celula::Texto(format!("{:.0}%", c * 100.0))
                    }),
                    celula_texto_opt(&motivo),
                ],
            })
            .collect(),
    };

    // (c) Registros com o mesmo valor, de tipo ou fonte diferente, em datas próximas.
    let mut params: Vec<Value> = vec![texto(j.de.as_str()), texto(j.ate.as_str())];
    let mut filtro_de_fontes = String::new();
    if !filtro.fontes.is_empty() {
        let marcas = vec!["?"; filtro.fontes.len()].join(",");
        filtro_de_fontes = format!(" AND r1.fonte_id IN ({marcas}) AND r2.fonte_id IN ({marcas})");
        for _ in 0..2 {
            params.extend(filtro.fontes.iter().map(|f| texto(f.as_str())));
        }
    }
    // Os valores repetidos em mais de uma combinação de tipo e fonte: só eles podem ter par.
    params.push(texto(j.de.as_str()));
    params.push(texto(j.ate.as_str()));
    let pares = consultar(
        ctx.conn,
        &format!(
            "SELECT r1.id, r2.id, r1.valor_centavos, r1.data, r2.data, r1.descricao_original, r2.descricao_original
             FROM registro r1 JOIN registro r2 ON r2.valor_centavos = r1.valor_centavos AND r2.id > r1.id
             WHERE r1.valor_centavos < 0 AND {m1} AND {m2}
               AND r2.data BETWEEN date(r1.data, '-5 days') AND date(r1.data, '+5 days')
               AND (r1.tipo <> r2.tipo OR r1.fonte_id <> r2.fonte_id)
               AND r1.ciclo_id BETWEEN ? AND ?{filtro_de_fontes}
               AND r1.valor_centavos IN (
                     SELECT x.valor_centavos FROM registro x
                      WHERE x.valor_centavos < 0 AND {m3} AND x.ciclo_id BETWEEN ? AND ?
                      GROUP BY 1 HAVING count(DISTINCT x.tipo || '|' || x.fonte_id) > 1)
             ORDER BY r1.id, r2.id LIMIT 100",
            m1 = movimento::sql_nao_e_movimento("r1."),
            m2 = movimento::sql_nao_e_movimento("r2."),
            m3 = movimento::sql_nao_e_movimento("x."),
        ),
        &params,
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, i64>(1)?,
                l.get::<_, Centavos>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, Option<String>>(5)?,
                l.get::<_, Option<String>>(6)?,
            ))
        },
    )?;
    let tabela_c = Tabela {
        colunas: vec![
            coluna("Registro"),
            coluna("Data"),
            coluna("Outro registro"),
            coluna("Data do outro"),
            coluna_num("Valor"),
        ],
        linhas: pares
            .into_iter()
            .map(|(a, b, valor, data_a, data_b, desc_a, desc_b)| Linha {
                alvo: Some(Ref::nova(Tipo::Registro, a.to_string())),
                celulas: vec![
                    celula_link(
                        Tipo::Registro,
                        &a.to_string(),
                        &rotulo_do_registro(a, &desc_a),
                    ),
                    Celula::Data(data_a),
                    celula_link(
                        Tipo::Registro,
                        &b.to_string(),
                        &rotulo_do_registro(b, &desc_b),
                    ),
                    Celula::Data(data_b),
                    celula_valor(valor),
                ],
            })
            .collect(),
    };
    Ok(painel(vec![
        secao(
            "Achados com o mesmo texto",
            Some("Arquivos diferentes que descrevem o mesmo fato. Conteúdo idêntico não entra: é o mesmo achado com mais de uma origem.".to_owned()),
            tabela_a,
        ),
        secao(
            "Duplicidades já marcadas",
            Some("Relações entre achados gravadas pelas regras ou por você.".to_owned()),
            tabela_b,
        ),
        secao(
            "Registros com o mesmo valor em datas próximas",
            Some(format!(
                "Mesmo valor, até 5 dias de diferença, de tipo ou fonte diferente; ciclos {} a {}.",
                j.de, j.ate
            )),
            tabela_c,
        ),
    ]))
}

fn o_que_falta_para_confiar(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Painel> {
    let lacunas = vistas::tabela_contas_buracos(&vistas::contas_buracos(ctx, &Selecao::nova())?);
    let faturas: Vec<_> = vistas::faturas_conferencia(ctx, &Selecao::nova())?
        .into_iter()
        .filter(|f| f.situacao != "confere")
        .collect();
    let faturas = vistas::tabela_faturas(&faturas);

    let fontes = consultar(
        ctx.conn,
        "SELECT f.id, f.descricao,
                (SELECT status FROM varredura w WHERE w.fonte_id = f.id
                  ORDER BY (w.status = 'rodando') DESC, w.id DESC LIMIT 1)
         FROM fonte f WHERE f.escopo = 'dentro' ORDER BY f.id",
        &[],
        |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<String>>(2)?,
            ))
        },
    )?;
    let sem_coleta = Tabela {
        colunas: vec![coluna("Conexão"), coluna("Situação")],
        linhas: fontes
            .into_iter()
            .filter_map(|(id, descricao, status)| {
                let situacao = match status.as_deref() {
                    None => "sem coleta",
                    Some("falhou") => "a última coleta falhou",
                    Some("interrompida") => "a última coleta foi interrompida",
                    Some(_) => return None,
                };
                Some(Linha {
                    alvo: Some(Ref::nova(Tipo::Conexao, id.as_str())),
                    celulas: vec![
                        celula_link(Tipo::Conexao, &id, &descricao),
                        celula_texto(situacao),
                    ],
                })
            })
            .collect(),
    };
    let nao_aprovados = vistas::escalar(
        ctx.conn,
        "SELECT count(*) FROM registro WHERE estado IN ('extraido', 'conferido', 'divergente')",
        &[],
    )?;
    let aprovacao = Tabela {
        colunas: vec![coluna("Indicador"), coluna_num("Quantidade")],
        linhas: vec![Linha {
            alvo: None,
            celulas: vec![
                celula_texto("Registros que você ainda não aprovou"),
                celula_contagem(
                    nao_aprovados,
                    ref_de_todos_os_registros(),
                    None,
                    recorte::NAO_APROVADOS,
                ),
            ],
        }],
    };
    Ok(painel(vec![
        secao(
            "Lacunas por conta",
            Some("Meses, dentro do período da conta, em que a coleta não trouxe nada.".to_owned()),
            lacunas,
        ),
        secao(
            "Faturas que não fecham",
            Some("A fatura contra a soma das compras ligadas a ela.".to_owned()),
            faturas,
        ),
        secao(
            "Conexões sem coleta",
            Some(
                "Conexões dentro do escopo sem coleta, ou cuja última coleta não terminou."
                    .to_owned(),
            ),
            sem_coleta,
        ),
        secao("Aprovação", None, aprovacao),
    ]))
}

// ---------------------------------------------------------------------------
// Entrada
// ---------------------------------------------------------------------------

/// Responde uma pergunta do §11. `hoje` é `AAAA-MM-DD`, para o ciclo atual.
pub fn responder(
    ctx: &Contexto<'_>,
    pergunta: Pergunta,
    fonte: Fonte,
    filtro: &Filtro,
    hoje: &str,
) -> ResultadoDeConsulta<Painel> {
    match fonte {
        Fonte::Inventario => responder_com(
            &GastosDoInventario { ctx: *ctx },
            ctx,
            pergunta,
            filtro,
            hoje,
        ),
        Fonte::LivroCaixa => Err(ErroDeConsulta::NaoSuportado(
            Tipo::Movimentacao
                .motivo_da_fatia()
                .unwrap_or_else(|| "Chega na fatia 2".to_owned()),
        )),
    }
}

/// O código das 8 perguntas, sobre qualquer [`FonteDeGastos`]. `ctx` serve só ao que é
/// próprio do inventário (qualidade da coleta, achados, duplicidades).
pub fn responder_com(
    gastos: &dyn FonteDeGastos,
    ctx: &Contexto<'_>,
    pergunta: Pergunta,
    filtro: &Filtro,
    hoje: &str,
) -> ResultadoDeConsulta<Painel> {
    let janela = Janela::do_filtro(filtro, hoje);
    match pergunta {
        Pergunta::QuantoSaiu => quanto_saiu(gastos, filtro, &janela),
        Pergunta::MaioresFornecedores => maiores_fornecedores(gastos, filtro, &janela),
        Pergunta::OQueSeRepete => o_que_se_repete(gastos, filtro, &janela),
        Pergunta::OQueAumentou => o_que_aumentou(gastos, filtro, &janela),
        Pergunta::OndeSeConcentra => onde_se_concentra(gastos, filtro, &janela),
        Pergunta::NaoClassificado => nao_classificado(ctx, filtro, &janela),
        Pergunta::PossiveisDuplicidades => possiveis_duplicidades(ctx, filtro, &janela),
        Pergunta::OQueFaltaParaConfiar => o_que_falta_para_confiar(ctx),
    }
}
