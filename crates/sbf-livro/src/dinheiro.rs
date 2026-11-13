//! Dinheiro em centavos inteiros e a taxa de juros em milionésimos.
//!
//! O banco guarda `numeric(15,2)`; o servidor lê como texto e converte aqui, sem ponto
//! flutuante (`0.10 + 0.20` dá `0.30`, nunca `0.30000000000000004`).

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Um valor de dinheiro em centavos de real. Positivo ou negativo; a direção de uma
/// movimentação vem do tipo dela, não do sinal.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Centavos(pub i64);

/// Uma taxa de juros (ex.: mensal) em milionésimos, como o `numeric(8,6)` do banco:
/// `0.012500` (1,25% a.m.) é `12_500`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Taxa {
    pub milionesimos: i64,
}

/// O texto não é um valor que o banco grava.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDeValor {
    #[error("valor vazio")]
    Vazio,
    #[error("valor com caractere que não é dígito: {0:?}")]
    NaoNumerico(String),
    #[error("valor com mais casas decimais do que cabem ({casas} no máximo): {texto:?}")]
    CasasDemais { texto: String, casas: u32 },
    #[error("valor fora do limite: {0:?}")]
    ForaDoLimite(String),
}

/// Lê `[-]inteiro[.fração]` com no máximo `casas` casas e devolve o valor escalado por
/// `10^casas`, sem ponto flutuante.
fn ler_decimal(texto: &str, casas: u32) -> Result<i64, ErroDeValor> {
    let t = texto.trim();
    if t.is_empty() {
        return Err(ErroDeValor::Vazio);
    }
    let (negativo, corpo) = match t.strip_prefix('-') {
        Some(resto) => (true, resto),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (inteiro, fracao) = corpo.split_once('.').unwrap_or((corpo, ""));
    let so_digitos = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (inteiro.is_empty() && fracao.is_empty()) || !so_digitos(inteiro) || !so_digitos(fracao) {
        return Err(ErroDeValor::NaoNumerico(texto.to_owned()));
    }
    let casas_lidas = u32::try_from(fracao.len()).unwrap_or(u32::MAX);
    if casas_lidas > casas {
        return Err(ErroDeValor::CasasDemais {
            texto: texto.to_owned(),
            casas,
        });
    }
    let fora = || ErroDeValor::ForaDoLimite(texto.to_owned());
    let escala = 10_i64.pow(casas);
    let parte_inteira: i64 = if inteiro.is_empty() {
        0
    } else {
        inteiro.parse().map_err(|_| fora())?
    };
    let parte_fracao: i64 = if fracao.is_empty() {
        0
    } else {
        fracao.parse::<i64>().map_err(|_| fora())? * 10_i64.pow(casas - casas_lidas)
    };
    let valor = parte_inteira
        .checked_mul(escala)
        .and_then(|v| v.checked_add(parte_fracao))
        .ok_or_else(fora)?;
    Ok(if negativo { -valor } else { valor })
}

impl Centavos {
    pub const ZERO: Centavos = Centavos(0);

    /// Lê o texto de um `numeric(15,2)` (ex.: `"1234.56"`, `"-0.10"`, `"12"`).
    pub fn de_texto(texto: &str) -> Result<Centavos, ErroDeValor> {
        ler_decimal(texto, 2).map(Centavos)
    }

    pub fn abs(self) -> Centavos {
        Centavos(self.0.abs())
    }

    /// O maior entre o valor e zero.
    pub fn ou_zero(self) -> Centavos {
        Centavos(self.0.max(0))
    }

    pub fn e_zero(self) -> bool {
        self.0 == 0
    }
}

/// Escreve o texto que o `numeric` lê de volta: `1234.56`, `-0.01`.
impl fmt::Display for Centavos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sinal = if self.0 < 0 { "-" } else { "" };
        let a = self.0.unsigned_abs();
        write!(f, "{sinal}{}.{:02}", a / 100, a % 100)
    }
}

impl Add for Centavos {
    type Output = Centavos;
    fn add(self, outro: Centavos) -> Centavos {
        Centavos(self.0 + outro.0)
    }
}

