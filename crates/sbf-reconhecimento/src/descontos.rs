//! Os descontos em folha do contracheque (spec 12, fase A): propor os compromissos que ainda
//! faltam (INSS, IR, FUNCEF, Saúde CAIXA), criá-los e ligá-los às rubricas (D4) e propor a
//! baixa dos descontos do mês pelo próprio contracheque (D3).
//!
//! Consignações (empréstimos) ficam para a fase B: aqui só entram as rubricas fixas de
//! desconto sem contrato. Nada é gravado ao propor; só [`aplicar_descontos`], no clique.

use sbf_planejamento::{
    Alteracao, Autor, Centavos, Certeza, Competencia, Composicao, ErroDoPlanejamento, Fonte,
    Frequencia, Natureza, NovoRecorrente, Planejamento, Prazo, Rubrica, TipoDeRubrica, Versao,
};

use crate::{Sugestao, UsoDeRubricas};

/// O grupo dos compromissos de desconto em folha.
pub const GRUPO: &str = "Descontos em folha";

/// O meio, como na planilha, de um desconto em folha.
const CONTA: &str = "Salário";

/// Os descontos fixos sem contrato que viram compromisso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Familia {
    Inss,
    ImpostoDeRenda,
    Funcef,
    SaudeCaixa,
}

impl Familia {
    /// O nome do compromisso.
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Familia::Inss => "INSS",
            Familia::ImpostoDeRenda => "Imposto de renda",
            Familia::Funcef => "FUNCEF",
            Familia::SaudeCaixa => "Saúde CAIXA",
        }
    }

    /// O nome de um compromisso (normalizado) é desta família?
    fn casa_com(self, nome_normalizado: &str) -> bool {
        let palavras: Vec<&str> = nome_normalizado.split_whitespace().collect();
        let tem = |p: &str| palavras.contains(&p);
        match self {
            Familia::Inss => tem("inss"),
            Familia::ImpostoDeRenda => {
                tem("irrf")
                    || tem("irpf")
                    || tem("ir")
                    || nome_normalizado.contains("imposto de renda")
            }
            Familia::Funcef => tem("funcef"),
            Familia::SaudeCaixa => tem("saude"),
        }
    }
}

fn normalizar(texto: &str) -> String {
    sbf_planilha::texto::normalizar(texto)
}

/// A família de uma rubrica de desconto, pelo nome (ou pelo código do layout conhecido).
/// `None` para o que não é desconto fixo sem contrato: consignação, desconto sobre PLR,
/// contribuição negocial e qualquer outro.
#[must_use]
pub fn familia(rubrica: &Rubrica) -> Option<Familia> {
    if rubrica.tipo != TipoDeRubrica::Desconto {
        return None;
    }
    let nome = normalizar(&rubrica.nome);
    let codigo = rubrica.codigo.as_deref().map(str::trim);
    if nome.contains("consignac") || nome.contains("emprestimo") || nome.contains("plr") {
        return None;
    }
    if nome.split_whitespace().any(|p| p == "inss") || codigo == Some("4313") {
        Some(Familia::Inss)
    } else if nome == "imposto de renda" || codigo == Some("4325") {
        Some(Familia::ImpostoDeRenda)
    } else if nome.contains("funcef") || codigo == Some("4342") {
        Some(Familia::Funcef)
    } else if nome.contains("saude") || codigo == Some("4460") {
        Some(Familia::SaudeCaixa)
    } else {
        None
    }
}

/// Um compromisso de desconto em folha a criar (ou a ligar a um que já existe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescontoNovo {
    /// O nome do compromisso ("INSS").
    pub nome: String,
    /// As rubricas do contracheque (índices em `Contracheque::rubricas`) que o compromisso
    /// segue; o valor dele é a soma delas.
    pub rubricas: Vec<usize>,
    /// A soma das rubricas.
    pub valor: Centavos,
    /// Já existe um compromisso de desconto em folha com este nome: liga a rubrica a ele em
    /// vez de criar outro.
    pub ligar_a: Option<String>,
}

/// A rubrica do contracheque já está ligada a um compromisso na composição vigente?
fn ja_ligada(composicao: Option<&Composicao>, rubrica: &Rubrica) -> bool {
    composicao.is_some_and(|c| {
        c.rubricas.iter().any(|x| {
            x.recorrente.is_some()
                && x.codigo == rubrica.codigo
                && x.nome == rubrica.nome
                && x.tipo == rubrica.tipo
        })
    })
}

