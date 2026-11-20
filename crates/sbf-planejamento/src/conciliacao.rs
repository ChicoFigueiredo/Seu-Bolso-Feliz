//! A conciliação das ocorrências com o extrato: pura, sobre `Vec<MovimentoDoExtrato>`.
//!
//! Ela só calcula; quem grava é o serviço (`Planejamento::buscar_conciliacoes`). As regras:
//!
//! - Pares do melhor ao pior; **um movimento por ocorrência** e uma ocorrência por movimento.
//! - Um movimento disputado por duas ocorrências com a mesma pontuação é **ambíguo**: não se
//!   sugere (o usuário decide).
//! - Tipos que nunca pagam compromisso ficam de fora (`transferencia_interna`,
//!   `pagamento_fatura`, `investimento`, `estorno`).
//! - **Conta**: com par confirmado (o apelido da ocorrência ↔ a conta do extrato), só serve
//!   movimento da conta certa. Sem par, a conta não pesa.
//! - **Assinatura**: o movimento cuja descrição normalizada contém o texto, dentro da janela
//!   da assinatura em volta do vencimento, é candidato com qualquer valor; a diferença de
//!   valor aparece para a tela avisar.
//! - O valor esperado é o líquido da composição vigente no mês, quando há; senão o previsto
//!   (ou o pago informado).
//! - Um movimento já conciliado (confirmado) a outra ocorrência não serve.
//!
//! Nenhum valor, descrição ou conta vai a log.

use std::collections::{HashMap, HashSet};

use crate::{
    Assinatura, Centavos, Composicao, Conciliacao, Data, EstadoDaOcorrencia, IdOcorrencia,
    MovimentoCandidato, MovimentoDoExtrato, Natureza, Ocorrencia, ParDeConta, Sentido,
};

/// Peso de uma conciliação confirmada na escolha do par de conta.
const PESO_CONFIRMADA: u32 = 3;
/// Peso de uma conciliação só sugerida.
const PESO_SUGERIDA: u32 = 1;
/// Peso de um crédito achado pela assinatura.
const PESO_DA_ASSINATURA: u32 = 2;

/// Os parâmetros da conciliação.
#[derive(Debug, Clone)]
pub struct Config {
    /// Dias antes e depois do vencimento para casar por valor.
    pub janela_dias: i64,
    /// Tolerância relativa, em %.
    pub tolerancia_pct: i64,
    /// Tolerância absoluta mínima. Vale a maior das duas.
    pub tolerancia_minima: Centavos,
    /// `tipo` do extrato que nunca é pagamento de compromisso.
    pub tipos_excluidos: Vec<String>,
    /// Dias antes e depois do vencimento na lista de candidatos (a escolha manual).
    pub janela_dos_candidatos: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            janela_dias: 5,
            tolerancia_pct: 1,
            tolerancia_minima: Centavos(100),
            tipos_excluidos: [
                "transferencia_interna",
                "pagamento_fatura",
                "investimento",
                "estorno",
            ]
            .map(str::to_owned)
            .to_vec(),
            janela_dos_candidatos: 10,
        }
    }
}

/// Uma ocorrência a conciliar, com o que o serviço sabe dela.
#[derive(Debug, Clone)]
pub struct Alvo {
    pub ocorrencia: Ocorrencia,
    /// O valor esperado (ver [`valor_esperado`]).
    pub esperado: Option<Centavos>,
    /// A conta do extrato exigida (o par confirmado do apelido da ocorrência).
    pub conta_exigida: Option<String>,
    pub assinatura: Option<Assinatura>,
}

impl Alvo {
    /// Sem par de conta, sem assinatura e com o esperado que a ocorrência já traz.
    #[must_use]
    pub fn simples(ocorrencia: Ocorrencia) -> Alvo {
        let esperado = valor_esperado(&ocorrencia, None);
        Alvo {
            ocorrencia,
            esperado,
            conta_exigida: None,
            assinatura: None,
        }
    }
}

/// Minúsculas e sem acento: a assinatura "salario" acha "SALÁRIO EMPRESA".
#[must_use]
pub fn normalizar(texto: &str) -> String {
    texto
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            outro => outro,
        })
        .collect()
}

