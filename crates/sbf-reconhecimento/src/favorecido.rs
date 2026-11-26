//! O favorecido de um Pix e a conciliação de um compromisso mensal (spec 13, D4, D13).
//!
//! Na CAIXA a descrição de um Pix é só "PIX ENVIADO": o favorecido (nome e CPF/CNPJ) vem em
//! colunas à parte do registro da Pluggy. Uma **regra de reconhecimento** com
//! `aplica_a: ["registro"]` e a condição `contraparte_doc` (ou `contraparte`) reconhece o
//! favorecido e propõe `conciliar_ocorrencia { compromisso: "Diarista" }`. Este módulo
//! resolve essa proposta contra o Controle mensal:
//!
//! 1. para a ocorrência do compromisso nomeado, junta os **débitos** do extrato que a regra
//!    reconhece: a competência inteira da ocorrência, mais a folga `janela_dias` da proposta
//!    em volta do vencimento (no máximo [`FOLGA_MAXIMA`] dias);
//! 2. se UM débito tem o valor previsto, a sugestão é de um; se a SOMA de vários dá o
//!    previsto (todos, ou o menor subconjunto de até [`MAXIMO_DE_DEBITOS`]), é uma sugestão
//!    combinada; senão são todos os reconhecidos, com o aviso "⚠ soma R$ X difere do
//!    previsto R$ Y";
//! 3. débito já conciliado (ou sugerido) a outra ocorrência, e débito de proposta que o
//!    usuário já decidiu, não entram.
//!
//! O resultado é só **sugestão**: [`buscar`] grava uma conciliação não confirmada por débito
//! (o mesmo mecanismo das sugestões do extrato, que o Controle mensal e o MCP já leem em
//! `Ocorrencia::sugestao_de_conciliacao`) e deixa as propostas abertas no catálogo.
//! [`confirmar`] é o clique do usuário (uma conciliação confirmada por débito, todas na mesma
//! ocorrência; o "Conciliado" soma) e registra `Aceito` pela chave de cada proposta;
//! [`ignorar`] desfaz as sugestões e registra `Rejeitado`. Só uma regra **consolidada**, com
//! soma exata e conta certa (as três provas), concilia sozinha, com Desfazer.
//!
//! Nenhum valor, nome ou documento vai para log ou erro; o CPF/CNPJ não é gravado na
//! execução nem na proposta (o catálogo o tira).

use std::collections::BTreeSet;
use std::sync::Arc;

use sbf_planejamento::conciliacao::normalizar;
use sbf_planejamento::{
    Alteracao, Autor, Centavos, Competencia, Conciliacao, Data, EstadoDaOcorrencia, Filtro,
    FonteDeMovimentos, IdOcorrencia, MovimentoDoExtrato, Natureza, Ocorrencia, Periodo,
    Planejamento,
};
use sbf_regras::catalogo::{Catalogo, EstadoDaProposta};
use sbf_regras::motor::Motor;
use sbf_regras::script::ExecutorDeScript;
use sbf_regras::{Estado as EstadoDaRegra, Proposta, Reconhecimento, Retorno, Valor};

use crate::PROVAS_POSSIVEIS;
use crate::regras::{entrada_do_registro, ler_registro};

/// A maior folga, em dias, que uma proposta pode pedir em volta do vencimento.
pub const FOLGA_MAXIMA: i64 = 60;

/// Quantos débitos, no máximo, formam o subconjunto que fecha o previsto.
pub const MAXIMO_DE_DEBITOS: usize = 6;

/// Quantos débitos entram na busca do subconjunto (os mais perto do vencimento): limita o
/// custo de combinar.
const CANDIDATOS_DA_COMBINACAO: usize = 20;

/// Um débito do extrato que uma regra reconheceu como pagamento do compromisso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebitoDoFavorecido {
    pub movimento: MovimentoDoExtrato,
    pub regra: String,
    pub versao: u32,
    pub consolidada: bool,
    /// A chave de idempotência da proposta (o `Retorno` do usuário vai por ela).
    pub chave: String,
    /// O reconhecimento que gerou a proposta (o catálogo o guarda, sem o documento).
    pub reconhecimento: Reconhecimento,
}