/// A sugestão "Criar compromissos de desconto em folha", se há rubrica fixa de INSS, IR,
/// FUNCEF ou Saúde ainda sem compromisso. Só lê o planejamento.
///
/// # Errors
/// Falha do planejamento.
pub fn propor_descontos(
    plan: &dyn Planejamento,
    uso: &UsoDeRubricas,
) -> Result<Option<Sugestao>, ErroDoPlanejamento> {
    let c = &uso.contracheque;
    let vigente = plan.composicao(&uso.recorrente, c.pagamento)?;
    let existentes: Vec<_> = plan
        .recorrentes(false)?
        .into_iter()
        .filter(|r| r.natureza == Natureza::DescontoEmFolha)
        .collect();
    // Os compromissos que alguma rubrica da composição vigente já segue não se oferecem de novo.
    let seguidos: Vec<&str> = vigente
        .iter()
        .flat_map(|k| k.rubricas.iter().filter_map(|r| r.recorrente.as_deref()))
        .collect();
    let mut descontos: Vec<DescontoNovo> = Vec::new();
    for fam in [
        Familia::Inss,
        Familia::ImpostoDeRenda,
        Familia::Funcef,
        Familia::SaudeCaixa,
    ] {
        let rubricas: Vec<usize> = c
            .rubricas
            .iter()
            .enumerate()
            .filter(|(i, r)| {
                familia(r) == Some(fam)
                    && !c.informativas.get(*i).copied().unwrap_or(false)
                    && !c.eventuais.get(*i).copied().unwrap_or(false)
                    && !ja_ligada(vigente.as_ref(), r)
            })
            .map(|(i, _)| i)
            .collect();
        if rubricas.is_empty() {
            continue;
        }
        let casam: Vec<_> = existentes
            .iter()
            .filter(|r| fam.casa_com(&normalizar(&r.nome)) && !seguidos.contains(&r.id.as_str()))
            .collect();
        // Mais de um compromisso com cara da mesma família: a pessoa liga à mão.
        let ligar_a = match casam.as_slice() {
            [] => None,
            [um] => Some(um.id.clone()),
            _ => continue,
        };
        let valor = Centavos(rubricas.iter().map(|&i| c.rubricas[i].valor.0).sum());
        descontos.push(DescontoNovo {
            nome: fam.nome().to_owned(),
            rubricas,
            valor,
            ligar_a,
        });
    }
    if descontos.is_empty() {
        return Ok(None);
    }
    let criar = descontos.iter().filter(|d| d.ligar_a.is_none()).count();
    let ligar = descontos.len() - criar;
    let nomes = descontos
        .iter()
        .map(|d| d.nome.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let rotulo = match (criar, ligar) {
        (_, 0) => format!("Criar compromissos de desconto em folha ({nomes})"),
        (0, _) => format!("Ligar descontos em folha aos compromissos que já existem ({nomes})"),
        _ => format!("Criar {criar} e ligar {ligar} compromissos de desconto em folha ({nomes})"),
    };
    Ok(Some(Sugestao::CriarDescontosEmFolha {
        rotulo,
        uso: Box::new(uso.clone()),
        descontos,
    }))
}

/// Copia as ligações (`Rubrica::recorrente`) da composição de antes para a nova, nas rubricas
/// que são as mesmas (código, nome e tipo): confirmar de novo "usar estas rubricas" não pode
/// desligar o que "criar compromissos" ligou.
pub fn preservar_ligacoes(nova: &mut Composicao, anterior: &Composicao) {
    for r in nova.rubricas.iter_mut().filter(|r| r.recorrente.is_none()) {
        if let Some(x) = anterior.rubricas.iter().find(|x| {
            x.recorrente.is_some() && x.codigo == r.codigo && x.nome == r.nome && x.tipo == r.tipo
        }) {
            r.recorrente.clone_from(&x.recorrente);
        }
    }
}

fn base_do(plan: &dyn Planejamento, id: &str) -> Result<i64, ErroDoPlanejamento> {
    plan.recorrente(id)?
        .map(|r| r.versao_do_registro)
        .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("recorrente {id}")))
}

