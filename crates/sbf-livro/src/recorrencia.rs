//! O equivalente mensal de uma recorrência e o total mensal comprometido.
//!
//! Portado de `apps/web/src/app/actions/painel-recorrentes.ts` (`equivalenteMensal` e o
//! laço de `carregarPainelRecorrentes`): semanal ×52/12, quinzenal ×26/12, trimestral ÷3,
//! anual ÷12, personalizada × 30,4368 ÷ intervalo. O TS soma reais em ponto flutuante; aqui
//! a soma é de frações exatas de centavo e só o total é arredondado.
//! Teste de ouro: `tests/ouro_recorrencia.rs` (e o total em `tests/ouro_paineis.rs`).

use std::ops::Add;

use crate::dinheiro::Centavos;
use crate::tipos::{Frequencia, Recorrente, TipoDeMovimentacao};

/// Média de dias por mês (365,25 ÷ 12) do TS, em décimos de milésimo: 30,4368.
const DIAS_POR_MES_DEZ_MILESIMOS: i128 = 304_368;

/// Uma quantia de centavos fracionária e exata: `numerador / denominador`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fracao {
    numerador: i128,
    denominador: i128,
}

fn mdc(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl Fracao {
    pub const ZERO: Fracao = Fracao {
        numerador: 0,
        denominador: 1,
    };

    fn nova(numerador: i128, denominador: i128) -> Fracao {
        let sinal = if denominador < 0 { -1 } else { 1 };
        let d = mdc(numerador, denominador);
        Fracao {
            numerador: sinal * numerador / d,
            denominador: sinal * denominador / d,
        }
    }

    pub fn de_centavos(c: Centavos) -> Fracao {
        Fracao::nova(i128::from(c.0), 1)
    }

    /// O centavo mais próximo; meio centavo arredonda para longe do zero.
    pub fn arredondar(self) -> Centavos {
        let n = self.numerador;
        let d = self.denominador;
        let arredondado = if n >= 0 {
            (2 * n + d) / (2 * d)
        } else {
            -((-2 * n + d) / (2 * d))
        };
        Centavos(i64::try_from(arredondado).unwrap_or(i64::MAX))
    }

    /// Para exibir ou comparar, nunca para somar.
    #[allow(clippy::cast_precision_loss)]
    pub fn como_f64(self) -> f64 {
        self.numerador as f64 / self.denominador as f64
    }
}

impl Add for Fracao {
    type Output = Fracao;
    fn add(self, outra: Fracao) -> Fracao {
        Fracao::nova(
            self.numerador * outra.denominador + outra.numerador * self.denominador,
            self.denominador * outra.denominador,
        )
    }
}

/// Quanto um valor recorrente pesa por mês. `None` na personalizada sem intervalo
/// válido: melhor admitir que não se sabe do que inventar um número.
pub fn equivalente_mensal(
    valor: Centavos,
    frequencia: Frequencia,
    intervalo_dias: Option<i32>,
) -> Option<Fracao> {
    let v = i128::from(valor.0);
    Some(match frequencia {
        Frequencia::Mensal => Fracao::nova(v, 1),
        Frequencia::Semanal => Fracao::nova(v * 52, 12),
        Frequencia::Quinzenal => Fracao::nova(v * 26, 12),
        Frequencia::Trimestral => Fracao::nova(v, 3),
        Frequencia::Anual => Fracao::nova(v, 12),
        Frequencia::Personalizada => {
            let intervalo = i128::from(intervalo_dias.filter(|d| *d > 0)?);
            Fracao::nova(v * DIAS_POR_MES_DEZ_MILESIMOS, intervalo * 10_000)
        }
    })
}

/// O total mensal comprometido pelos recorrentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotalMensal {
    pub total: Centavos,
    /// Há recorrente de valor variável, sem valor, ou personalizado sem intervalo.
    pub estimativa: bool,
}

/// Soma os recorrentes **ativos** que não são receita (receita fixa não é compromisso).
/// O sem valor e o personalizado sem intervalo não somam, só marcam a estimativa; o
/// variável soma e marca.
pub fn total_mensal_comprometido<'a>(
    recorrentes: impl IntoIterator<Item = &'a Recorrente>,
) -> TotalMensal {
    let mut soma = Fracao::ZERO;
    let mut estimativa = false;
    for r in recorrentes {
        if !r.ativo || r.tipo == TipoDeMovimentacao::Receita {
            continue;
        }
        match r
            .valor
            .and_then(|v| equivalente_mensal(v, r.frequencia, r.intervalo_dias))
        {
            None => estimativa = true,
            Some(eq) => {
                soma = soma + eq;
                estimativa |= r.valor_variavel;
            }
        }
    }
    TotalMensal {
        total: soma.arredondar(),
        estimativa,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn soma_de_fracoes_nao_perde_centavo() {
        // 3 assinaturas semanais de 10,00: 3 × 43,333… = 130,00 exatos.
        let um = equivalente_mensal(Centavos(1000), Frequencia::Semanal, None).expect("semanal");
        assert_eq!(um.arredondar(), Centavos(4333));
        assert_eq!((um + um + um).arredondar(), Centavos(13_000));
    }

    #[test]
    fn personalizada_sem_intervalo_nao_inventa_numero() {
        assert_eq!(
            equivalente_mensal(Centavos(100), Frequencia::Personalizada, None),
            None
        );
        assert_eq!(
            equivalente_mensal(Centavos(100), Frequencia::Personalizada, Some(0)),
            None
        );
        assert_eq!(
            equivalente_mensal(Centavos(10_000), Frequencia::Personalizada, Some(10))
                .map(Fracao::arredondar),
            Some(Centavos(30_437))
        );
    }

    #[test]
    fn arredonda_meio_centavo_para_longe_do_zero() {
        assert_eq!(Fracao::nova(5, 2).arredondar(), Centavos(3));
        assert_eq!(Fracao::nova(-5, 2).arredondar(), Centavos(-3));
        assert_eq!(Fracao::nova(7, 3).arredondar(), Centavos(2));
    }
}
