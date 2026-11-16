//! Os números do topo do controle mensal (spec 11, "Resumo"). Cada um sai das linhas.

use crate::banco::de_rfc3339;
use crate::{Centavos, Data, EstadoDaOcorrencia, Natureza, Ocorrencia, Resumo, Saldo, Sentido};

/// Quantos dias um saldo pode ter, no máximo, para valer como atual (D10).
const DIAS_DE_VALIDADE_DO_SALDO: i64 = 1;

/// O dia de São Paulo em que o saldo foi coletado.
fn dia_da_coleta(saldo: &Saldo) -> Option<Data> {
    de_rfc3339(&saldo.quando).map(Data::de_instante_em_sao_paulo)
}

/// Um saldo desatualizado (coletado há mais de 1 dia) ou com data ilegível.
fn desatualizado(saldo: &Saldo, hoje: Data) -> bool {
    dia_da_coleta(saldo).is_none_or(|d| d.dias_ate(hoje) > DIAS_DE_VALIDADE_DO_SALDO)
}

fn plural(n: u32, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("{n} {singular}")
    } else {
        format!("{n} {plural}")
    }
}

/// Calcula o resumo de um período.
///
/// - `linhas`: as ocorrências que vencem no período, **sem** as suprimidas.
/// - `atrasadas`: as em atraso de períodos anteriores, ainda devidas.
///
/// Só `Natureza::Saida` é saída de caixa; desconto em folha fica à parte e dinheiro de
/// passagem fora de receita e despesa. Dispensada não conta.
pub(crate) fn calcular(
    linhas: &[Ocorrencia],
    atrasadas: &[Ocorrencia],
    saldos: &[Saldo],
    hoje: Data,
) -> Resumo {
    let zero = Centavos(0);
    let mut r = Resumo {
        disponivel_agora: saldos.to_vec(),
        ..Resumo::default()
    };
    let mut entradas_nao_recebidas = zero;
    let mut saidas_nao_pagas = zero;
    let mut passagem_entrada = zero;
    let mut passagem_saida = zero;
    let mut sem_valor = 0_u32;
    let mut estimados = 0_u32;

    let contam = linhas
        .iter()
        .filter(|o| !o.suprimida() && o.estado != EstadoDaOcorrencia::Dispensada);
    for o in contam {
        let previsto = o.previsto.unwrap_or(zero);
        let falta = o.falta().unwrap_or(zero);
        if o.previsto.is_none() || o.pago_informado.is_some() && o.conciliado.is_none() {
            r.a_conferir += 1;
        }
        let paga = matches!(
            o.estado,
            EstadoDaOcorrencia::Conciliada | EstadoDaOcorrencia::PagoInformado
        );
        match o.natureza {
            Natureza::Entrada => {
                r.entradas_previstas += previsto;
                entradas_nao_recebidas += falta;
            }
            Natureza::Saida => {
                r.saidas_de_caixa_previstas += previsto;
                if o.devida_vencida(hoje) {
                    r.em_atraso += falta;
                } else {
                    saidas_nao_pagas += falta;
                }
            }
            Natureza::DescontoEmFolha => r.descontos_em_folha += previsto,
            Natureza::DentroDaFatura => r.compras_no_cartao += previsto,
            Natureza::Passagem { sentido } => match sentido {
                Sentido::Entrada => passagem_entrada += previsto,
                Sentido::Saida => passagem_saida += previsto,
            },
        }
        // O desconto em folha não entra no livre; as demais entram até serem pagas.
        if !paga && !matches!(o.natureza, Natureza::DescontoEmFolha | Natureza::DentroDaFatura) {
            if o.previsto.is_none() {
                sem_valor += 1;
            } else if o.e_incerta() {
                estimados += 1;
            }
        }
    }

    for o in atrasadas
        .iter()
        .filter(|o| matches!(o.natureza, Natureza::Saida))
    {
        r.em_atraso += o.falta().unwrap_or(zero);
        if o.previsto.is_none() {
            sem_valor += 1;
        } else if o.e_incerta() {
            estimados += 1;
        }
    }

    // Dinheiro de passagem que entrou e ainda não saiu; nunca negativo.
    let pendente = passagem_entrada - passagem_saida;
    r.passagem_pendente = if pendente < zero { zero } else { pendente };

    let saldo_total: Centavos = saldos.iter().map(|s| s.valor).sum();
    r.livre_apos_compromissos =
        saldo_total + entradas_nao_recebidas - saidas_nao_pagas - r.em_atraso - r.passagem_pendente;

    if saldos.is_empty() {
        r.lacunas.push(
            "Saldo das contas não encontrado: o livre não conta o dinheiro disponível.".into(),
        );
    } else if saldos.iter().any(|s| desatualizado(s, hoje)) {
        r.lacunas
            .push("Saldo desatualizado: coletado há mais de 1 dia.".into());
    }
    if sem_valor > 0 {
        r.lacunas.push(format!(
            "{} sem valor: o livre não conta o que não tem valor.",
            plural(sem_valor, "compromisso", "compromissos")
        ));
    }
    if estimados > 0 {
        r.lacunas.push(format!(
            "{} com valor estimado.",
            plural(estimados, "compromisso", "compromissos")
        ));
    }
    if r.a_conferir > 0 {
        r.lacunas.push(format!(
            "{} a conferir (pago só informado ou sem valor).",
            plural(r.a_conferir, "item", "itens")
        ));
    }
    r.livre_e_estimativa = saldos.is_empty()
        || saldos.iter().any(|s| desatualizado(s, hoje))
        || sem_valor > 0
        || estimados > 0;
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    fn saldo(quando: &str) -> Saldo {
        Saldo {
            conta: "CX-0001".into(),
            valor: Centavos(100_000),
            quando: quando.into(),
        }
    }

    #[test]
    fn saldo_de_ontem_vale_e_de_anteontem_nao() {
        let hoje = Data::nova(2026, 11, 16).expect("data");
        assert!(!desatualizado(&saldo("2026-11-15T20:00:00Z"), hoje));
        assert!(!desatualizado(&saldo("2026-11-16T09:00:00Z"), hoje));
        assert!(desatualizado(&saldo("2026-11-14T09:00:00Z"), hoje));
        assert!(desatualizado(&saldo("lixo"), hoje));
    }

    #[test]
    fn sem_linhas_e_sem_saldo_o_livre_e_zero_e_estimativa() {
        let hoje = Data::nova(2026, 11, 16).expect("data");
        let r = calcular(&[], &[], &[], hoje);
        assert_eq!(r.livre_apos_compromissos, Centavos(0));
        assert!(r.livre_e_estimativa);
        assert_eq!(r.lacunas.len(), 1);
    }
}