/// A sugestão de conciliar uma ocorrência com um ou mais débitos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SugestaoDoFavorecido {
    pub ocorrencia: IdOcorrencia,
    /// O nome do compromisso ("Diarista").
    pub nome: String,
    /// O favorecido, como a regra o extraiu ou o banco o informou.
    pub favorecido: Option<String>,
    /// Do mais antigo ao mais novo.
    pub debitos: Vec<DebitoDoFavorecido>,
    pub previsto: Option<Centavos>,
    /// A soma dos débitos, em módulo.
    pub soma: Centavos,
    /// A soma é exatamente o previsto.
    pub exata: bool,
    /// "⚠ soma R$ X difere do previsto R$ Y" (ou a falta de previsto).
    pub aviso: Option<String>,
    /// "Pix para <favorecido> · 2 débitos somam o previsto".
    pub rotulo: String,
    /// A regra reconhece o favorecido, o valor é exato e a conta confere: de 1 a 3.
    pub provas: u8,
    /// A regra é consolidada, a soma é exata e as três provas batem: a busca concilia sozinha.
    pub automatica: bool,
}

// ---------------------------------------------------------------------------------------
// O rótulo
// ---------------------------------------------------------------------------------------

/// `R$ 1.234,56`.
fn reais(centavos: i64) -> String {
    Valor::Centavos(centavos).to_string()
}

/// O aviso quando a soma não fecha o previsto.
fn aviso_da_soma(soma: i64, previsto: Option<i64>) -> Option<String> {
    match previsto {
        Some(p) if p != soma => Some(format!(
            "⚠ soma {} difere do previsto {}",
            reais(soma),
            reais(p)
        )),
        Some(_) => None,
        None => Some("⚠ sem valor previsto para comparar".to_owned()),
    }
}

/// O rótulo de uma sugestão: "Pix para Fulana de Tal · 2 débitos somam o previsto". Puro,
/// para a janela montar o mesmo texto a partir das conciliações gravadas.
pub fn rotulo_da_sugestao(
    favorecido: Option<&str>,
    debitos: usize,
    soma: i64,
    previsto: Option<i64>,
) -> String {
    let quem = favorecido
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map_or_else(|| "Pix".to_owned(), |f| format!("Pix para {f}"));
    let resto = match previsto {
        Some(p) if p == soma && debitos == 1 => "o débito é o previsto".to_owned(),
        Some(p) if p == soma => format!("{debitos} débitos somam o previsto"),
        Some(p) => aviso_da_soma(soma, Some(p)).unwrap_or_default(),
        None if debitos == 1 => "sem valor previsto para comparar".to_owned(),
        None => format!("{debitos} débitos, sem valor previsto para comparar"),
    };
    format!("{quem} · {resto}")
}

// ---------------------------------------------------------------------------------------
// A escolha dos débitos
// ---------------------------------------------------------------------------------------

struct Pedido {
    /// Folga em dias em volta do vencimento, já limitada.
    folga: i64,
    /// O nome do compromisso, normalizado.
    compromisso: String,
}

struct Lido {
    movimento: MovimentoDoExtrato,
    rec: Reconhecimento,
    pedidos: Vec<Pedido>,
}