/// A ocorrência pode receber uma conciliação (ainda não tem uma, nem sugestão, e está viva).
#[must_use]
pub fn pode_conciliar(o: &Ocorrencia) -> bool {
    !matches!(o.natureza, Natureza::DescontoEmFolha)
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

/// A ocorrência é dinheiro entrando.
#[must_use]
pub fn entra(o: &Ocorrencia) -> bool {
    matches!(
        o.natureza,
        Natureza::Entrada
            | Natureza::Passagem {
                sentido: Sentido::Entrada
            }
    )
}

/// O valor com que o movimento se compara: o líquido da composição vigente no mês, quando há;
/// senão o previsto, ou o pago informado.
#[must_use]
pub fn valor_esperado(o: &Ocorrencia, composicao: Option<&Composicao>) -> Option<Centavos> {
    composicao
        .map(Composicao::liquido)
        .or(o.previsto)
        .or(o.pago_informado)
}

/// Uma ocorrência só entra na busca se há como casá-la: um valor esperado positivo ou uma
/// assinatura.
#[must_use]
pub fn tem_como_casar(alvo: &Alvo) -> bool {
    pode_conciliar(&alvo.ocorrencia)
        && (alvo.esperado.is_some_and(|v| v.0 > 0) || alvo.assinatura.is_some())
}

#[derive(Debug, Clone, Copy)]
struct Avaliacao {
    bate_assinatura: bool,
    mesma_conta: bool,
    diferenca: Option<i64>,
    dias: i64,
}

fn mesma(a: &str, b: &str) -> bool {
    normalizar(a).trim() == normalizar(b).trim()
}

/// Como o movimento se sai contra o alvo, ou `None` se nem entra na conta. `estrito` é o modo
/// da sugestão (exige a conta e o valor ou a assinatura); o outro é o da lista de candidatos.
fn avaliar(
    alvo: &Alvo,
    m: &MovimentoDoExtrato,
    usados: &HashSet<String>,
    cfg: &Config,
    estrito: bool,
) -> Option<Avaliacao> {
    if m.valor.0 == 0
        || usados.contains(&m.movimento)
        || cfg.tipos_excluidos.contains(&m.tipo)
        || (m.valor.0 > 0) != entra(&alvo.ocorrencia)
    {
        return None;
    }
    let dias = alvo.ocorrencia.vencimento.dias_ate(m.data).abs();
    let bate_assinatura = alvo.assinatura.as_ref().is_some_and(|a| {
        let texto = normalizar(&a.texto);
        let texto = texto.trim();
        !texto.is_empty()
            && dias <= i64::from(a.janela_dias)
            && normalizar(&m.descricao).contains(texto)
    });
    let mesma_conta = alvo
        .conta_exigida
        .as_deref()
        .is_some_and(|c| mesma(c, &m.conta_do_extrato));
    let diferenca = alvo.esperado.map(|e| (m.valor.0.abs() - e.0.abs()).abs());
    if estrito {
        if alvo.conta_exigida.is_some() && !mesma_conta {
            return None;
        }
        let por_valor = alvo.esperado.is_some_and(|e| {
            let tolerancia = (e.0 * cfg.tolerancia_pct / 100).max(cfg.tolerancia_minima.0);
            e.0 > 0 && dias <= cfg.janela_dias && diferenca.is_some_and(|d| d <= tolerancia)
        });
        if !bate_assinatura && !por_valor {
            return None;
        }
    } else {
        let janela = alvo
            .assinatura
            .as_ref()
            .map_or(0, |a| i64::from(a.janela_dias))
            .max(cfg.janela_dos_candidatos);
        if dias > janela {
            return None;
        }
    }
    Some(Avaliacao {
        bate_assinatura,
        mesma_conta,
        diferenca,
        dias,
    })
}

/// (sem assinatura, diferença, dias, alvo, movimento): o menor é o melhor.
type Par = (bool, i64, i64, usize, usize);

/// Um corte a mais: a ocorrência e o movimento servem um ao outro?
pub type FiltroExtra<'a> = dyn Fn(&Ocorrencia, &MovimentoDoExtrato) -> bool + 'a;

/// As sugestões do período, sem gravar. `usados` são os movimentos que outras ocorrências já
/// têm (confirmados ou sugeridos). `filtro` é um corte a mais por ocorrência e movimento (o
/// fornecedor da planilha), que não vale para quem bate a assinatura.
#[must_use]
pub fn sugerir(
    alvos: &[Alvo],
    movimentos: &[MovimentoDoExtrato],
    usados: &HashSet<String>,
    cfg: &Config,
    filtro: Option<&FiltroExtra<'_>>,
) -> Vec<Conciliacao> {
    let mut pares: Vec<Par> = Vec::new();
    for (i, alvo) in alvos.iter().enumerate().filter(|(_, a)| tem_como_casar(a)) {
        for (j, m) in movimentos.iter().enumerate() {
            let Some(a) = avaliar(alvo, m, usados, cfg, true) else {
                continue;
            };
            if !a.bate_assinatura && filtro.is_some_and(|f| !f(&alvo.ocorrencia, m)) {
                continue;
            }
            pares.push((
                !a.bate_assinatura,
                a.diferenca.unwrap_or(i64::MAX),
                a.dias,
                i,
                j,
            ));
        }
    }
    pares.sort_by(|a, b| {
        (a.0, a.1, a.2, a.3, &movimentos[a.4].movimento).cmp(&(
            b.0,
            b.1,
            b.2,
            b.3,
            &movimentos[b.4].movimento,
        ))
    });
    // Movimento disputado por duas ocorrências com a mesma pontuação é ambíguo: não se sugere,
    // em vez de sortear e repetir a sugestão a cada rodada.
    let ambiguos: HashSet<usize> = pares
        .iter()
        .filter(|a| {
            pares
                .iter()
                .any(|b| b.4 == a.4 && b.3 != a.3 && (b.0, b.1, b.2) == (a.0, a.1, a.2))
        })
        .map(|a| a.4)
        .collect();
    pares.retain(|p| !ambiguos.contains(&p.4));
    let mut alvos_usados = HashSet::new();
    let mut movimentos_usados = HashSet::new();
    let mut sugestoes = Vec::new();
    for (_, _, _, i, j) in pares {
        if alvos_usados.contains(&i) || movimentos_usados.contains(&j) {
            continue;
        }
        alvos_usados.insert(i);
        movimentos_usados.insert(j);
        let m = &movimentos[j];
        sugestoes.push(Conciliacao {
            ocorrencia: alvos[i].ocorrencia.id.clone(),
            movimento: m.movimento.clone(),
            valor: Centavos(m.valor.0.abs()),
            data: m.data,
            confirmada: false,
        });
    }
    sugestoes.sort_by(|a, b| a.ocorrencia.cmp(&b.ocorrencia));
    sugestoes
}

/// Os candidatos de uma ocorrência, do melhor ao pior: assinatura na conta certa, conta
/// certa, assinatura, o resto; dentro de cada faixa, a menor diferença de valor e depois a
/// menor distância em dias. `usados` são os movimentos já confirmados a outras ocorrências.
#[must_use]
pub fn candidatos(
    alvo: &Alvo,
    movimentos: &[MovimentoDoExtrato],
    usados: &HashSet<String>,
    cfg: &Config,
) -> Vec<MovimentoCandidato> {
    let mut lista: Vec<(Avaliacao, &MovimentoDoExtrato)> = movimentos
        .iter()
        .filter_map(|m| Some((avaliar(alvo, m, usados, cfg, false)?, m)))
        .collect();
    let faixa = |a: &Avaliacao| match (a.bate_assinatura, a.mesma_conta) {
        (true, true) => 0,
        (false, true) => 1,
        (true, false) => 2,
        (false, false) => 3,
    };
    lista.sort_by(|(a, ma), (b, mb)| {
        (
            faixa(a),
            a.diferenca.unwrap_or(i64::MAX),
            a.dias,
            &ma.movimento,
        )
            .cmp(&(
                faixa(b),
                b.diferenca.unwrap_or(i64::MAX),
                b.dias,
                &mb.movimento,
            ))
    });
    lista
        .into_iter()
        .map(|(a, m)| MovimentoCandidato {
            movimento: m.clone(),
            bate_assinatura: a.bate_assinatura,
            mesma_conta: a.mesma_conta,
            diferenca: a.diferenca.map(Centavos),
        })
        .collect()
}

/// De onde vem um indício de que o apelido corresponde a uma conta do extrato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Indicio {
    /// Uma conciliação do histórico (a do movimento está nesta conta).
    Conciliacao { confirmada: bool },
    /// Um crédito achado pela assinatura de um recorrente do apelido.
    Assinatura,
}

