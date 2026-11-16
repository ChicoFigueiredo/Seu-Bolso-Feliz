//! Prévia e confirmação da importação de uma aba mensal.
//!
//! A prévia não grava nada. A confirmação aplica tudo por `Planejamento::aplicar` com
//! `Autor::Importacao`. A idempotência é por `(sha256 do arquivo, aba, linha)`: reimportar o
//! mesmo arquivo não cria nada. Um arquivo novo casa por nome com o que já existe e gera
//! diff; nunca sobrescreve uma versão que o usuário decidiu.

use std::collections::HashSet;

use sbf_planejamento::{
    Alteracao, Autor, Centavos, Certeza, Competencia, Composicao, Data, Escopo, EstadoDaOcorrencia,
    Excecao, Fonte, Frequencia, Grupo, IdOcorrencia, MudancaDeVersao, Natureza, NovoRecorrente,
    OrigemImportada, Planejamento, Prazo, Recorrente, TipoDeRubrica, Versao,
};

use crate::Resultado;
use crate::leitura::{AbaLida, LinhaDaPlanilha};
use crate::mapa::{Extra, ItemDoMapa, Mapa, natureza_de_texto};
use crate::texto::normalizar;

/// Vencimento mais antigo que isto, em meses antes da competência, é resto antigo (D14).
pub const MESES_DE_RESTO_ANTIGO: i32 = 6;
/// Dia do salário quando nem a planilha nem o mapa dizem.
const DIA_PADRAO_DO_SALARIO: u8 = 1;
const NOME_DO_SALARIO: &str = "Salário";
const LINHA_DO_SALARIO: u32 = 3;