/// Os débitos que a regra reconhece e que pedem conciliar uma ocorrência de compromisso
/// com nome.
fn ler_debitos(motor: &Motor, movimentos: Vec<MovimentoDoExtrato>) -> Vec<Lido> {
    movimentos
        .into_iter()
        .filter(|m| m.valor.0 < 0)
        .filter_map(|movimento| {
            let rec = ler_registro(motor, &entrada_do_registro(&movimento))?;
            let pedidos: Vec<Pedido> = rec
                .propostas
                .iter()
                .filter_map(|p| match p {
                    Proposta::ConciliarOcorrencia {
                        janela_dias,
                        compromisso: Some(c),
                        ..
                    } => {
                        let compromisso = normalizar(c).trim().to_owned();
                        (!compromisso.is_empty()).then(|| Pedido {
                            folga: i64::from(*janela_dias).min(FOLGA_MAXIMA),
                            compromisso,
                        })
                    }
                    _ => None,
                })
                .collect();
            (!pedidos.is_empty()).then_some(Lido {
                movimento,
                rec,
                pedidos,
            })
        })
        .collect()
}

fn centavos(m: &MovimentoDoExtrato) -> i64 {
    m.valor.0.abs()
}

/// Os índices (em `pool`, do mais antigo ao mais novo) que formam a sugestão e se a soma é
/// exata. `pool` não é vazio.
fn escolher(pool: &[&Lido], previsto: Option<i64>, vencimento: Data) -> (Vec<usize>, bool) {
    let todos = || (0..pool.len()).collect::<Vec<_>>();
    let Some(previsto) = previsto.filter(|p| *p > 0) else {
        return (todos(), false);
    };
    let distancia = |i: usize| pool[i].movimento.data.dias_ate(vencimento).abs();
    // 1. Um débito com o valor previsto (o mais perto do vencimento, depois o mais antigo).
    if let Some(i) = (0..pool.len())
        .filter(|i| centavos(&pool[*i].movimento) == previsto)
        .min_by_key(|i| (distancia(*i), *i))
    {
        return (vec![i], true);
    }
    // 2. Todos somam o previsto.
    let soma: i64 = pool.iter().map(|l| centavos(&l.movimento)).sum();
    if pool.len() > 1 && soma == previsto {
        return (todos(), true);
    }
    // 3. O menor subconjunto que dá exato, entre os mais perto do vencimento.
    let mut perto: Vec<usize> = todos();
    perto.sort_by_key(|i| (distancia(*i), *i));
    perto.truncate(CANDIDATOS_DA_COMBINACAO);
    for tamanho in 2..=MAXIMO_DE_DEBITOS.min(perto.len()) {
        let mut melhor: Option<(i64, Vec<usize>)> = None;
        combinar(&perto, tamanho, &mut Vec::new(), 0, &mut |comb| {
            let s: i64 = comb.iter().map(|i| centavos(&pool[*i].movimento)).sum();
            if s != previsto {
                return;
            }
            let custo: i64 = comb.iter().map(|i| distancia(*i)).sum();
            let mut ordenada = comb.to_vec();
            ordenada.sort_unstable();
            if melhor
                .as_ref()
                .is_none_or(|(c, o)| (custo, &ordenada) < (*c, o))
            {
                melhor = Some((custo, ordenada));
            }
        });
        if let Some((_, indices)) = melhor {
            return (indices, true);
        }
    }
    // 4. Todos, com o aviso.
    (todos(), false)
}

/// Todas as combinações de `tamanho` elementos de `itens`.
fn combinar(
    itens: &[usize],
    tamanho: usize,
    atual: &mut Vec<usize>,
    de: usize,
    ao_achar: &mut dyn FnMut(&[usize]),
) {
    if atual.len() == tamanho {
        ao_achar(atual);
        return;
    }
    for i in de..itens.len() {
        if itens.len() - i < tamanho - atual.len() {
            break;
        }
        atual.push(itens[i]);
        combinar(itens, tamanho, atual, i + 1, ao_achar);
        atual.pop();
    }
}

// ---------------------------------------------------------------------------------------
// Sugerir
// ---------------------------------------------------------------------------------------

fn erro_do_planejamento(e: &sbf_planejamento::ErroDoPlanejamento) -> String {
    e.to_string()
}

fn erro_do_catalogo(e: &sbf_regras::ErroDasRegras) -> String {
    e.to_string()
}