/// Um indício: o apelido e a conta do extrato para onde ele aponta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidencia {
    pub apelido: String,
    pub conta_do_extrato: String,
    pub indicio: Indicio,
}

#[derive(Default)]
struct Placar {
    peso: u32,
    confirmadas: u32,
    sugeridas: u32,
    assinaturas: u32,
}

fn plural(n: u32, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

fn motivo_do(p: &Placar) -> String {
    let mut partes = Vec::new();
    if p.confirmadas > 0 {
        partes.push(plural(
            p.confirmadas,
            "conciliação confirmada",
            "conciliações confirmadas",
        ));
    }
    if p.sugeridas > 0 {
        partes.push(plural(
            p.sugeridas,
            "conciliação sugerida",
            "conciliações sugeridas",
        ));
    }
    if p.assinaturas > 0 {
        partes.push(plural(
            p.assinaturas,
            "crédito da assinatura",
            "créditos da assinatura",
        ));
    }
    match partes.split_last() {
        Some((ultima, [])) => ultima.clone(),
        Some((ultima, antes)) => format!("{} e {ultima}", antes.join(", ")),
        None => String::new(),
    }
}

/// Os pares propostos: para cada apelido, a conta do extrato com mais peso (confirmadas pesam
/// mais). Empate no topo não propõe nada. Quem já tem par confirmado fica de fora.
#[must_use]
///
/// Uma conta do extrato é de um apelido só: a que já está confirmada para outro apelido não é
/// proposta; a que dois apelidos disputam fica com o de mais peso, e no empate os dois
/// aparecem com "disputada com …" no motivo (o usuário escolhe).
pub fn pares_sugeridos(
    evidencias: &[Evidencia],
    ja_pareados: &HashSet<String>,
    contas_ja_usadas: &HashSet<String>,
) -> Vec<ParDeConta> {
    let mut por_apelido: HashMap<&str, HashMap<&str, Placar>> = HashMap::new();
    for e in evidencias {
        if ja_pareados.contains(&e.apelido) || contas_ja_usadas.contains(&e.conta_do_extrato) {
            continue;
        }
        let p = por_apelido
            .entry(&e.apelido)
            .or_default()
            .entry(&e.conta_do_extrato)
            .or_default();
        match e.indicio {
            Indicio::Conciliacao { confirmada: true } => {
                p.peso += PESO_CONFIRMADA;
                p.confirmadas += 1;
            }
            Indicio::Conciliacao { confirmada: false } => {
                p.peso += PESO_SUGERIDA;
                p.sugeridas += 1;
            }
            Indicio::Assinatura => {
                p.peso += PESO_DA_ASSINATURA;
                p.assinaturas += 1;
            }
        }
    }
    let vencedores: Vec<(ParDeConta, u32)> = por_apelido
        .into_iter()
        .filter_map(|(apelido, contas)| {
            let maior = contas.values().map(|p| p.peso).max()?;
            let mut do_topo = contas.iter().filter(|(_, p)| p.peso == maior);
            let (conta, placar) = do_topo.next()?;
            if do_topo.next().is_some() {
                return None;
            }
            Some((
                ParDeConta {
                    apelido: apelido.to_owned(),
                    conta_do_extrato: (*conta).to_owned(),
                    evidencias: placar.confirmadas + placar.sugeridas + placar.assinaturas,
                    motivo: motivo_do(placar),
                },
                placar.peso,
            ))
        })
        .collect();
    // Dois apelidos querendo a mesma conta: fica o de mais peso; no empate, os dois, avisados.
    let mut pares: Vec<ParDeConta> = vencedores
        .iter()
        .filter(|(par, peso)| {
            !vencedores.iter().any(|(outro, peso_do_outro)| {
                outro.apelido != par.apelido
                    && outro.conta_do_extrato == par.conta_do_extrato
                    && peso_do_outro > peso
            })
        })
        .map(|(par, peso)| {
            let mut rivais: Vec<&str> = vencedores
                .iter()
                .filter(|(outro, peso_do_outro)| {
                    outro.apelido != par.apelido
                        && outro.conta_do_extrato == par.conta_do_extrato
                        && peso_do_outro == peso
                })
                .map(|(outro, _)| outro.apelido.as_str())
                .collect();
            rivais.sort_unstable();
            let mut par = par.clone();
            if !rivais.is_empty() {
                par.motivo = format!("{} · disputada com {}", par.motivo, rivais.join(", "));
            }
            par
        })
        .collect();
    pares.sort_by(|a, b| a.apelido.cmp(&b.apelido));
    pares
}

/// Os créditos que a assinatura acha para uma ocorrência (para o placar dos pares): o mais
/// próximo do vencimento, sem exigir conta nem valor.
#[must_use]
pub fn credito_da_assinatura<'a>(
    alvo: &Alvo,
    movimentos: &'a [MovimentoDoExtrato],
    cfg: &Config,
) -> Option<&'a MovimentoDoExtrato> {
    alvo.assinatura.as_ref()?;
    let semconta = Alvo {
        conta_exigida: None,
        ..alvo.clone()
    };
    let nenhum = HashSet::new();
    movimentos
        .iter()
        .filter_map(|m| Some((avaliar(&semconta, m, &nenhum, cfg, false)?, m)))
        .filter(|(a, _)| a.bate_assinatura)
        .min_by_key(|(a, m)| (a.dias, &m.movimento))
        .map(|(_, m)| m)
}

