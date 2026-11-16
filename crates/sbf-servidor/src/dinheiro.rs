//! `numeric` do Postgres → centavos, sem ponto flutuante. O SQL lê o valor como texto
//! (`amount::text`) e esta função faz a conta em inteiros.

use sbf_contrato::Centavos;

/// O texto não é um valor em reais com até 2 casas que caiba em `i64` centavos.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("valor de dinheiro inválido no banco: {0:?}")]
pub struct ValorInvalido(pub String);

/// Converte o texto de um `numeric` (`"1234.56"`, `"-0.10"`, `"12"`, `"0.5"`) em
/// centavos. Casas além da segunda só são aceitas se forem zero: nada é arredondado.
pub fn centavos_do_numeric(texto: &str) -> Result<Centavos, ValorInvalido> {
    let invalido = || ValorInvalido(texto.to_owned());
    let t = texto.trim();
    let (negativo, sem_sinal) = match t.strip_prefix('-') {
        Some(resto) => (true, resto),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (inteira, fracao) = match sem_sinal.split_once('.') {
        Some((i, f)) => (i, f),
        None => (sem_sinal, ""),
    };
    if inteira.is_empty() && fracao.is_empty() {
        return Err(invalido());
    }
    if !inteira.bytes().all(|b| b.is_ascii_digit()) || !fracao.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalido());
    }
    let (duas, resto) = fracao.split_at(fracao.len().min(2));
    if resto.bytes().any(|b| b != b'0') {
        return Err(invalido());
    }
    let mut centavos: i64 = 0;
    for b in inteira.bytes() {
        centavos = centavos
            .checked_mul(10)
            .and_then(|c| c.checked_add(i64::from(b - b'0')))
            .ok_or_else(invalido)?;
    }
    let mut casas = duas.bytes();
    for _ in 0..2 {
        let digito = casas.next().map_or(0, |b| i64::from(b - b'0'));
        centavos = centavos
            .checked_mul(10)
            .and_then(|c| c.checked_add(digito))
            .ok_or_else(invalido)?;
    }
    Ok(if negativo { -centavos } else { centavos })
}

/// Centavos como texto de `numeric` (`1250` → `"12.50"`, `-5` → `"-0.05"`), sem ponto
/// flutuante, para ir ao Postgres como `text` com `::numeric`.
pub fn numeric_de_centavos(centavos: Centavos) -> String {
    let sinal = if centavos < 0 { "-" } else { "" };
    let abs = centavos.unsigned_abs();
    format!("{sinal}{}.{:02}", abs / 100, abs % 100)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn centavos_viram_numeric_e_voltam() {
        for c in [0, 5, 10, 1250, -5, -123_456, i64::MAX / 2] {
            assert_eq!(centavos_do_numeric(&numeric_de_centavos(c)), Ok(c), "{c}");
        }
        assert_eq!(numeric_de_centavos(1250), "12.50");
        assert_eq!(numeric_de_centavos(-5), "-0.05");
    }

    #[test]
    fn valores_com_duas_casas() {
        assert_eq!(centavos_do_numeric("1234.56"), Ok(123_456));
        assert_eq!(centavos_do_numeric("0.10"), Ok(10));
        assert_eq!(centavos_do_numeric("0.20"), Ok(20));
        assert_eq!(centavos_do_numeric("0.01"), Ok(1));
        assert_eq!(centavos_do_numeric("0.00"), Ok(0));
        assert_eq!(centavos_do_numeric("-0.10"), Ok(-10));
        assert_eq!(centavos_do_numeric("-1500.00"), Ok(-150_000));
    }

    #[test]
    fn dez_mais_vinte_centavos_sao_trinta() {
        // O caso que o ponto flutuante erra: 0.1 + 0.2 != 0.3.
        let soma = centavos_do_numeric("0.10").unwrap() + centavos_do_numeric("0.20").unwrap();
        assert_eq!(soma, 30);
    }

    #[test]
    fn menos_casas_e_zeros_a_mais() {
        assert_eq!(centavos_do_numeric("12"), Ok(1200));
        assert_eq!(centavos_do_numeric("0.5"), Ok(50));
        assert_eq!(centavos_do_numeric(".5"), Ok(50));
        assert_eq!(centavos_do_numeric("7."), Ok(700));
        assert_eq!(centavos_do_numeric("3.140000"), Ok(314));
        assert_eq!(centavos_do_numeric("+2.00"), Ok(200));
    }

    #[test]
    fn o_maior_numeric_15_2_cabe() {
        assert_eq!(
            centavos_do_numeric("9999999999999.99"),
            Ok(999_999_999_999_999)
        );
    }

    #[test]
    fn recusa_o_que_exigiria_arredondar_ou_nao_e_numero() {
        for ruim in [
            "0.005",
            "1.001",
            "",
            "-",
            ".",
            "abc",
            "1,50",
            "1e3",
            "NaN",
            "--1",
            "1.2.3",
            " ",
            "99999999999999999999",
        ] {
            assert!(
                centavos_do_numeric(ruim).is_err(),
                "{ruim:?} deveria falhar"
            );
        }
    }
}