/// Uma ocorrência que pode receber a sugestão: saída devida, sem conciliação nem sugestão.
fn elegivel(o: &Ocorrencia) -> bool {
    matches!(o.natureza, Natureza::Saida)
        && o.conciliado.is_none()
        && !o.sugestao_de_conciliacao
        && matches!(
            o.estado,
            EstadoDaOcorrencia::Prevista
                | EstadoDaOcorrencia::PagoInformado
                | EstadoDaOcorrencia::Parcial
                | EstadoDaOcorrencia::EmAtraso
                | EstadoDaOcorrencia::Adiada
        )
}

fn mesma_conta(a: &str, b: &str) -> bool {
    normalizar(a).trim() == normalizar(b).trim()
}

/// Os movimentos que conciliações (confirmadas ou sugeridas) de qualquer ocorrência perto do
/// período já seguram.
fn movimentos_seguros(
    plan: &dyn Planejamento,
    competencias: &BTreeSet<Competencia>,
    hoje: Data,
) -> Result<BTreeSet<String>, String> {
    let mut seguros = BTreeSet::new();
    let mut vistas: BTreeSet<IdOcorrencia> = BTreeSet::new();
    for c in competencias {
        let controle = plan
            .controle(&Periodo::MesCivil(*c), Filtro::Todos, None, hoje)
            .map_err(|e| erro_do_planejamento(&e))?;
        for o in controle
            .linhas
            .iter()
            .chain(controle.atrasadas_de_antes.iter())
        {
            if !(o.conciliado.is_some() || o.sugestao_de_conciliacao)
                || !vistas.insert(o.id.clone())
            {
                continue;
            }
            for g in plan
                .conciliacoes_da_ocorrencia(&o.id)
                .map_err(|e| erro_do_planejamento(&e))?
            {
                seguros.insert(g.conciliacao.movimento);
            }
        }
    }
    Ok(seguros)
}

fn texto_do_campo(rec: &Reconhecimento, nome: &str) -> Option<String> {
    match rec.campos.get(nome).map(|c| &c.valor) {
        Some(Valor::Texto(t)) if !t.trim().is_empty() => Some(t.trim().to_owned()),
        _ => None,
    }
}