/// O pago informado da coluna J, para virar `Excecao::PagoInformado`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagoDoMes {
    pub competencia: Competencia,
    pub valor: Centavos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemNovo {
    pub linha: u32,
    pub recorrente: NovoRecorrente,
    pub origem: OrigemImportada,
    pub pago: Option<PagoDoMes>,
    /// A competência da ocorrência a adiar sem data (previsto zero com unitário e obs).
    pub adiar: Option<Competencia>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CasadoPor {
    /// A mesma origem `(sha256, aba, linha)`: já foi importado.
    Origem,
    /// O mesmo nome (e grupo), vindo de outro arquivo.
    Nome,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemExistente {
    pub linha: u32,
    pub nome: String,
    pub recorrente: String,
    pub por: CasadoPor,
    /// Só nos casados por nome: o pago do arquivo novo.
    pub pago: Option<PagoDoMes>,
    pub adiar: Option<Competencia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemAtualizavel {
    pub linha: u32,
    pub nome: String,
    pub recorrente: String,
    pub versao_base: i64,
    pub valor_atual: Option<Centavos>,
    pub valor_novo: Centavos,
    pub desde: Competencia,
    pub pago: Option<PagoDoMes>,
    pub adiar: Option<Competencia>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflito {
    pub linha: u32,
    pub nome: String,
    pub recorrente: String,
    pub motivo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignorado {
    pub linha: u32,
    pub motivo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Faltante {
    pub linha: u32,
    pub nome: String,
    pub o_que: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Previa {
    pub aba: String,
    pub sha256: String,
    pub competencia: Competencia,
    /// Os grupos que ainda não existem, na ordem da planilha.
    pub grupos_novos: Vec<Grupo>,
    pub novos: Vec<ItemNovo>,
    pub existentes: Vec<ItemExistente>,
    pub atualizaveis: Vec<ItemAtualizavel>,
    pub conflitos: Vec<Conflito>,
    pub ignorados: Vec<Ignorado>,
    pub faltantes: Vec<Faltante>,
    /// As rubricas do painel lateral, para o recorrente do salário.
    pub composicao: Option<ComposicaoPrevia>,
}

/// As rubricas do painel, antes de saber o id do recorrente do salário.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposicaoPrevia {
    pub desde: Competencia,
    pub origem: String,
    pub rubricas: Vec<RubricaPrevia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RubricaPrevia {
    pub rubrica: sbf_planejamento::Rubrica,
    /// O nome do item da grade (desconto em folha) a que a consignação se liga.
    pub ligar_a: Option<String>,
}

impl ComposicaoPrevia {
    pub fn liquido(&self) -> Centavos {
        Centavos(
            self.rubricas
                .iter()
                .map(|r| match r.rubrica.tipo {
                    TipoDeRubrica::Provento => r.rubrica.valor.0,
                    TipoDeRubrica::Desconto => -r.rubrica.valor.0,
                })
                .sum(),
        )
    }
}

impl Previa {
    /// Só contagens, para a saída padrão e o log.
    pub fn contagens(&self) -> String {
        format!(
            "novos={} existentes={} atualizaveis={} conflitos={} ignorados={} faltantes={} grupos_novos={}",
            self.novos.len(),
            self.existentes.len(),
            self.atualizaveis.len(),
            self.conflitos.len(),
            self.ignorados.len(),
            self.faltantes.len(),
            self.grupos_novos.len()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Confirmacao {
    pub grupos: u32,
    pub criados: u32,
    pub atualizados: u32,
    pub pagos_informados: u32,
    pub adiadas: u32,
    /// Composições (rubricas) do salário gravadas.
    pub composicoes: u32,
}

/// O que se sabe de uma linha (ou do salário) antes de decidir criar, casar ou atualizar.
struct Candidato {
    linha: u32,
    nome: String,
    grupo: String,
    posicao: i64,
    natureza: Natureza,
    prazo: Prazo,
    versao: Versao,
    pago: Option<PagoDoMes>,
    adiar: Option<Competencia>,
    /// Origem própria (os extras do mapa); as linhas usam `(sha256, aba, linha)`.
    origem: Option<OrigemImportada>,
}

pub fn previa(
    aba: &AbaLida,
    sha256: &str,
    plan: &dyn Planejamento,
    competencia: Competencia,
) -> Resultado<Previa> {
    previa_com_mapa(aba, sha256, plan, competencia, &Mapa::vazio())
}

pub fn previa_com_mapa(
    aba: &AbaLida,
    sha256: &str,
    plan: &dyn Planejamento,
    competencia: Competencia,
    mapa: &Mapa,
) -> Resultado<Previa> {
    let mut faltantes = Vec::new();
    let mut candidatos = Vec::new();
    if let Some(c) = candidato_do_salario(aba, mapa, competencia, &mut faltantes) {
        candidatos.push(c);
    }
    for (i, l) in aba.linhas.iter().enumerate() {
        candidatos.push(candidato_da_linha(
            aba,
            l,
            i as i64 + 1,
            mapa,
            competencia,
            &mut faltantes,
        ));
    }
    let mut posicao = aba.linhas.len() as i64;
    for (i, e) in mapa.extras.iter().enumerate() {
        posicao += 1;
        candidatos.push(candidato_do_extra(
            i,
            e,
            posicao,
            competencia,
            &mut faltantes,
        ));
    }
    let composicao = composicao_da_aba(aba, &candidatos, competencia, &mut faltantes);
    let ignorados = aba
        .subtotais
        .iter()
        .map(|s| Ignorado {
            linha: s.linha,
            motivo: "subtotal ou cabeçalho de grupo".to_owned(),
        })
        .collect();

    // Grupos novos, na ordem da planilha.
    let grupos_existentes = plan.grupos()?;
    let mut base = grupos_existentes
        .iter()
        .map(|g| g.posicao + 1)
        .max()
        .unwrap_or(0);
    let mut vistos: HashSet<String> = grupos_existentes
        .iter()
        .map(|g| normalizar(&g.nome))
        .collect();
    let mut grupos_novos = Vec::new();
    for c in &candidatos {
        if vistos.insert(normalizar(&c.grupo)) {
            grupos_novos.push(Grupo {
                nome: c.grupo.clone(),
                posicao: base,
            });
            base += 1;
        }
    }

    let existentes_no_banco = plan.recorrentes(false)?;
    let mut usados: HashSet<String> = HashSet::new();
    let mut p = Previa {
        aba: aba.nome.clone(),
        sha256: sha256.to_owned(),
        competencia,
        grupos_novos,
        novos: Vec::new(),
        existentes: Vec::new(),
        atualizaveis: Vec::new(),
        conflitos: Vec::new(),
        ignorados,
        faltantes,
        composicao,
    };

    let hoje = Data::hoje_em_sao_paulo();
    for c in candidatos {
        let origem = c.origem.clone().unwrap_or_else(|| OrigemImportada {
            arquivo_sha256: sha256.to_owned(),
            aba: aba.nome.clone(),
            linha: c.linha,
        });
        if let Some(id) = plan.origem_importada(&origem)? {
            usados.insert(id.clone());
            let adiar = pendente_de_adiar(plan, &id, c.adiar, hoje)?;
            p.existentes.push(ItemExistente {
                linha: c.linha,
                nome: c.nome,
                recorrente: id,
                por: CasadoPor::Origem,
                pago: None,
                adiar,
            });
            continue;
        }
        let Some(r) = casar_por_nome(&existentes_no_banco, &c, &usados) else {
            p.novos.push(ItemNovo {
                linha: c.linha,
                recorrente: NovoRecorrente {
                    nome: c.nome,
                    grupo: c.grupo,
                    subgrupo: None,
                    posicao: c.posicao,
                    natureza: c.natureza,
                    prazo: c.prazo,
                    primeira_versao: c.versao,
                },
                origem,
                pago: c.pago,
                adiar: c.adiar,
            });
            continue;
        };
        usados.insert(r.id.clone());
        let adiar = pendente_de_adiar(plan, &r.id, c.adiar, hoje)?;
        let vigente = versao_vigente(r, competencia);
        match (c.versao.valor, vigente.and_then(|v| v.valor)) {
            (Some(novo), atual) if Some(novo) != atual => {
                if vigente.is_some_and(|v| v.fonte == Fonte::Usuario) {
                    p.conflitos.push(Conflito {
                        linha: c.linha,
                        nome: c.nome,
                        recorrente: r.id.clone(),
                        motivo: "o valor atual foi decidido por você; o arquivo traz outro"
                            .to_owned(),
                    });
                } else {
                    p.atualizaveis.push(ItemAtualizavel {
                        linha: c.linha,
                        nome: c.nome,
                        recorrente: r.id.clone(),
                        versao_base: r.versao_do_registro,
                        valor_atual: atual,
                        valor_novo: novo,
                        desde: competencia,
                        pago: c.pago,
                        adiar,
                    });
                }
            }
            _ => p.existentes.push(ItemExistente {
                linha: c.linha,
                nome: c.nome,
                recorrente: r.id.clone(),
                por: CasadoPor::Nome,
                pago: c.pago,
                adiar,
            }),
        }
    }
    Ok(p)
}

fn versao_vigente(r: &Recorrente, c: Competencia) -> Option<&Versao> {
    r.versoes
        .iter()
        .rev()
        .find(|v| v.desde <= c)
        .or_else(|| r.versoes.first())
}

/// Casa pelo nome normalizado: primeiro com o mesmo grupo, depois só pelo nome. Cada
/// recorrente existente casa uma vez por prévia (duas linhas iguais não casam com o mesmo).
fn casar_por_nome<'a>(
    existentes: &'a [Recorrente],
    c: &Candidato,
    usados: &HashSet<String>,
) -> Option<&'a Recorrente> {
    let nome = normalizar(&c.nome);
    let grupo = normalizar(&c.grupo);
    let mut mesmos = existentes
        .iter()
        .filter(|r| !usados.contains(&r.id) && normalizar(&r.nome) == nome);
    let primeiro = mesmos.clone().find(|r| normalizar(&r.grupo) == grupo);
    primeiro.or_else(|| mesmos.next())
}

/// Classifica pelas flags do painel; sem flag, pelo código do contracheque.
fn tipo_da_rubrica(r: &crate::leitura::Rubrica) -> Option<TipoDeRubrica> {
    let ligada = |nome: &str| r.flags.iter().any(|(n, on)| *on && normalizar(n) == nome);
    if ligada("sal") {
        return Some(TipoDeRubrica::Provento);
    }
    if ["consig", "saude", "inss", "prev", "ir"]
        .iter()
        .any(|n| ligada(n))
    {
        return Some(TipoDeRubrica::Desconto);
    }
    match r.codigo.chars().next() {
        Some('2') => Some(TipoDeRubrica::Provento),
        Some('3' | '4') => Some(TipoDeRubrica::Desconto),
        _ => None,
    }
}

fn palavras_fortes(nome: &str) -> Vec<String> {
    normalizar(nome)
        .split(' ')
        .filter(|p| p.len() >= 4)
        .map(str::to_owned)
        .collect()
}

/// O item de desconto em folha da grade que casa com a consignação: mesmo valor e nome
/// parecido; na falta, o único com o mesmo valor.
fn item_da_consignacao(candidatos: &[Candidato], nome: &str, valor: Centavos) -> Option<String> {
    let folha: Vec<&Candidato> = candidatos
        .iter()
        .filter(|c| c.natureza == Natureza::DescontoEmFolha && c.versao.valor == Some(valor))
        .collect();
    let n = normalizar(nome);
    let fortes = palavras_fortes(nome);
    let parecido = folha.iter().find(|c| {
        let cn = normalizar(&c.nome);
        cn.contains(&n)
            || n.contains(&cn)
            || palavras_fortes(&c.nome).iter().any(|p| fortes.contains(p))
    });
    match (parecido, folha.as_slice()) {
        (Some(c), _) => Some(c.nome.clone()),
        (None, [unico]) => Some(unico.nome.clone()),
        _ => None,
    }
}

fn composicao_da_aba(
    aba: &AbaLida,
    candidatos: &[Candidato],
    competencia: Competencia,
    faltantes: &mut Vec<Faltante>,
) -> Option<ComposicaoPrevia> {
    aba.salario.as_ref()?;
    let mut rubricas = Vec::new();
    let mut converter = |r: &crate::leitura::Rubrica, consignacao: bool| {
        let nome = if r.nome.is_empty() {
            r.nome_painel.clone().unwrap_or_default()
        } else {
            r.nome.clone()
        };
        let tipo = if consignacao {
            Some(TipoDeRubrica::Desconto)
        } else {
            tipo_da_rubrica(r)
        };
        let (Some(tipo), Some(valor)) = (tipo, r.valor.or(r.valores[0])) else {
            faltantes.push(Faltante {
                linha: r.linha,
                nome,
                o_que: "rubrica do painel sem tipo ou sem valor (fora da composição)".to_owned(),
            });
            return;
        };
        let ligar_a = (consignacao && tipo == TipoDeRubrica::Desconto)
            .then(|| item_da_consignacao(candidatos, &nome, valor))
            .flatten();
        let comp = r.competencia.as_deref().and_then(|c| {
            let (a, m) = c.split_once('/')?;
            Competencia::nova(a.parse().ok()?, m.parse().ok()?)
        });
        rubricas.push(RubricaPrevia {
            rubrica: sbf_planejamento::Rubrica {
                codigo: Some(r.codigo.clone()),
                nome,
                tipo,
                valor,
                competencia: comp,
                prazo: None,
                recorrente: None,
            },
            ligar_a,
        });
    };
    for r in &aba.rubricas {
        converter(r, false);
    }
    for r in &aba.consignacoes {
        converter(r, true);
    }
    (!rubricas.is_empty()).then(|| ComposicaoPrevia {
        desde: competencia,
        origem: format!("planilha {}", aba.nome),
        rubricas,
    })
}

fn candidato_do_salario(
    aba: &AbaLida,
    mapa: &Mapa,
    competencia: Competencia,
    faltantes: &mut Vec<Faltante>,
) -> Option<Candidato> {
    let s = aba.salario.as_ref()?;
    let item = mapa.item(NOME_DO_SALARIO);
    let soma_consignacoes: Option<Centavos> = aba
        .consignacoes
        .iter()
        .map(|c| c.valor.or(c.valores[0]).map(|v| v.0))
        .sum::<Option<i64>>()
        .map(Centavos);
    let liquido = match (s.previsto, soma_consignacoes) {
        (Some(bruto), Some(desc)) => Some(Centavos(bruto.0 - desc.0)),
        _ => None,
    };
    if liquido.is_none() {
        faltantes.push(Faltante {
            linha: LINHA_DO_SALARIO,
            nome: NOME_DO_SALARIO.to_owned(),
            o_que: "salário líquido: falta o salário ou alguma consignação sem valor".to_owned(),
        });
    }
    let dia = item.and_then(|i| i.dia).unwrap_or_else(|| {
        faltantes.push(Faltante {
            linha: LINHA_DO_SALARIO,
            nome: NOME_DO_SALARIO.to_owned(),
            o_que: "dia do salário (usei o dia 1)".to_owned(),
        });
        DIA_PADRAO_DO_SALARIO
    });
    Some(Candidato {
        linha: LINHA_DO_SALARIO,
        nome: NOME_DO_SALARIO.to_owned(),
        grupo: NOME_DO_SALARIO.to_owned(),
        posicao: 0,
        natureza: Natureza::Entrada,
        prazo: Prazo::SemFim,
        versao: Versao {
            id: String::new(),
            desde: competencia,
            valor: liquido,
            certeza: Certeza::Estimado,
            fonte: Fonte::Planilha,
            frequencia: Frequencia::Mensal { dia },
            conta: None,
            centro: item.and_then(|i| i.centro.clone()),
            fornecedor: item.and_then(|i| i.fornecedor.clone()),
            categoria: None,
            nota: Some("líquido: salário menos as consignações".to_owned()),
        },
        pago: None,
        adiar: None,
        origem: None,
    })
}

fn candidato_do_extra(
    indice: usize,
    e: &Extra,
    posicao: i64,
    competencia: Competencia,
    faltantes: &mut Vec<Faltante>,
) -> Candidato {
    let valor = e.valor_centavos.map(Centavos);
    if valor.is_none() {
        faltantes.push(Faltante {
            linha: indice as u32,
            nome: e.nome.clone(),
            o_que: "extra sem valor".to_owned(),
        });
    }
    Candidato {
        linha: indice as u32,
        nome: e.nome.clone(),
        grupo: e.grupo.clone(),
        posicao,
        natureza: natureza_de_texto(&e.natureza).unwrap_or(Natureza::Saida),
        prazo: Prazo::SemFim,
        versao: Versao {
            id: String::new(),
            desde: competencia,
            valor,
            certeza: if e.certeza == "confirmado" {
                Certeza::Confirmado
            } else {
                Certeza::Estimado
            },
            fonte: Fonte::Usuario,
            frequencia: Frequencia::Mensal { dia: e.dia },
            conta: None,
            centro: e.centro.clone(),
            fornecedor: e.fornecedor.clone(),
            categoria: None,
            nota: Some("extra do mapa".to_owned()),
        },
        pago: None,
        adiar: None,
        origem: Some(OrigemImportada {
            arquivo_sha256: e.sha256(),
            aba: "mapa".to_owned(),
            linha: indice as u32,
        }),
    }
}

fn candidato_da_linha(
    aba: &AbaLida,
    l: &LinhaDaPlanilha,
    posicao: i64,
    mapa: &Mapa,
    competencia: Competencia,
    faltantes: &mut Vec<Faltante>,
) -> Candidato {
    let item: Option<&ItemDoMapa> = mapa.item(&l.descricao);
    let natureza = item
        .and_then(|i| i.natureza.as_deref())
        .and_then(natureza_de_texto)
        .unwrap_or_else(|| {
            if l.meio
                .as_deref()
                .is_some_and(|m| aba.meio_e_desconto_em_folha(m))
            {
                Natureza::DescontoEmFolha
            } else {
                Natureza::Saida
            }
        });
    if l.previsto.is_none() {
        let o_que = if l.erros.iter().any(|e| e.coluna == "I") {
            "previsto com erro de fórmula"
        } else {
            "previsto sem valor"
        };
        faltantes.push(Faltante {
            linha: l.linha,
            nome: l.descricao.clone(),
            o_que: o_que.to_owned(),
        });
    }
    if l.vencimento.is_none() {
        faltantes.push(Faltante {
            linha: l.linha,
            nome: l.descricao.clone(),
            o_que: "vencimento ausente (usei o dia 1)".to_owned(),
        });
    }

    // A competência da ocorrência: a do vencimento, nunca antes do mês importado.
    let venc_comp = l.vencimento.map(Competencia::de_data);
    let resto_antigo = venc_comp.is_some_and(|c| c.meses_ate(competencia) > MESES_DE_RESTO_ANTIGO);
    let ocorrencia = match venc_comp {
        Some(c) if !resto_antigo => c.max(competencia),
        Some(c) => c,
        None => competencia,
    };
    let (frequencia, prazo) = match (l.vencimento, resto_antigo) {
        (Some(data), true) => (Frequencia::Unica { data }, Prazo::SemFim),
        (venc, _) => {
            let dia = venc.map_or(1, |d| d.dia);
            let prazo = match item.and_then(|i| i.parcelas_que_faltam) {
                Some(total) if total > 0 => Prazo::Parcelas {
                    total,
                    primeira: ocorrencia,
                },
                _ => Prazo::Desconhecido,
            };
            (Frequencia::Mensal { dia }, prazo)
        }
    };
    // Previsto zero nunca vira regra de valor 0: com unitário e observação o valor é o
    // unitário e a ocorrência fica adiada (em atraso); sem unitário, fica sem valor.
    let (valor, adiar) = match l.previsto {
        Some(Centavos(0)) => match (l.unitario, &l.obs) {
            (Some(u), Some(_)) if u.0 > 0 => (Some(u), Some(ocorrencia)),
            _ => {
                faltantes.push(Faltante {
                    linha: l.linha,
                    nome: l.descricao.clone(),
                    o_que: "previsto zero".to_owned(),
                });
                (None, None)
            }
        },
        outro => (outro, None),
    };
    // O usuário declarou (no mapa) que a ocorrência do mês está devida.
    let adiar = adiar.or(item.filter(|i| i.em_atraso).map(|_| ocorrencia));
    let certeza = if valor.is_some() && l.previsto != Some(Centavos(0)) && l.erros.is_empty() {
        Certeza::Confirmado
    } else {
        Certeza::Estimado
    };
    let pago = l.pago.filter(|p| p.0 > 0).map(|valor| PagoDoMes {
        competencia: ocorrencia,
        valor,
    });
    Candidato {
        linha: l.linha,
        nome: l.descricao.clone(),
        grupo: l.grupo.clone(),
        posicao,
        natureza,
        prazo,
        versao: Versao {
            id: String::new(),
            desde: ocorrencia,
            valor,
            certeza,
            fonte: Fonte::Planilha,
            frequencia,
            conta: l.meio.clone(),
            centro: item.and_then(|i| i.centro.clone()),
            fornecedor: item.and_then(|i| i.fornecedor.clone()),
            categoria: None,
            nota: if resto_antigo {
                Some("resto antigo da planilha".to_owned())
            } else {
                l.obs.clone()
            },
        },
        pago,
        adiar,
        origem: None,
    }
}

fn excecao_de_pago(recorrente: &str, prazo: &Prazo, pago: &PagoDoMes) -> Alteracao {
    let parcela = match prazo {
        Prazo::Parcelas { primeira, .. } if *primeira == pago.competencia => Some(1),
        _ => None,
    };
    Alteracao::Excecao {
        ocorrencia: IdOcorrencia {
            recorrente: recorrente.to_owned(),
            competencia: pago.competencia,
            parcela,
        },
        excecao: Excecao::PagoInformado {
            valor: pago.valor,
            data: None::<Data>,
        },
    }
}

/// A ocorrência a adiar, a menos que o planejamento já a mostre como adiada (idempotência:
/// o trait não lista exceções, então olha o estado da ocorrência em `proximas`).
fn pendente_de_adiar(
    plan: &dyn Planejamento,
    recorrente: &str,
    comp: Option<Competencia>,
    hoje: Data,
) -> Resultado<Option<Competencia>> {
    let Some(comp) = comp else { return Ok(None) };
    // Uma exceção Adiar ativa naquela competência já basta (um adiamento sem data vencido
    // aparece como EmAtraso, não Adiada, então o estado sozinho não serve).
    let ja_gravado = plan.excecoes_do_recorrente(recorrente)?.iter().any(|e| {
        !e.desfeita
            && e.ocorrencia.competencia == comp
            && matches!(e.excecao, sbf_planejamento::Excecao::Adiar { .. })
    });
    let ja = ja_gravado
        || plan
            .proximas(recorrente, comp, 1, hoje)?
            .iter()
            .any(|o| o.id.competencia == comp && o.estado == EstadoDaOcorrencia::Adiada);
    Ok((!ja).then_some(comp))
}

fn prazo_de(plan: &dyn Planejamento, recorrente: &str) -> Resultado<Prazo> {
    Ok(plan
        .recorrente(recorrente)?
        .map_or(Prazo::Desconhecido, |r| r.prazo))
}

fn adiar_sem_data(
    plan: &mut dyn Planejamento,
    recorrente: &str,
    prazo: &Prazo,
    competencia: Competencia,
) -> Resultado<()> {
    let parcela = match prazo {
        Prazo::Parcelas { primeira, .. } if *primeira == competencia => Some(1),
        _ => None,
    };
    plan.aplicar(
        Alteracao::Excecao {
            ocorrencia: IdOcorrencia {
                recorrente: recorrente.to_owned(),
                competencia,
                parcela,
            },
            excecao: Excecao::Adiar { nova_data: None },
        },
        Autor::Importacao,
    )?;
    Ok(())
}

/// Aplica a prévia: grupos, recorrentes novos, atualizações e pagos informados.
pub fn confirmar(previa: &Previa, plan: &mut dyn Planejamento) -> Resultado<Confirmacao> {
    let mut c = Confirmacao::default();
    // Nome do item da grade -> id do recorrente, para ligar as consignações e achar o salário.
    let mut ids: Vec<(String, String)> = Vec::new();
    let mut salario_id: Option<String> = None;
    for g in &previa.grupos_novos {
        plan.aplicar(
            Alteracao::CriarGrupo {
                nome: g.nome.clone(),
                posicao: g.posicao,
            },
            Autor::Importacao,
        )?;
        c.grupos += 1;
    }
    for n in &previa.novos {
        let recibo = plan.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: n.recorrente.clone(),
                origem: Some(n.origem.clone()),
            },
            Autor::Importacao,
        )?;
        c.criados += 1;
        ids.push((n.recorrente.nome.clone(), recibo.alvo.clone()));
        if n.linha == LINHA_DO_SALARIO
            && n.recorrente.nome == NOME_DO_SALARIO
            && n.recorrente.natureza == Natureza::Entrada
        {
            salario_id = Some(recibo.alvo.clone());
        }
        if let Some(p) = &n.pago {
            plan.aplicar(
                excecao_de_pago(&recibo.alvo, &n.recorrente.prazo, p),
                Autor::Importacao,
            )?;
            c.pagos_informados += 1;
        }
        if let Some(comp) = n.adiar {
            adiar_sem_data(plan, &recibo.alvo, &n.recorrente.prazo, comp)?;
            c.adiadas += 1;
        }
    }
    for e in &previa.existentes {
        ids.push((e.nome.clone(), e.recorrente.clone()));
        if e.linha == LINHA_DO_SALARIO && e.nome == NOME_DO_SALARIO {
            salario_id = Some(e.recorrente.clone());
        }
        if let (CasadoPor::Nome, Some(p)) = (e.por, &e.pago) {
            plan.aplicar(
                excecao_de_pago(&e.recorrente, &Prazo::Desconhecido, p),
                Autor::Importacao,
            )?;
            c.pagos_informados += 1;
        }
        if let Some(comp) = e.adiar {
            let prazo = prazo_de(plan, &e.recorrente)?;
            adiar_sem_data(plan, &e.recorrente, &prazo, comp)?;
            c.adiadas += 1;
        }
    }
    for a in &previa.atualizaveis {
        ids.push((a.nome.clone(), a.recorrente.clone()));
        if a.linha == LINHA_DO_SALARIO && a.nome == NOME_DO_SALARIO {
            salario_id = Some(a.recorrente.clone());
        }
        plan.aplicar(
            Alteracao::Editar {
                recorrente: a.recorrente.clone(),
                escopo: Escopo::DaquiEmDiante(a.desde),
                mudanca: MudancaDeVersao {
                    valor: Some(Some(a.valor_novo)),
                    fonte: Some(Fonte::Planilha),
                    ..MudancaDeVersao::default()
                },
                versao_base: a.versao_base,
            },
            Autor::Importacao,
        )?;
        c.atualizados += 1;
        if let Some(comp) = a.adiar {
            let prazo = prazo_de(plan, &a.recorrente)?;
            adiar_sem_data(plan, &a.recorrente, &prazo, comp)?;
            c.adiadas += 1;
        }
        if let Some(p) = &a.pago {
            plan.aplicar(
                excecao_de_pago(&a.recorrente, &Prazo::Desconhecido, p),
                Autor::Importacao,
            )?;
            c.pagos_informados += 1;
        }
    }
    if let (Some(comp), Some(salario)) = (&previa.composicao, salario_id) {
        let composicao = Composicao {
            recorrente: salario.clone(),
            desde: comp.desde,
            rubricas: comp
                .rubricas
                .iter()
                .map(|r| {
                    let mut rubrica = r.rubrica.clone();
                    rubrica.recorrente = r.ligar_a.as_ref().and_then(|nome| {
                        ids.iter()
                            .find(|(n, _)| n == nome)
                            .map(|(_, id)| id.clone())
                    });
                    rubrica
                })
                .collect(),
            documento: None,
            origem: comp.origem.clone(),
        };
        // Idempotente: a mesma composição na mesma competência não é regravada.
        let igual = plan.composicao(&salario, comp.desde)?.is_some_and(|atual| {
            atual.desde == composicao.desde && atual.rubricas == composicao.rubricas
        });
        if !igual {
            let versao_base = plan
                .recorrente(&salario)?
                .map_or(1, |r| r.versao_do_registro);
            plan.aplicar(
                Alteracao::DefinirComposicao {
                    composicao,
                    versao_base,
                },
                Autor::Importacao,
            )?;
            c.composicoes += 1;
        }
    }
    Ok(c)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::falso::Falso;
    use crate::fixtura::{ABA, planilha_ficticia};
    use crate::leitura::ler_aba;

    const SHA: &str = "aaaa";
    const COMP: Competencia = Competencia { ano: 2026, mes: 11 };

    fn aba() -> AbaLida {
        let pasta = tempfile::tempdir().expect("pasta");
        let arq = pasta.path().join("p.xlsx");
        planilha_ficticia(&arq);
        ler_aba(&arq, ABA).expect("aba")
    }

    fn por_nome<'a>(p: &'a Previa, nome: &str) -> &'a ItemNovo {
        p.novos
            .iter()
            .find(|n| n.recorrente.nome == nome)
            .expect(nome)
    }

    // AC-03
    #[test]
    fn previa_reconhece_linhas_grupos_e_erros_sem_gravar() {
        let plan = Falso::default();
        let p = previa(&aba(), SHA, &plan, COMP).expect("previa");
        // 8 linhas da grade + o salário.
        assert_eq!(p.novos.len(), 9);
        assert_eq!(p.ignorados.len(), 1);
        assert!(p.existentes.is_empty() && p.atualizaveis.is_empty() && p.conflitos.is_empty());
        let nomes: Vec<&str> = p.grupos_novos.iter().map(|g| g.nome.as_str()).collect();
        assert_eq!(nomes, ["Salário", "Essencial", "Lar", "Parcial"]);
        let pos: Vec<i64> = p.grupos_novos.iter().map(|g| g.posicao).collect();
        assert_eq!(pos, [0, 1, 2, 3]);
        // Erro de fórmula: sem valor (nunca zero), estimado, e apontado nos faltantes.
        let net = por_nome(&p, "Internet Exemplo");
        assert_eq!(net.recorrente.primeira_versao.valor, None);
        assert_eq!(net.recorrente.primeira_versao.certeza, Certeza::Estimado);
        assert!(
            p.faltantes
                .iter()
                .any(|f| f.nome == "Internet Exemplo" && f.o_que.contains("erro de fórmula"))
        );
        assert!(p.faltantes.iter().any(|f| f.nome == "Sem Previsto"));
        // Nada foi gravado.
        assert!(plan.aplicadas.is_empty());
        // Ordem da planilha vira posição.
        assert!(
            por_nome(&p, "Aluguel Fictício").recorrente.posicao
                < por_nome(&p, "Luz Exemplo").recorrente.posicao
        );
    }

    #[test]
    fn mapeamentos_pago_resto_antigo_e_frequencia() {
        let plan = Falso::default();
        let p = previa(&aba(), SHA, &plan, COMP).expect("previa");
        let aluguel = por_nome(&p, "Aluguel Fictício");
        assert_eq!(aluguel.recorrente.natureza, Natureza::Saida);
        assert_eq!(aluguel.recorrente.prazo, Prazo::Desconhecido);
        assert_eq!(
            aluguel.recorrente.primeira_versao.frequencia,
            Frequencia::Mensal { dia: 5 }
        );
        assert_eq!(
            aluguel.recorrente.primeira_versao.conta.as_deref(),
            Some("CX-0630")
        );
        assert_eq!(
            aluguel.recorrente.primeira_versao.certeza,
            Certeza::Confirmado
        );
        assert_eq!(aluguel.recorrente.primeira_versao.fonte, Fonte::Planilha);
        assert_eq!(
            aluguel.pago,
            Some(PagoDoMes {
                competencia: COMP,
                valor: Centavos(150_000)
            })
        );
        assert_eq!(
            aluguel.origem,
            OrigemImportada {
                arquivo_sha256: SHA.into(),
                aba: ABA.into(),
                linha: 7
            }
        );
        // Resto antigo: evento único, sem recorrência.
        let velha = por_nome(&p, "Fatura Velha");
        assert_eq!(
            velha.recorrente.primeira_versao.frequencia,
            Frequencia::Unica {
                data: Data::nova(2026, 3, 10).expect("data")
            }
        );
        assert_eq!(velha.recorrente.prazo, Prazo::SemFim);
        assert!(!matches!(
            velha.recorrente.primeira_versao.frequencia,
            Frequencia::Mensal { .. }
        ));
        // 0,1 + 0,2 vira 30 centavos.
        assert_eq!(
            por_nome(&p, "Taxa Soma").recorrente.primeira_versao.valor,
            Some(Centavos(30))
        );
    }

    // AC-06 e D3
    #[test]
    fn consignacao_e_desconto_em_folha_e_salario_entra_liquido() {
        let plan = Falso::default();
        let p = previa(&aba(), SHA, &plan, COMP).expect("previa");
        assert_eq!(
            por_nome(&p, "Plano de Saúde Exemplo").recorrente.natureza,
            Natureza::DescontoEmFolha
        );
        assert_eq!(
            por_nome(&p, "Empréstimo Consignado").recorrente.natureza,
            Natureza::DescontoEmFolha
        );
        let sal = por_nome(&p, "Salário");
        assert_eq!(sal.recorrente.natureza, Natureza::Entrada);
        // 5.000,00 menos 250,00 e 400,00.
        assert_eq!(
            sal.recorrente.primeira_versao.valor,
            Some(Centavos(435_000))
        );
        assert_eq!(sal.recorrente.primeira_versao.certeza, Certeza::Estimado);
        assert_eq!(sal.origem.linha, 3);
        // A soma das saídas de caixa não conta os descontos em folha.
        let caixa: i64 = p
            .novos
            .iter()
            .filter(|n| n.recorrente.natureza == Natureza::Saida)
            .filter_map(|n| n.recorrente.primeira_versao.valor)
            .map(|v| v.0)
            .sum();
        assert_eq!(caixa, 150_000 + 18_050 + 9_000 + 30);
    }

    // Rubricas do painel (CEO: salário digitado, rubricas explicam o líquido)
    #[test]
    fn rubricas_do_painel_viram_composicao_do_salario_idempotente() {
        let mut plan = Falso::default();
        let a = aba();
        let p = previa(&a, SHA, &plan, COMP).expect("previa");
        let comp = p.composicao.as_ref().expect("composicao");
        // Líquido das rubricas confere com o salário líquido importado.
        assert_eq!(
            Some(comp.liquido()),
            por_nome(&p, "Salário").recorrente.primeira_versao.valor
        );
        confirmar(&p, &mut plan).expect("confirmar");
        assert_eq!(plan.composicoes.len(), 1);
        let c = &plan.composicoes[0];
        let sal = plan
            .recorrentes
            .iter()
            .find(|r| r.nome == "Salário")
            .expect("sal");
        assert_eq!(c.recorrente, sal.id);
        assert_eq!(c.desde, COMP);
        assert_eq!(c.origem, "planilha 2026.11");
        assert_eq!(c.rubricas.len(), 4);
        assert_eq!(c.liquido(), sal.versoes[0].valor.expect("valor"));
        let tipo = |cod: &str| {
            c.rubricas
                .iter()
                .find(|r| r.codigo.as_deref() == Some(cod))
                .expect(cod)
        };
        assert_eq!(tipo("001").tipo, TipoDeRubrica::Provento);
        assert_eq!(tipo("210").tipo, TipoDeRubrica::Desconto);
        let plano = plan
            .recorrentes
            .iter()
            .find(|r| r.nome.starts_with("Plano"))
            .expect("plano");
        assert_eq!(tipo("301").recorrente.as_deref(), Some(plano.id.as_str()));
        let emp = plan
            .recorrentes
            .iter()
            .find(|r| r.nome.starts_with("Empr"))
            .expect("emp");
        assert_eq!(tipo("302").recorrente.as_deref(), Some(emp.id.as_str()));
        assert_eq!(tipo("210").recorrente, None);
        // Reimportar não regrava.
        let antes = plan.aplicadas.len();
        let p2 = previa(&a, SHA, &plan, COMP).expect("previa");
        let c2 = confirmar(&p2, &mut plan).expect("confirmar");
        assert_eq!(c2.composicoes, 0);
        assert_eq!(plan.aplicadas.len(), antes);
    }

    // AC-04 e AC-29
    #[test]
    fn confirmar_e_reimportar_nao_duplica() {
        let mut plan = Falso::default();
        let a = aba();
        let p = previa(&a, SHA, &plan, COMP).expect("previa");
        let c = confirmar(&p, &mut plan).expect("confirmar");
        assert_eq!((c.grupos, c.criados), (4, 9));
        assert_eq!(c.pagos_informados, 1);
        assert_eq!(plan.recorrentes.len(), 9);
        assert!(plan.aplicadas.iter().all(|(_, a)| *a == Autor::Importacao));
        // Os grupos foram criados antes dos recorrentes, na ordem da planilha.
        assert_eq!(
            plan.grupos
                .iter()
                .map(|g| g.nome.as_str())
                .collect::<Vec<_>>(),
            ["Salário", "Essencial", "Lar", "Parcial"]
        );
        assert!(matches!(plan.aplicadas[0].0, Alteracao::CriarGrupo { .. }));
        assert_eq!(plan.excecoes.len(), 1);
        assert!(matches!(
            plan.excecoes[0].1,
            Excecao::PagoInformado {
                valor: Centavos(150_000),
                ..
            }
        ));

        let antes = plan.aplicadas.len();
        let p2 = previa(&a, SHA, &plan, COMP).expect("previa 2");
        assert!(p2.novos.is_empty() && p2.grupos_novos.is_empty());
        assert_eq!(p2.existentes.len(), 9);
        assert!(p2.existentes.iter().all(|e| e.por == CasadoPor::Origem));
        let c2 = confirmar(&p2, &mut plan).expect("confirmar 2");
        assert_eq!(c2, Confirmacao::default());
        assert_eq!(plan.aplicadas.len(), antes);
    }

    #[test]
    fn arquivo_novo_gera_diff_e_respeita_decisao_do_usuario() {
        let mut plan = Falso::default();
        let a = aba();
        confirmar(&previa(&a, SHA, &plan, COMP).expect("p"), &mut plan).expect("c");
        // Outro arquivo (outro sha): a Luz mudou e o Aluguel foi decidido pelo usuário.
        let mut b = a.clone();
        b.linhas
            .iter_mut()
            .find(|l| l.descricao == "Luz Exemplo")
            .expect("luz")
            .previsto = Some(Centavos(20_000));
        b.linhas
            .iter_mut()
            .find(|l| l.descricao.starts_with("Aluguel"))
            .expect("al")
            .previsto = Some(Centavos(160_000));
        let aluguel = plan
            .recorrentes
            .iter_mut()
            .find(|r| r.nome.starts_with("Aluguel"))
            .expect("al");
        aluguel.versoes[0].fonte = Fonte::Usuario;

        let p = previa(&b, "bbbb", &plan, COMP).expect("previa");
        assert!(p.novos.is_empty());
        assert_eq!(p.atualizaveis.len(), 1);
        assert_eq!(p.atualizaveis[0].valor_novo, Centavos(20_000));
        assert_eq!(p.atualizaveis[0].valor_atual, Some(Centavos(18_050)));
        assert_eq!(p.conflitos.len(), 1);
        assert_eq!(p.existentes.len(), 7);
        confirmar(&p, &mut plan).expect("c2");
        let luz = plan
            .recorrentes
            .iter()
            .find(|r| r.nome == "Luz Exemplo")
            .expect("luz");
        assert_eq!(luz.versoes.len(), 2);
        assert_eq!(luz.versoes[1].valor, Some(Centavos(20_000)));
        let al = plan
            .recorrentes
            .iter()
            .find(|r| r.nome.starts_with("Aluguel"))
            .expect("al");
        assert_eq!(al.versoes.len(), 1);
    }

    #[test]
    fn mapa_diz_natureza_centro_prazo_e_fornecedor() {
        let mapa = Mapa::de_json(
            r#"{"itens":[
              {"descricao":"Luz Exemplo","centro":"Casa","fornecedor":"ENERGIA SA","parcelas_que_faltam":12},
              {"descricao":"Taxa Soma","natureza":"passagem_saida"},
              {"descricao":"Salário","dia":5}]}"#,
        )
        .expect("mapa");
        let p = previa_com_mapa(&aba(), SHA, &Falso::default(), COMP, &mapa).expect("previa");
        let luz = por_nome(&p, "Luz Exemplo");
        assert_eq!(
            luz.recorrente.prazo,
            Prazo::Parcelas {
                total: 12,
                primeira: COMP
            }
        );
        assert_eq!(
            luz.recorrente.primeira_versao.centro.as_deref(),
            Some("Casa")
        );
        assert_eq!(
            luz.recorrente.primeira_versao.fornecedor.as_deref(),
            Some("ENERGIA SA")
        );
        assert!(matches!(
            por_nome(&p, "Taxa Soma").recorrente.natureza,
            Natureza::Passagem { .. }
        ));
        assert_eq!(
            por_nome(&p, "Salário")
                .recorrente
                .primeira_versao
                .frequencia,
            Frequencia::Mensal { dia: 5 }
        );
        assert!(
            !p.faltantes
                .iter()
                .any(|f| f.o_que.contains("dia do salário"))
        );
    }

    fn aba_de_casos() -> AbaLida {
        let pasta = tempfile::tempdir().expect("pasta");
        let arq = pasta.path().join("c.xlsx");
        crate::fixtura::planilha_de_casos(&arq);
        ler_aba(&arq, ABA).expect("aba")
    }

    #[test]
    fn grupos_da_previa_ignoram_totais_em_b() {
        let p = previa(&aba_de_casos(), SHA, &Falso::default(), COMP).expect("previa");
        let nomes: Vec<&str> = p.grupos_novos.iter().map(|g| g.nome.as_str()).collect();
        assert_eq!(nomes, ["Essencial", "Lar"]);
    }

    #[test]
    fn previsto_zero_com_unitario_e_obs_vira_unitario_e_adiada() {
        let mut plan = Falso::default();
        let p = previa(&aba_de_casos(), SHA, &plan, COMP).expect("previa");
        let a = por_nome(&p, "Zero Com Obs");
        assert_eq!(a.recorrente.primeira_versao.valor, Some(Centavos(8_000)));
        assert_eq!(a.adiar, Some(COMP));
        // Sem unitário, e com unitário mas sem obs: sem valor, faltante, nunca zero.
        for nome in ["Zero Sem Unitario", "Zero Sem Obs"] {
            let n = por_nome(&p, nome);
            assert_eq!(n.recorrente.primeira_versao.valor, None, "{nome}");
            assert_eq!(n.adiar, None);
            assert!(
                p.faltantes
                    .iter()
                    .any(|f| f.nome == nome && f.o_que == "previsto zero")
            );
        }
        assert!(
            p.novos
                .iter()
                .all(|n| n.recorrente.primeira_versao.valor != Some(Centavos(0)))
        );
        let c = confirmar(&p, &mut plan).expect("confirmar");
        assert_eq!(c.adiadas, 1);
        let adiadas: Vec<_> = plan
            .excecoes
            .iter()
            .filter(|(_, e)| matches!(e, Excecao::Adiar { nova_data: None }))
            .collect();
        assert_eq!(adiadas.len(), 1);
        assert_eq!(adiadas[0].0.competencia, COMP);
    }

    #[test]
    fn extras_do_mapa_viram_recorrentes_idempotentes_no_fim() {
        let mapa = Mapa::de_json(
            r#"{"extras":[
              {"nome":"Bolão","grupo":"Passagem","natureza":"passagem_entrada","valor_centavos":5000,"dia":10,"certeza":"estimado"},
              {"nome":"Renda Extra","grupo":"Entradas","natureza":"entrada","dia":3,"centro":"Casa","fornecedor":"CLIENTE","certeza":"confirmado"}]}"#,
        )
        .expect("mapa");
        let mut plan = Falso::default();
        let a = aba();
        let p = previa_com_mapa(&a, SHA, &plan, COMP, &mapa).expect("previa");
        let nomes: Vec<&str> = p.grupos_novos.iter().map(|g| g.nome.as_str()).collect();
        assert_eq!(
            nomes,
            [
                "Salário",
                "Essencial",
                "Lar",
                "Parcial",
                "Passagem",
                "Entradas"
            ]
        );
        let bolao = por_nome(&p, "Bolão");
        assert!(matches!(
            bolao.recorrente.natureza,
            Natureza::Passagem { .. }
        ));
        assert_eq!(bolao.recorrente.prazo, Prazo::SemFim);
        let v = &bolao.recorrente.primeira_versao;
        assert_eq!(
            (v.fonte, v.certeza, v.valor),
            (Fonte::Usuario, Certeza::Estimado, Some(Centavos(5000)))
        );
        assert_eq!(v.frequencia, Frequencia::Mensal { dia: 10 });
        assert_eq!(bolao.origem.aba, "mapa");
        assert_eq!(bolao.origem.linha, 0);
        assert_eq!(bolao.origem.arquivo_sha256, mapa.extras[0].sha256());
        let renda = por_nome(&p, "Renda Extra");
        assert_eq!(renda.recorrente.primeira_versao.valor, None);
        assert!(p.faltantes.iter().any(|f| f.nome == "Renda Extra"));
        confirmar(&p, &mut plan).expect("confirmar");
        assert_eq!(plan.recorrentes.len(), 11);

        // Outro arquivo (outro sha), mesmo mapa: extras não duplicam.
        let p2 = previa_com_mapa(&a, "outro", &plan, COMP, &mapa).expect("previa 2");
        assert!(p2.novos.is_empty(), "{}", p2.contagens());
        assert!(
            p2.existentes
                .iter()
                .any(|e| e.nome == "Bolão" && e.por == CasadoPor::Origem)
        );
    }

    #[test]
    fn dia_do_salario_vem_do_mapa_e_sem_mapa_e_dia_1_com_faltante() {
        let sem = previa(&aba(), SHA, &Falso::default(), COMP).expect("previa");
        assert_eq!(
            por_nome(&sem, "Salário")
                .recorrente
                .primeira_versao
                .frequencia,
            Frequencia::Mensal { dia: 1 }
        );
        assert!(
            sem.faltantes
                .iter()
                .any(|f| f.nome == "Salário" && f.o_que.contains("dia do salário"))
        );
        let mapa = Mapa::de_json(r#"{"itens":[{"descricao":"Salário","dia":7}]}"#).expect("mapa");
        let com = previa_com_mapa(&aba(), SHA, &Falso::default(), COMP, &mapa).expect("previa");
        assert_eq!(
            por_nome(&com, "Salário")
                .recorrente
                .primeira_versao
                .frequencia,
            Frequencia::Mensal { dia: 7 }
        );
        assert!(
            !com.faltantes
                .iter()
                .any(|f| f.o_que.contains("dia do salário"))
        );
    }

    #[test]
    fn em_atraso_no_mapa_adia_uma_vez_inclusive_em_item_existente() {
        let mut plan = Falso::default();
        let a = aba();
        // Primeira importação, sem o mapa: nada adiado.
        confirmar(&previa(&a, SHA, &plan, COMP).expect("p"), &mut plan).expect("c");
        assert!(
            plan.excecoes
                .iter()
                .all(|(_, e)| !matches!(e, Excecao::Adiar { .. }))
        );
        // Reimportação com o mapa novo: o item que já existe ganha a exceção que falta.
        let mapa = Mapa::de_json(r#"{"itens":[{"descricao":"Luz Exemplo","em_atraso":true}]}"#)
            .expect("mapa");
        let p = previa_com_mapa(&a, SHA, &plan, COMP, &mapa).expect("previa");
        assert!(p.novos.is_empty());
        let luz = p
            .existentes
            .iter()
            .find(|e| e.nome == "Luz Exemplo")
            .expect("luz");
        assert_eq!((luz.por, luz.adiar), (CasadoPor::Origem, Some(COMP)));
        assert_eq!(p.existentes.iter().filter(|e| e.adiar.is_some()).count(), 1);
        let c = confirmar(&p, &mut plan).expect("c2");
        assert_eq!(c.adiadas, 1);
        // De novo: já está adiada, não grava outra.
        let p2 = previa_com_mapa(&a, SHA, &plan, COMP, &mapa).expect("previa 2");
        assert!(p2.existentes.iter().all(|e| e.adiar.is_none()));
        let c2 = confirmar(&p2, &mut plan).expect("c3");
        assert_eq!(c2.adiadas, 0);
        let n = plan
            .excecoes
            .iter()
            .filter(|(_, e)| matches!(e, Excecao::Adiar { .. }))
            .count();
        assert_eq!(n, 1);
    }

    #[test]
    fn em_atraso_no_mapa_vale_para_item_novo() {
        let mapa = Mapa::de_json(r#"{"itens":[{"descricao":"Luz Exemplo","em_atraso":true}]}"#)
            .expect("mapa");
        let mut plan = Falso::default();
        let p = previa_com_mapa(&aba(), SHA, &plan, COMP, &mapa).expect("previa");
        assert_eq!(por_nome(&p, "Luz Exemplo").adiar, Some(COMP));
        let c = confirmar(&p, &mut plan).expect("c");
        assert_eq!(c.adiadas, 1);
    }
}
