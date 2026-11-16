//! Amortização de dívidas: SAC (amortização constante), Price (parcela fixa) e Misto
//! (primeira metade SAC, segunda Price), saldo devedor, juros totais e quitação antecipada.
//!
//! Portado de `packages/domain/src/amortization/index.ts`. O dinheiro entra e sai em
//! [`Centavos`], mas a conta de dentro é em reais `f64`, **na mesma ordem de operações do
//! TS**, com o mesmo `roundMoney` (`Math.round(v * 100) / 100`): juros sobre saldo com taxa
//! fracionária e a potência da Price não são conta de centavo inteiro, e só repetindo o TS
//! passo a passo o cronograma bate centavo a centavo. Teste de ouro:
//! `tests/ouro_amortizacao.rs`.
//!
//! Comportamentos do TS mantidos de propósito (o ouro manda):
//! - O arredondamento da amortização constante deixa resíduo: SAC de 1.000,00 em 3 vezes
//!   termina com saldo 0,01; o saldo negativo é mostrado como zero.
//! - No **Misto**, a fase SAC usa metade das parcelas para amortizar o principal inteiro;
//!   a fase Price recebe só o resíduo. Para o CEO conferir: provavelmente não é o "misto"
//!   que o nome promete.

use crate::dinheiro::{Centavos, Taxa};
use crate::tipos::SistemaDeAmortizacao;

/// Os sistemas que têm cronograma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sistema {
    Sac,
    Price,
    Misto,
}

impl Sistema {
    /// O sistema de uma dívida; `None` para `other`/`none`, que não têm cronograma.
    pub fn da_divida(sistema: SistemaDeAmortizacao) -> Option<Sistema> {
        match sistema {
            SistemaDeAmortizacao::Sac => Some(Sistema::Sac),
            SistemaDeAmortizacao::Price => Some(Sistema::Price),
            SistemaDeAmortizacao::Misto => Some(Sistema::Misto),
            SistemaDeAmortizacao::Outro | SistemaDeAmortizacao::Nenhum => None,
        }
    }
}

/// O que define um cronograma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfiguracaoDeAmortizacao {
    pub sistema: Sistema,
    pub principal: Centavos,
    pub taxa_mensal: Taxa,
    pub parcelas: u32,
    /// Valor fixo somado a cada parcela.
    pub seguro_mensal: Centavos,
    /// Taxa administrativa fixa somada a cada parcela.
    pub tarifa_mensal: Centavos,
}

/// Uma parcela do cronograma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParcelaDoCronograma {
    pub numero: u32,
    pub total: Centavos,
    pub principal: Centavos,
    pub juros: Centavos,
    pub seguro: Centavos,
    pub tarifa: Centavos,
    /// Depois desta parcela; nunca negativo.
    pub saldo_devedor: Centavos,
}

/// A simulação de quitar a dívida depois de uma parcela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitacaoAntecipada {
    pub saldo_devedor: Centavos,
    /// O que já foi pago mais o saldo devedor.
    pub total_pago: Centavos,
    /// Juros já pagos.
    pub total_de_juros: Centavos,
    /// Juros do cronograma que deixam de ser pagos.
    pub juros_economizados: Centavos,
}

/// `Math.round` do JS: o inteiro mais próximo, empate para +∞.
fn arredondar_js(x: f64) -> f64 {
    let piso = x.floor();
    if x - piso >= 0.5 { piso + 1.0 } else { piso }
}

/// `roundMoney` do TS.
fn arredondar_dinheiro(reais: f64) -> f64 {
    arredondar_js(reais * 100.0) / 100.0
}

#[allow(clippy::cast_precision_loss)]
fn reais(c: Centavos) -> f64 {
    c.0 as f64 / 100.0
}

#[allow(clippy::cast_possible_truncation)]
fn centavos(reais: f64) -> Centavos {
    Centavos(arredondar_js(reais * 100.0) as i64)
}