/// As sugestões do período, sem gravar nada. Só enxerga as regras que `motor` carrega
/// (ativas e consolidadas: um rascunho nunca sugere). `catalogo` serve para não repetir o que
/// o usuário já decidiu; sem ele, nada é filtrado por decisão.
///
/// # Errors
/// O texto do erro do planejamento ou do catálogo (sem conteúdo do extrato).
pub fn sugerir(
    plan: &dyn Planejamento,
    motor: &Motor,
    fonte: &dyn FonteDeMovimentos,
    catalogo: Option<&Catalogo>,
    periodo: &Periodo,
    hoje: Data,
) -> Result<Vec<SugestaoDoFavorecido>, String> {
    let controle = plan
        .controle(periodo, Filtro::Todos, None, hoje)
        .map_err(|e| erro_do_planejamento(&e))?;
    let mut candidatas: Vec<Ocorrencia> = Vec::new();
    for o in controle
        .linhas
        .iter()
        .chain(controle.atrasadas_de_antes.iter())
        .filter(|o| elegivel(o))
    {
        if !candidatas.iter().any(|x| x.id == o.id) {
            candidatas.push(o.clone());
        }
    }
    if candidatas.is_empty() {
        return Ok(Vec::new());
    }
    candidatas.sort_by(|a, b| (a.vencimento, &a.id).cmp(&(b.vencimento, &b.id)));

    // O extrato que importa: da primeira competência à última, com a folga máxima.
    let primeira = candidatas
        .iter()
        .map(|o| o.id.competencia.dia(1).min(o.vencimento))
        .min()
        .unwrap_or(hoje);
    let ultima = candidatas
        .iter()
        .map(|o| o.id.competencia.dia(31).max(o.vencimento))
        .max()
        .unwrap_or(hoje);
    let lidos = ler_debitos(
        motor,
        fonte.movimentos(
            primeira.somar_dias(-FOLGA_MAXIMA),
            ultima.somar_dias(FOLGA_MAXIMA),
        ),
    );
    if lidos.is_empty() {
        return Ok(Vec::new());
    }

    let decididas: BTreeSet<String> = match catalogo {
        Some(c) => c
            .propostas(None)
            .map_err(|e| erro_do_catalogo(&e))?
            .into_iter()
            .filter(|p| p.estado != EstadoDaProposta::Aberta)
            .map(|p| p.chave)
            .collect(),
        None => BTreeSet::new(),
    };
    let competencias: BTreeSet<Competencia> = candidatas
        .iter()
        .flat_map(|o| {
            [
                o.id.competencia.somar_meses(-1),
                o.id.competencia,
                o.id.competencia.somar_meses(1),
            ]
        })
        .collect();
    let mut usados = movimentos_seguros(plan, &competencias, hoje)?;
    let pares = plan.contas().map_err(|e| erro_do_planejamento(&e))?;

    let mut sugestoes = Vec::new();
    for o in &candidatas {
        let nome = normalizar(&o.nome);
        let (inicio, fim) = (o.id.competencia.dia(1), o.id.competencia.dia(31));
        let pool: Vec<&Lido> = lidos
            .iter()
            .filter(|l| {
                !usados.contains(&l.movimento.movimento) && !decididas.contains(&l.rec.chave)
            })
            .filter(|l| {
                let d = l.movimento.data;
                l.pedidos.iter().any(|p| {
                    nome.contains(p.compromisso.as_str())
                        && ((d >= inicio && d <= fim) || d.dias_ate(o.vencimento).abs() <= p.folga)
                })
            })
            .collect();
        if pool.is_empty() {
            continue;
        }
        let previsto = o.previsto.map(|p| p.0.abs());
        let (indices, exata) = escolher(&pool, previsto, o.vencimento);
        let mut escolhidos: Vec<&Lido> = indices.iter().map(|i| pool[*i]).collect();
        escolhidos.sort_by(|a, b| {
            (a.movimento.data, &a.movimento.movimento)
                .cmp(&(b.movimento.data, &b.movimento.movimento))
        });
        let soma: i64 = escolhidos.iter().map(|l| centavos(&l.movimento)).sum();
        let conta_exigida = o
            .conta
            .as_deref()
            .map(str::trim)
            .and_then(|a| pares.iter().find(|p| p.codigo.trim() == a))
            .and_then(|p| p.conta_do_extrato.clone());
        let conta_ok = conta_exigida.as_deref().is_some_and(|c| {
            escolhidos
                .iter()
                .all(|l| mesma_conta(c, &l.movimento.conta_do_extrato))
        });
        let provas = 1 + u8::from(exata) + u8::from(conta_ok);
        let mut desfeita = false;
        for l in &escolhidos {
            desfeita |= plan
                .conciliacao_desfeita(&o.id, &l.movimento.movimento)
                .map_err(|e| erro_do_planejamento(&e))?;
        }
        let automatica = provas >= PROVAS_POSSIVEIS
            && !desfeita
            && escolhidos
                .iter()
                .all(|l| l.rec.estado == EstadoDaRegra::Consolidada);
        let favorecido = escolhidos.iter().find_map(|l| {
            texto_do_campo(&l.rec, "favorecido").or_else(|| {
                l.movimento
                    .contraparte
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned)
            })
        });
        for l in &escolhidos {
            usados.insert(l.movimento.movimento.clone());
        }
        sugestoes.push(SugestaoDoFavorecido {
            ocorrencia: o.id.clone(),
            nome: o.nome.clone(),
            rotulo: rotulo_da_sugestao(favorecido.as_deref(), escolhidos.len(), soma, previsto),
            favorecido,
            previsto: o.previsto.map(|p| Centavos(p.0.abs())),
            soma: Centavos(soma),
            exata,
            aviso: aviso_da_soma(soma, previsto),
            provas,
            automatica,
            debitos: escolhidos
                .into_iter()
                .map(|l| DebitoDoFavorecido {
                    movimento: l.movimento.clone(),
                    regra: l.rec.regra.clone(),
                    versao: l.rec.versao,
                    consolidada: l.rec.estado == EstadoDaRegra::Consolidada,
                    chave: l.rec.chave.clone(),
                    reconhecimento: l.rec.clone(),
                })
                .collect(),
        });
    }
    Ok(sugestoes)
}