impl Sub for Centavos {
    type Output = Centavos;
    fn sub(self, outro: Centavos) -> Centavos {
        Centavos(self.0 - outro.0)
    }
}

impl Neg for Centavos {
    type Output = Centavos;
    fn neg(self) -> Centavos {
        Centavos(-self.0)
    }
}

impl AddAssign for Centavos {
    fn add_assign(&mut self, outro: Centavos) {
        self.0 += outro.0;
    }
}

impl SubAssign for Centavos {
    fn sub_assign(&mut self, outro: Centavos) {
        self.0 -= outro.0;
    }
}

impl Sum for Centavos {
    fn sum<I: Iterator<Item = Centavos>>(iter: I) -> Centavos {
        iter.fold(Centavos::ZERO, Add::add)
    }
}

impl<'a> Sum<&'a Centavos> for Centavos {
    fn sum<I: Iterator<Item = &'a Centavos>>(iter: I) -> Centavos {
        iter.copied().sum()
    }
}

impl Taxa {
    /// Lê o texto de um `numeric(8,6)` (ex.: `"0.012500"`).
    pub fn de_texto(texto: &str) -> Result<Taxa, ErroDeValor> {
        ler_decimal(texto, 6).map(|milionesimos| Taxa { milionesimos })
    }

    /// A taxa como fração (`0.0125`). Só a amortização usa: juros sobre saldo é conta
    /// fracionária por natureza.
    #[allow(clippy::cast_precision_loss)]
    pub fn fracao(self) -> f64 {
        self.milionesimos as f64 / 1_000_000.0
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_texto_do_numeric_sem_ponto_flutuante() {
        assert_eq!(Centavos::de_texto("1234.56"), Ok(Centavos(123_456)));
        assert_eq!(Centavos::de_texto("0.10"), Ok(Centavos(10)));
        assert_eq!(Centavos::de_texto("0.1"), Ok(Centavos(10)));
        assert_eq!(Centavos::de_texto("12"), Ok(Centavos(1200)));
        assert_eq!(Centavos::de_texto("-7.05"), Ok(Centavos(-705)));
        assert_eq!(Centavos::de_texto("-0.01"), Ok(Centavos(-1)));
    }

    #[test]
    fn soma_de_dez_e_vinte_centavos_da_trinta() {
        let a = Centavos::de_texto("0.10").expect("valor");
        let b = Centavos::de_texto("0.20").expect("valor");
        assert_eq!(a + b, Centavos(30));
        assert_eq!((a + b).to_string(), "0.30");
    }

    #[test]
    fn recusa_texto_que_nao_e_valor_de_dinheiro() {
        assert!(Centavos::de_texto("").is_err());
        assert!(Centavos::de_texto("abc").is_err());
        assert!(
            Centavos::de_texto("1.234").is_err(),
            "três casas não é centavo"
        );
        assert!(Centavos::de_texto("1,50").is_err());
        assert!(Centavos::de_texto("99999999999999999999").is_err());
    }

    #[test]
    fn escreve_o_texto_que_o_numeric_le_de_volta() {
        for t in ["0.00", "1234.56", "-7.05", "-0.01", "12.00"] {
            assert_eq!(Centavos::de_texto(t).expect("valor").to_string(), t);
        }
    }

    #[test]
    fn soma_uma_lista_e_tira_o_modulo() {
        let total: Centavos = [Centavos(10), Centavos(-25), Centavos(5)].into_iter().sum();
        assert_eq!(total, Centavos(-10));
        assert_eq!(total.abs(), Centavos(10));
        assert_eq!(-total, Centavos(10));
    }

    #[test]
    fn taxa_le_seis_casas() {
        assert_eq!(
            Taxa::de_texto("0.012500"),
            Ok(Taxa {
                milionesimos: 12_500
            })
        );
        assert_eq!(
            Taxa::de_texto("0.01"),
            Ok(Taxa {
                milionesimos: 10_000
            })
        );
        assert!(Taxa::de_texto("0.0000001").is_err());
        assert!(
            (Taxa {
                milionesimos: 12_500
            }
            .fracao()
                - 0.0125)
                .abs()
                < f64::EPSILON
        );
    }
}