/// Uma parcela ainda em reais, como no TS.
#[derive(Debug, Clone, Copy)]
struct ParcelaEmReais {
    numero: u32,
    total: f64,
    principal: f64,
    juros: f64,
    seguro: f64,
    tarifa: f64,
    saldo: f64,
}

#[derive(Clone, Copy)]
struct Parametros {
    principal: f64,
    taxa: f64,
    parcelas: u32,
    seguro: f64,
    tarifa: f64,
}

impl Parametros {
    fn de(cfg: &ConfiguracaoDeAmortizacao) -> Parametros {
        Parametros {
            principal: reais(cfg.principal),
            taxa: cfg.taxa_mensal.fracao(),
            parcelas: cfg.parcelas,
            seguro: reais(cfg.seguro_mensal),
            tarifa: reais(cfg.tarifa_mensal),
        }
    }
}

fn sac(p: &Parametros) -> Vec<ParcelaEmReais> {
    let amortizacao = p.principal / f64::from(p.parcelas);
    let mut saldo = p.principal;
    (1..=p.parcelas)
        .map(|numero| {
            let juros = arredondar_dinheiro(saldo * p.taxa);
            let principal = arredondar_dinheiro(amortizacao);
            let total = arredondar_dinheiro(principal + juros + p.seguro + p.tarifa);
            saldo = arredondar_dinheiro(saldo - principal);
            ParcelaEmReais {
                numero,
                total,
                principal,
                juros,
                seguro: p.seguro,
                tarifa: p.tarifa,
                saldo: saldo.max(0.0),
            }
        })
        .collect()
}

fn price(p: &Parametros) -> Vec<ParcelaEmReais> {
    let n = f64::from(p.parcelas);
    let parcela_fixa = if p.taxa == 0.0 {
        p.principal / n
    } else {
        let fator = (1.0 + p.taxa).powf(n);
        (p.principal * p.taxa * fator) / (fator - 1.0)
    };
    let mut saldo = p.principal;
    (1..=p.parcelas)
        .map(|numero| {
            let juros = arredondar_dinheiro(saldo * p.taxa);
            let principal = arredondar_dinheiro(parcela_fixa - juros);
            let total = arredondar_dinheiro(parcela_fixa + p.seguro + p.tarifa);
            saldo = arredondar_dinheiro(saldo - principal);
            ParcelaEmReais {
                numero,
                total,
                principal,
                juros,
                seguro: p.seguro,
                tarifa: p.tarifa,
                saldo: saldo.max(0.0),
            }
        })
        .collect()
}

fn misto(p: &Parametros) -> Vec<ParcelaEmReais> {
    let meio = p.parcelas / 2;
    let mut fase_sac = sac(&Parametros {
        parcelas: meio,
        ..*p
    });
    let restante = fase_sac.last().map_or(p.principal, |u| u.saldo);
    let fase_price = price(&Parametros {
        principal: restante,
        parcelas: p.parcelas - meio,
        ..*p
    });
    fase_sac.extend(fase_price.into_iter().map(|parcela| ParcelaEmReais {
        numero: parcela.numero + meio,
        ..parcela
    }));
    fase_sac
}

fn em_reais(cfg: &ConfiguracaoDeAmortizacao) -> Vec<ParcelaEmReais> {
    let p = Parametros::de(cfg);
    match cfg.sistema {
        Sistema::Sac => sac(&p),
        Sistema::Price => price(&p),
        Sistema::Misto => misto(&p),
    }
}

/// O cronograma completo (`generateSchedule`).
pub fn cronograma(cfg: &ConfiguracaoDeAmortizacao) -> Vec<ParcelaDoCronograma> {
    em_reais(cfg)
        .into_iter()
        .map(|p| ParcelaDoCronograma {
            numero: p.numero,
            total: centavos(p.total),
            principal: centavos(p.principal),
            juros: centavos(p.juros),
            seguro: centavos(p.seguro),
            tarifa: centavos(p.tarifa),
            saldo_devedor: centavos(p.saldo),
        })
        .collect()
}