// ---------------------------------------------------------------------------------------
// Gravar, confirmar, ignorar, desfazer
// ---------------------------------------------------------------------------------------

fn conciliacao_do_debito(
    ocorrencia: &IdOcorrencia,
    m: &MovimentoDoExtrato,
    confirmada: bool,
) -> Conciliacao {
    Conciliacao {
        ocorrencia: ocorrencia.clone(),
        movimento: m.movimento.clone(),
        valor: Centavos(centavos(m)),
        data: m.data,
        confirmada,
    }
}

/// Grava as sugestões: uma conciliação **não confirmada** por débito (autor `Importacao`) e
/// a proposta aberta de cada débito no catálogo. As `automaticas` (regra consolidada, soma
/// exata, conta certa, par nunca desfeito) são gravadas confirmadas, com autor
/// `Regra { "<id>@<versão>" }`, e o usuário pode desfazê-las. Devolve quantas ocorrências
/// receberam sugestão.
///
/// # Errors
/// O texto do erro do planejamento ou do catálogo.
pub fn gravar(
    plan: &mut dyn Planejamento,
    catalogo: &mut Catalogo,
    sugestoes: &[SugestaoDoFavorecido],
) -> Result<usize, String> {
    for s in sugestoes {
        for d in &s.debitos {
            catalogo
                .registrar_proposta(&d.reconhecimento, &d.movimento.movimento)
                .map_err(|e| erro_do_catalogo(&e))?;
            let autor = if s.automatica {
                Autor::Regra {
                    regra: format!("{}@{}", d.regra, d.versao),
                }
            } else {
                Autor::Importacao
            };
            plan.aplicar(
                Alteracao::Conciliar {
                    conciliacao: conciliacao_do_debito(&s.ocorrencia, &d.movimento, s.automatica),
                },
                autor,
            )
            .map_err(|e| erro_do_planejamento(&e))?;
        }
    }
    Ok(sugestoes.len())
}

/// Procura e grava as sugestões do período com as regras vigentes do catálogo (a ação
/// "Buscar no extrato"). Devolve quantas ocorrências receberam sugestão.
///
/// # Errors
/// O texto do erro do planejamento ou do catálogo.
pub fn buscar(
    plan: &mut dyn Planejamento,
    catalogo: &mut Catalogo,
    executor: Option<Arc<dyn ExecutorDeScript>>,
    fonte: &dyn FonteDeMovimentos,
    periodo: &Periodo,
    hoje: Data,
) -> Result<usize, String> {
    let motor = Motor::das_vigentes(catalogo, executor).map_err(|e| erro_do_catalogo(&e))?;
    let sugestoes = sugerir(&*plan, &motor, fonte, Some(&*catalogo), periodo, hoje)?;
    gravar(plan, catalogo, &sugestoes)
}

/// As chaves das propostas **abertas** deste movimento que pedem conciliar uma ocorrência.
pub fn chaves_abertas(catalogo: &Catalogo, movimento: &str) -> Vec<String> {
    catalogo
        .propostas(Some(EstadoDaProposta::Aberta))
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            p.entrada == movimento
                && p.conteudo
                    .propostas
                    .iter()
                    .any(|x| matches!(x, Proposta::ConciliarOcorrencia { .. }))
        })
        .map(|p| p.chave)
        .collect()
}