/// Os movimentos que as conciliações gravadas seguram. `so_confirmadas` deixa de fora as
/// sugestões; `exceto` é a ocorrência que está escolhendo (o que ela mesma segura não conta).
pub fn movimentos_usados<'a>(
    conciliacoes: impl IntoIterator<Item = &'a Conciliacao>,
    so_confirmadas: bool,
    exceto: Option<&IdOcorrencia>,
) -> HashSet<String> {
    conciliacoes
        .into_iter()
        .filter(|c| (c.confirmada || !so_confirmadas) && exceto != Some(&c.ocorrencia))
        .map(|c| c.movimento.clone())
        .collect()
}

/// A data de hoje a partir de segundos Unix (UTC).
#[must_use]
pub fn data_de_segundos(segundos: i64) -> Data {
    Data::de_dias_desde_1970(segundos.div_euclid(86_400))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{Certeza, Competencia, Fonte, Rubrica, TipoDeRubrica};

    fn data(dia: u8) -> Data {
        Data::nova(2026, 11, dia).expect("data")
    }

    fn ocorrencia(rec: &str, natureza: Natureza, dia: u8, previsto: Option<i64>) -> Ocorrencia {
        Ocorrencia {
            id: IdOcorrencia {
                recorrente: rec.to_owned(),
                competencia: Competencia { ano: 2026, mes: 11 },
                parcela: None,
            },
            nome: rec.to_owned(),
            grupo: "G".to_owned(),
            subgrupo: None,
            posicao: 1,
            natureza,
            vencimento: data(dia),
            vencimento_original: data(dia),
            previsto: previsto.map(Centavos),
            certeza: Certeza::Confirmado,
            fonte: Fonte::Planilha,
            pago_informado: None,
            conciliado: None,
            estado: EstadoDaOcorrencia::Prevista,
            parcela: None,
            conta: Some("CX-0000".to_owned()),
            centro: None,
            fornecedor: None,
            versao: "v".to_owned(),
            calculo: None,
            prazo_desconhecido: false,
            sugestao_de_conciliacao: false,
        }
    }

    fn mov(id: &str, dia: u8, valor: i64, conta: &str, descricao: &str) -> MovimentoDoExtrato {
        MovimentoDoExtrato {
            movimento: id.to_owned(),
            data: data(dia),
            valor: Centavos(valor),
            conta_do_extrato: conta.to_owned(),
            descricao: descricao.to_owned(),
            tipo: if valor > 0 { "credito" } else { "debito" }.to_owned(),
            contraparte: None,
            contraparte_doc: None,
        }
    }

    fn alvo(o: Ocorrencia) -> Alvo {
        Alvo::simples(o)
    }

    fn saida(rec: &str, dia: u8, previsto: i64) -> Alvo {
        alvo(ocorrencia(rec, Natureza::Saida, dia, Some(previsto)))
    }

    fn sugerir_simples(alvos: &[Alvo], movs: &[MovimentoDoExtrato]) -> Vec<Conciliacao> {
        sugerir(alvos, movs, &HashSet::new(), &Config::default(), None)
    }

    #[test]
    fn casa_por_valor_e_janela_um_movimento_por_ocorrencia() {
        let alvos = [
            saida("aluguel", 5, 150_000),
            saida("luz", 12, 18_050),
            saida("net", 12, 9_000),
        ];
        let movs = [
            mov("m_a", 7, -150_000, "B1", "ALUGUEL"),
            mov("m_l", 10, -18_140, "B1", "ENERGIA"),
            // Fora da janela, valor errado e tipo excluído.
            mov("m_longe", 18, -9_000, "B1", "NET"),
            mov("m_errado", 15, -9_500, "B1", "NET"),
            MovimentoDoExtrato {
                tipo: "transferencia_interna".to_owned(),
                contraparte: None,
                contraparte_doc: None,
                ..mov("m_interna", 15, -9_000, "B1", "NET")
            },
        ];
        let s = sugerir_simples(&alvos, &movs);
        let de = |r: &str| s.iter().find(|x| x.ocorrencia.recorrente == r);
        assert_eq!(de("aluguel").expect("aluguel").movimento, "m_a");
        assert_eq!(de("aluguel").expect("aluguel").valor, Centavos(150_000));
        assert_eq!(de("luz").expect("luz").movimento, "m_l");
        assert!(de("net").is_none());
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| !x.confirmada));
    }

    #[test]
    fn duas_ocorrencias_iguais_disputando_um_movimento_sao_ambiguas() {
        let alvos = [saida("a", 20, 3_990), saida("b", 20, 3_990)];
        let movs = [mov("m", 20, -3_990, "B1", "STREAMING")];
        assert!(sugerir_simples(&alvos, &movs).is_empty());
    }

    #[test]
    fn com_par_confirmado_a_conta_e_exigida_e_sem_par_e_ignorada() {
        let movs = [
            mov("m_outro", 5, -150_000, "OUTRO BANCO", "ALUGUEL"),
            mov("m_certo", 6, -150_000, "MEU BANCO", "ALUGUEL"),
        ];
        // Sem par: o mais próximo do vencimento (a conta não pesa).
        let sem = sugerir_simples(&[saida("a", 5, 150_000)], &movs);
        assert_eq!(sem[0].movimento, "m_outro");
        // Com par: só a conta certa, mesmo mais longe.
        let com = Alvo {
            conta_exigida: Some("meu banco".to_owned()),
            ..saida("a", 5, 150_000)
        };
        let s = sugerir_simples(std::slice::from_ref(&com), &movs);
        assert_eq!(s[0].movimento, "m_certo");
        // Só a conta errada tem o movimento: nada.
        assert!(sugerir_simples(&[com], &movs[..1]).is_empty());
    }

    #[test]
    fn assinatura_acha_o_credito_com_qualquer_valor_dentro_da_janela() {
        let o = ocorrencia("salario", Natureza::Entrada, 5, Some(500_000));
        let com = Alvo {
            assinatura: Some(Assinatura {
                texto: "Salário Empresa".to_owned(),
                janela_dias: 4,
            }),
            conta_exigida: Some("BANCO · 1".to_owned()),
            ..alvo(o)
        };
        let movs = [
            // PLR: valor bem diferente, mas a descrição bate.
            mov("m_plr", 8, 812_345, "BANCO · 1", "SALARIO EMPRESA LTDA"),
            // Mesmo texto na conta errada.
            mov("m_conta", 5, 500_000, "BANCO · 2", "SALARIO EMPRESA"),
            // Fora da janela da assinatura.
            mov("m_longe", 12, 500_000, "BANCO · 1", "SALARIO EMPRESA"),
        ];
        let s = sugerir_simples(&[com], &movs);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].movimento, "m_plr");
        assert_eq!(s[0].valor, Centavos(812_345));
    }

    #[test]
    fn assinatura_sem_valor_previsto_ainda_concilia() {
        let o = ocorrencia("salario", Natureza::Entrada, 5, None);
        let com = Alvo {
            assinatura: Some(Assinatura {
                texto: "salario".to_owned(),
                janela_dias: 5,
            }),
            ..alvo(o)
        };
        let s = sugerir_simples(&[com], &[mov("m", 6, 100_000, "B", "SALARIO X")]);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn o_liquido_da_composicao_e_o_valor_esperado() {
        let o = ocorrencia("salario", Natureza::Entrada, 5, Some(700_000));
        let composicao = Composicao {
            recorrente: "salario".to_owned(),
            desde: Competencia { ano: 2026, mes: 11 },
            rubricas: vec![
                Rubrica {
                    codigo: None,
                    nome: "Bruto".to_owned(),
                    tipo: TipoDeRubrica::Provento,
                    valor: Centavos(600_000),
                    competencia: None,
                    prazo: None,
                    recorrente: None,
                },
                Rubrica {
                    codigo: None,
                    nome: "Desconto".to_owned(),
                    tipo: TipoDeRubrica::Desconto,
                    valor: Centavos(120_000),
                    competencia: None,
                    prazo: None,
                    recorrente: None,
                },
            ],
            documento: None,
            origem: "teste".to_owned(),
        };
        assert_eq!(
            valor_esperado(&o, Some(&composicao)),
            Some(Centavos(480_000))
        );
        assert_eq!(valor_esperado(&o, None), Some(Centavos(700_000)));
        let alvo = Alvo {
            esperado: valor_esperado(&o, Some(&composicao)),
            ..alvo(o)
        };
        // O digitado (7.000,00) não bate; o líquido (4.800,00) sim.
        let movs = [
            mov("m_digitado", 5, 700_000, "B", "OUTRA COISA"),
            mov("m_liquido", 6, 480_000, "B", "OUTRA COISA"),
        ];
        let s = sugerir_simples(&[alvo], &movs);
        assert_eq!(s[0].movimento, "m_liquido");
    }

    #[test]
    fn movimento_ja_usado_nao_serve() {
        let movs = [mov("m", 5, -9_000, "B", "NET")];
        let usados: HashSet<String> = ["m".to_owned()].into();
        let s = sugerir(
            &[saida("net", 5, 9_000)],
            &movs,
            &usados,
            &Config::default(),
            None,
        );
        assert!(s.is_empty());
    }

    #[test]
    fn candidatos_ordenam_assinatura_com_conta_conta_diferenca_e_dias() {
        let o = ocorrencia("salario", Natureza::Entrada, 5, Some(500_000));
        let a = Alvo {
            assinatura: Some(Assinatura {
                texto: "salario".to_owned(),
                janela_dias: 3,
            }),
            conta_exigida: Some("B1".to_owned()),
            ..alvo(o)
        };
        let movs = [
            mov("e_dif", 5, 400_000, "B2", "OUTRO"),
            mov("d_perto", 6, 499_000, "B2", "OUTRO"),
            mov("c_conta", 9, 100_000, "B1", "OUTRO"),
            mov("b_ass_outra", 5, 900_000, "B2", "SALARIO"),
            mov("a_ass_conta", 7, 800_000, "B1", "SALARIO"),
            mov("z_fora", 30, 500_000, "B1", "SALARIO"),
        ];
        let c = candidatos(&a, &movs, &HashSet::new(), &Config::default());
        let ids: Vec<&str> = c.iter().map(|x| x.movimento.movimento.as_str()).collect();
        assert_eq!(
            ids,
            ["a_ass_conta", "c_conta", "b_ass_outra", "d_perto", "e_dif"]
        );
        assert!(c[0].bate_assinatura && c[0].mesma_conta);
        assert_eq!(c[0].diferenca, Some(Centavos(300_000)));
        assert!(!c[1].bate_assinatura && c[1].mesma_conta);
    }

    #[test]
    fn candidatos_respeitam_natureza_e_usados() {
        let a = saida("net", 5, 9_000);
        let movs = [
            mov("entrada", 5, 9_000, "B", "X"),
            mov("usado", 5, -9_000, "B", "X"),
            mov("livre", 7, -9_100, "B", "X"),
        ];
        let usados: HashSet<String> = ["usado".to_owned()].into();
        let c = candidatos(&a, &movs, &usados, &Config::default());
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].movimento.movimento, "livre");
    }

    fn evidencia(apelido: &str, conta: &str, indicio: Indicio) -> Evidencia {
        Evidencia {
            apelido: apelido.to_owned(),
            conta_do_extrato: conta.to_owned(),
            indicio,
        }
    }

    #[test]
    fn pares_escolhem_a_conta_de_mais_peso_e_explicam() {
        let confirmada = || Indicio::Conciliacao { confirmada: true };
        let sugerida = || Indicio::Conciliacao { confirmada: false };
        let evs = [
            evidencia("CX-1", "B1", confirmada()),
            evidencia("CX-1", "B1", sugerida()),
            evidencia("CX-1", "B2", sugerida()),
            evidencia("CX-1", "B2", sugerida()),
            evidencia("CX-1", "B1", Indicio::Assinatura),
            evidencia("CX-2", "B3", sugerida()),
            evidencia("PAREADO", "B9", confirmada()),
        ];
        let ja: HashSet<String> = ["PAREADO".to_owned()].into();
        let p = pares_sugeridos(&evs, &ja, &HashSet::new());
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].apelido, "CX-1");
        assert_eq!(p[0].conta_do_extrato, "B1");
        assert_eq!(p[0].evidencias, 3);
        assert_eq!(
            p[0].motivo,
            "1 conciliação confirmada, 1 conciliação sugerida e 1 crédito da assinatura"
        );
        assert_eq!(p[1].apelido, "CX-2");
        assert_eq!(p[1].evidencias, 1);
    }

    #[test]
    fn conta_disputada_fica_com_o_de_mais_peso_e_empate_avisa() {
        let sugerida = || Indicio::Conciliacao { confirmada: false };
        // Dois apelidos com o mesmo peso querendo a mesma conta: os dois, avisados.
        let empate = [
            evidencia("CX-1", "B1", sugerida()),
            evidencia("CARTAO", "B1", sugerida()),
        ];
        let p = pares_sugeridos(&empate, &HashSet::new(), &HashSet::new());
        assert_eq!(p.len(), 2);
        assert!(p[0].motivo.ends_with("disputada com CX-1"));
        assert!(p[1].motivo.ends_with("disputada com CARTAO"));
        // Com mais peso, um leva.
        let peso = [
            evidencia("CX-1", "B1", sugerida()),
            evidencia("CX-1", "B1", Indicio::Assinatura),
            evidencia("CARTAO", "B1", sugerida()),
        ];
        let p = pares_sugeridos(&peso, &HashSet::new(), &HashSet::new());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].apelido, "CX-1");
        // Conta já confirmada para outro apelido não é proposta.
        let usadas: HashSet<String> = ["B1".to_owned()].into();
        assert!(pares_sugeridos(&peso, &HashSet::new(), &usadas).is_empty());
    }

    #[test]
    fn pares_empatados_nao_propoem_nada() {
        let s = || Indicio::Conciliacao { confirmada: false };
        let evs = [evidencia("CX", "B1", s()), evidencia("CX", "B2", s())];
        assert!(pares_sugeridos(&evs, &HashSet::new(), &HashSet::new()).is_empty());
    }

    #[test]
    fn credito_da_assinatura_pega_o_mais_proximo_e_ignora_a_conta() {
        let a = Alvo {
            assinatura: Some(Assinatura {
                texto: "salario".to_owned(),
                janela_dias: 5,
            }),
            conta_exigida: Some("OUTRA".to_owned()),
            ..alvo(ocorrencia("s", Natureza::Entrada, 5, None))
        };
        let movs = [
            mov("longe", 9, 100, "B1", "SALARIO"),
            mov("perto", 6, 100, "B2", "SALARIO"),
            mov("sem_texto", 5, 100, "B2", "PIX"),
        ];
        let m = credito_da_assinatura(&a, &movs, &Config::default()).expect("credito");
        assert_eq!(m.movimento, "perto");
    }
}