/// O saldo devedor depois de `pagas` parcelas (`getOutstandingBalanceAfter`).
pub fn saldo_devedor_apos(cfg: &ConfiguracaoDeAmortizacao, pagas: u32) -> Centavos {
    let parcelas = em_reais(cfg);
    let pagas = pagas as usize;
    if pagas >= parcelas.len() {
        Centavos::ZERO
    } else if pagas == 0 {
        cfg.principal
    } else {
        centavos(parcelas[pagas - 1].saldo)
    }
}

/// Os juros de todo o cronograma (`totalInterestPaid`).
pub fn total_de_juros(cfg: &ConfiguracaoDeAmortizacao) -> Centavos {
    let soma: f64 = em_reais(cfg).iter().fold(0.0, |s, p| s + p.juros);
    centavos(arredondar_dinheiro(soma))
}

/// Quitar depois da parcela `apos` (`simulateEarlyPayoff`).
pub fn quitacao_antecipada(cfg: &ConfiguracaoDeAmortizacao, apos: u32) -> QuitacaoAntecipada {
    let parcelas = em_reais(cfg);
    let juros_do_cronograma = parcelas.iter().fold(0.0, |s, p| s + p.juros);
    let apos = apos as usize;
    if apos >= parcelas.len() {
        let total = parcelas.iter().fold(0.0, |s, p| s + p.total);
        return QuitacaoAntecipada {
            saldo_devedor: Centavos::ZERO,
            total_pago: centavos(arredondar_dinheiro(total)),
            total_de_juros: centavos(arredondar_dinheiro(juros_do_cronograma)),
            juros_economizados: Centavos::ZERO,
        };
    }
    let pagas = &parcelas[..apos];
    let total_pago = arredondar_dinheiro(pagas.iter().fold(0.0, |s, p| s + p.total));
    let juros_pagos = arredondar_dinheiro(pagas.iter().fold(0.0, |s, p| s + p.juros));
    let saldo = if apos > 0 {
        parcelas[apos - 1].saldo
    } else {
        reais(cfg.principal)
    };
    QuitacaoAntecipada {
        saldo_devedor: centavos(arredondar_dinheiro(saldo)),
        total_pago: centavos(arredondar_dinheiro(total_pago + saldo)),
        total_de_juros: centavos(juros_pagos),
        juros_economizados: centavos(arredondar_dinheiro(juros_do_cronograma - juros_pagos)),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cfg(
        sistema: Sistema,
        principal: i64,
        milionesimos: i64,
        parcelas: u32,
    ) -> ConfiguracaoDeAmortizacao {
        ConfiguracaoDeAmortizacao {
            sistema,
            principal: Centavos(principal),
            taxa_mensal: Taxa { milionesimos },
            parcelas,
            seguro_mensal: Centavos::ZERO,
            tarifa_mensal: Centavos::ZERO,
        }
    }

    #[test]
    fn sac_de_mil_reais_em_quatro_sem_juros() {
        let c = cronograma(&cfg(Sistema::Sac, 100_000, 0, 4));
        assert_eq!(c.len(), 4);
        assert!(
            c.iter()
                .all(|p| p.principal == Centavos(25_000) && p.juros.e_zero())
        );
        assert_eq!(c[3].saldo_devedor, Centavos::ZERO);
    }

    #[test]
    fn price_tem_parcela_fixa() {
        // 10.000,00 a 1% a.m. em 12: parcela de 888,49 (tabela Price conhecida).
        let c = cronograma(&cfg(Sistema::Price, 1_000_000, 10_000, 12));
        assert!(c.iter().all(|p| p.total == Centavos(88_849)), "{c:?}");
        assert_eq!(c[0].juros, Centavos(10_000));
    }

    #[test]
    fn arredonda_meio_centavo_para_cima_como_o_js() {
        assert!((arredondar_js(2.5) - 3.0).abs() < f64::EPSILON);
        assert!((arredondar_js(-2.5) + 2.0).abs() < f64::EPSILON);
    }
}