/// As sugestões (não confirmadas) gravadas da ocorrência.
fn pendentes(
    plan: &dyn Planejamento,
    ocorrencia: &IdOcorrencia,
) -> Result<Vec<Conciliacao>, String> {
    Ok(plan
        .conciliacoes_da_ocorrencia(ocorrencia)
        .map_err(|e| erro_do_planejamento(&e))?
        .into_iter()
        .map(|g| g.conciliacao)
        .filter(|c| !c.confirmada)
        .collect())
}

/// Registra o retorno do usuário nas propostas abertas dos movimentos. Uma proposta já
/// decidida não se repete. Falha do catálogo não desfaz a decisão do usuário.
fn registrar(catalogo: &mut Catalogo, movimentos: &[String], retorno: Retorno) {
    for m in movimentos {
        for chave in chaves_abertas(catalogo, m) {
            let _ = catalogo.registrar_retorno(&chave, retorno, &sbf_regras::Quem::Usuario, "");
        }
    }
}

/// O clique em Confirmar: todas as sugestões pendentes da ocorrência viram conciliações
/// confirmadas (autor: o usuário), uma por débito, e cada proposta aberta recebe `Aceito`.
/// Devolve as conciliações confirmadas (vazio quando não há sugestão pendente).
///
/// # Errors
/// O texto do erro do planejamento.
pub fn confirmar(
    plan: &mut dyn Planejamento,
    catalogo: Option<&mut Catalogo>,
    ocorrencia: &IdOcorrencia,
) -> Result<Vec<Conciliacao>, String> {
    let mut feitas = Vec::new();
    for c in pendentes(&*plan, ocorrencia)? {
        let confirmada = Conciliacao {
            confirmada: true,
            ..c
        };
        plan.aplicar(
            Alteracao::Conciliar {
                conciliacao: confirmada.clone(),
            },
            Autor::Usuario,
        )
        .map_err(|e| erro_do_planejamento(&e))?;
        feitas.push(confirmada);
    }
    if let Some(cat) = catalogo {
        let movimentos: Vec<String> = feitas.iter().map(|c| c.movimento.clone()).collect();
        registrar(cat, &movimentos, Retorno::Aceito);
    }
    Ok(feitas)
}

/// O clique em Ignorar: as sugestões pendentes da ocorrência são desfeitas (exclusão lógica,
/// a regra não as refaz) e cada proposta aberta recebe `Rejeitado`. Devolve quantas
/// sugestões foram desfeitas.
///
/// # Errors
/// O texto do erro do planejamento.
pub fn ignorar(
    plan: &mut dyn Planejamento,
    catalogo: Option<&mut Catalogo>,
    ocorrencia: &IdOcorrencia,
) -> Result<usize, String> {
    let pendentes = pendentes(&*plan, ocorrencia)?;
    for c in &pendentes {
        plan.aplicar(
            Alteracao::Desconciliar {
                ocorrencia: ocorrencia.clone(),
                movimento: c.movimento.clone(),
            },
            Autor::Usuario,
        )
        .map_err(|e| erro_do_planejamento(&e))?;
    }
    if let Some(cat) = catalogo {
        let movimentos: Vec<String> = pendentes.iter().map(|c| c.movimento.clone()).collect();
        registrar(cat, &movimentos, Retorno::Rejeitado);
    }
    Ok(pendentes.len())
}

/// O usuário desfez a conciliação automática de `movimento`: a proposta (que nunca passou
/// por um "aceito") é registrada como aceita e em seguida desfeita, o que zera a sequência
/// e rebaixa a regra consolidada a ativa (D4).
pub fn desfeito(catalogo: &mut Catalogo, movimento: &str) {
    for chave in chaves_abertas(catalogo, movimento) {
        let q = sbf_regras::Quem::Usuario;
        if catalogo
            .registrar_retorno(&chave, Retorno::Aceito, &q, "")
            .is_ok()
        {
            let _ = catalogo.registrar_retorno(&chave, Retorno::Desfeito, &q, "");
        }
    }
}
