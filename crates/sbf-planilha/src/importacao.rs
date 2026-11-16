//! Prévia e confirmação da importação de uma aba mensal.
//!
//! A prévia não grava nada. A confirmação aplica tudo por `Planejamento::aplicar` com
//! `Autor::Importacao`. A idempotência é por `(sha256 do arquivo, aba, linha)`: reimportar o
//! mesmo arquivo não cria nada. Um arquivo novo casa por nome com o que já existe e gera
//! diff; nunca sobrescreve uma versão que o usuário decidiu.

use std::collections::HashSet;

use sbf_planejamento::{
    Alteracao, Autor, Centavos, Certeza, Competencia, Data, Escopo, Excecao, Fonte, Frequencia,
    Grupo, IdOcorrencia, MudancaDeVersao, Natureza, NovoRecorrente, OrigemImportada, Planejamento,
    Prazo, Recorrente, Versao,
};

use crate::Resultado;
use crate::leitura::{AbaLida, LinhaDaPlanilha};
use crate::mapa::{ItemDoMapa, Mapa, natureza_de_texto};
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
    };

    for c in candidatos {
        let origem = OrigemImportada {
            arquivo_sha256: sha256.to_owned(),
            aba: aba.nome.clone(),
            linha: c.linha,
        };
        if let Some(id) = plan.origem_importada(&origem)? {
            usados.insert(id.clone());
            p.existentes.push(ItemExistente {
                linha: c.linha,
                nome: c.nome,
                recorrente: id,
                por: CasadoPor::Origem,
                pago: None,
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
            });
            continue;
        };
        usados.insert(r.id.clone());
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
                    });
                }
            }
            _ => p.existentes.push(ItemExistente {
                linha: c.linha,
                nome: c.nome,
                recorrente: r.id.clone(),
                por: CasadoPor::Nome,
                pago: c.pago,
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
    })
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
    let certeza = if l.previsto.is_some() && l.erros.is_empty() {
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
            valor: l.previsto,
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

/// Aplica a prévia: grupos, recorrentes novos, atualizações e pagos informados.
pub fn confirmar(previa: &Previa, plan: &mut dyn Planejamento) -> Resultado<Confirmacao> {
    let mut c = Confirmacao::default();
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
        if let Some(p) = &n.pago {
            plan.aplicar(
                excecao_de_pago(&recibo.alvo, &n.recorrente.prazo, p),
                Autor::Importacao,
            )?;
            c.pagos_informados += 1;
        }
    }
    for e in &previa.existentes {
        if let (CasadoPor::Nome, Some(p)) = (e.por, &e.pago) {
            plan.aplicar(
                excecao_de_pago(&e.recorrente, &Prazo::Desconhecido, p),
                Autor::Importacao,
            )?;
            c.pagos_informados += 1;
        }
    }
    for a in &previa.atualizaveis {
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
        if let Some(p) = &a.pago {
            plan.aplicar(
                excecao_de_pago(&a.recorrente, &Prazo::Desconhecido, p),
                Autor::Importacao,
            )?;
            c.pagos_informados += 1;
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
}
