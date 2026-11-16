//! Texto e dinheiro: normalização de nomes e conversão exata para centavos.

use sbf_planejamento::Centavos;

/// Minúsculas, sem acento, só letras, dígitos e espaço simples. Serve para casar nomes.
pub fn normalizar(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for c in texto.chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            outro => outro.to_ascii_lowercase(),
        };
        if c.is_ascii_alphanumeric() {
            saida.push(c);
        } else if !saida.ends_with(' ') && !saida.is_empty() {
            saida.push(' ');
        }
    }
    saida.trim_end().to_owned()
}

/// O SHA-256 de bytes, em hexadecimal minúsculo.
pub fn sha256_de_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Um decimal em texto para centavos, em aritmética inteira, arredondando meio para longe
/// do zero. Aceita `1234.56`, `-0.1`, `1.234,56` (formato brasileiro) e `R$`.
pub fn centavos_de_texto(texto: &str) -> Option<Centavos> {
    let mut t: String = texto
        .trim()
        .trim_start_matches("R$")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if t.is_empty() {
        return None;
    }
    // Formato brasileiro: o ponto separa milhar e a vírgula, os decimais.
    if t.contains(',') {
        t = t.replace('.', "").replace(',', ".");
    }
    let (negativo, t) = match t.strip_prefix('-') {
        Some(resto) => (true, resto.to_owned()),
        None => (false, t.trim_start_matches('+').to_owned()),
    };
    let (inteiro, fracao) = t.split_once('.').unwrap_or((&t, ""));
    if inteiro.is_empty() && fracao.is_empty() {
        return None;
    }
    if !inteiro.chars().all(|c| c.is_ascii_digit()) || !fracao.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let inteiro: i64 = if inteiro.is_empty() {
        0
    } else {
        inteiro.parse().ok()?
    };
    let mut digitos = fracao.chars().map(|c| i64::from(c as u8 - b'0'));
    let d1 = digitos.next().unwrap_or(0);
    let d2 = digitos.next().unwrap_or(0);
    let d3 = digitos.next().unwrap_or(0);
    let mut total = inteiro.checked_mul(100)?.checked_add(d1 * 10 + d2)?;
    if d3 >= 5 {
        total += 1;
    }
    Some(Centavos(if negativo { -total } else { total }))
}

/// O `f64` de uma célula para centavos. O valor é primeiro escrito com 12 decimais, o que
/// esconde o ruído binário (`0,1 + 0,2` é `0,30000000000000004`), e só então arredondado.
pub fn centavos_de_f64(valor: f64) -> Option<Centavos> {
    if !valor.is_finite() || valor.abs() > 1.0e12 {
        return None;
    }
    centavos_de_texto(&format!("{valor:.12}"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ruido_binario_nao_muda_o_centavo() {
        assert_eq!(centavos_de_f64(0.1 + 0.2), Some(Centavos(30)));
        assert_eq!(centavos_de_f64(1.005), Some(Centavos(101)));
        assert_eq!(centavos_de_f64(2.675), Some(Centavos(268)));
        assert_eq!(centavos_de_f64(-19.9), Some(Centavos(-1990)));
        assert_eq!(centavos_de_f64(1234.56), Some(Centavos(123_456)));
        assert_eq!(centavos_de_f64(0.0), Some(Centavos(0)));
        assert_eq!(centavos_de_f64(f64::NAN), None);
    }

    #[test]
    fn texto_brasileiro_e_ingles() {
        assert_eq!(centavos_de_texto("1.234,56"), Some(Centavos(123_456)));
        assert_eq!(centavos_de_texto("R$ 10,5"), Some(Centavos(1050)));
        assert_eq!(centavos_de_texto("-0.1"), Some(Centavos(-10)));
        assert_eq!(centavos_de_texto("abc"), None);
        assert_eq!(centavos_de_texto(""), None);
    }

    #[test]
    fn normaliza_acento_e_espaco() {
        assert_eq!(
            normalizar("  Plano  de SAÚDE – Unimed "),
            "plano de saude unimed"
        );
    }
}