/// Cria os compromissos (autor `Usuario`: é o clique), liga as rubricas a eles nas composições
/// do contracheque (a do mês e, se houver, a das fixas) e as grava. Devolve quantos
/// compromissos foram criados.
///
/// # Errors
/// Falha do planejamento; o que já foi gravado fica.
pub fn aplicar_descontos(
    plan: &mut dyn Planejamento,
    uso: &UsoDeRubricas,
    descontos: &[DescontoNovo],
    eventuais: &[bool],
) -> Result<usize, ErroDoPlanejamento> {
    let c = &uso.contracheque;
    let dia = plan
        .recorrente(&uso.recorrente)?
        .and_then(|r| sbf_planejamento::versao_vigente(&r, c.pagamento).map(|v| v.frequencia))
        .and_then(|f| match f {
            Frequencia::Mensal { dia } => Some(dia),
            _ => None,
        })
        .unwrap_or(1);
    let posicao_base = plan
        .recorrentes(false)?
        .iter()
        .filter(|r| r.grupo == GRUPO)
        .map(|r| r.posicao)
        .max()
        .unwrap_or(0);
    let mut criados = 0;
    let mut ligacoes: Vec<(String, &DescontoNovo)> = Vec::new();
    for (i, d) in descontos.iter().enumerate() {
        let id = if let Some(id) = &d.ligar_a {
            id.clone()
        } else {
            let recibo = plan.aplicar(
                Alteracao::CriarRecorrente {
                    recorrente: NovoRecorrente {
                        nome: d.nome.clone(),
                        grupo: GRUPO.to_owned(),
                        subgrupo: None,
                        posicao: posicao_base + 1 + i64::try_from(i).unwrap_or(0),
                        natureza: Natureza::DescontoEmFolha,
                        prazo: Prazo::SemFim,
                        primeira_versao: Versao {
                            id: String::new(),
                            desde: c.pagamento,
                            valor: Some(d.valor),
                            certeza: Certeza::Confirmado,
                            fonte: Fonte::Usuario,
                            frequencia: Frequencia::Mensal { dia },
                            conta: Some(CONTA.to_owned()),
                            centro: None,
                            fornecedor: None,
                            categoria: None,
                            nota: Some("Segue a rubrica do contracheque.".to_owned()),
                        },
                    },
                    origem: None,
                },
                Autor::Usuario,
            )?;
            criados += 1;
            recibo.alvo
        };
        ligacoes.push((id, d));
    }
    for mut composicao in uso.composicoes(eventuais) {
        for (id, d) in &ligacoes {
            for r in &mut composicao.rubricas {
                if d.rubricas.iter().any(|&i| mesma_rubrica(&c.rubricas[i], r)) {
                    r.recorrente = Some(id.clone());
                }
            }
        }
        if let Some(anterior) = plan.composicao(&composicao.recorrente, composicao.desde)?
            && anterior.desde == composicao.desde
        {
            preservar_ligacoes(&mut composicao, &anterior);
        }
        let versao_base = base_do(plan, &composicao.recorrente)?;
        plan.aplicar(
            Alteracao::DefinirComposicao {
                composicao,
                versao_base,
            },
            Autor::Usuario,
        )?;
    }
    Ok(criados)
}

fn mesma_rubrica(do_contracheque: &Rubrica, da_composicao: &Rubrica) -> bool {
    do_contracheque.codigo == da_composicao.codigo
        && do_contracheque.nome == da_composicao.nome
        && do_contracheque.tipo == da_composicao.tipo
        && do_contracheque.valor == da_composicao.valor
        && do_contracheque.competencia == da_composicao.competencia
}

/// A sugestão "Dar baixa nos N descontos em folha": as rubricas de desconto do contracheque
/// do mês, do contracheque `sha256`, que já estão ligadas a um compromisso e ainda não foram
/// baixadas. `None` sem nada a baixar. Só lê.
///
/// # Errors
/// Falha do planejamento.
pub fn sugestao_de_baixa(
    plan: &dyn Planejamento,
    competencia: Competencia,
    sha256: &str,
) -> Result<Option<Sugestao>, ErroDoPlanejamento> {
    let prefixo = format!("contracheque:{sha256}:");
    let baixas: Vec<_> = plan
        .baixas_pelo_contracheque(competencia)?
        .into_iter()
        .filter(|b| !b.ja_baixada && b.conciliacao.movimento.starts_with(&prefixo))
        .collect();
    if baixas.is_empty() {
        return Ok(None);
    }
    let n = baixas.len();
    let rotulo = if n == 1 {
        "Dar baixa no desconto em folha ligado".to_owned()
    } else {
        format!("Dar baixa nos {n} descontos em folha")
    };
    Ok(Some(Sugestao::DarBaixa { rotulo, baixas }))
}
