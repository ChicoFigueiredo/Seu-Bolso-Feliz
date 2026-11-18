//! As funções puras da tabela `ajuda` que o script enxerga. Ficam em Rust puro (sem Lua)
//! para serem testadas em fronteiras; `no_processo` só as embrulha.

/// Maior inteiro que o número do Luau (f64) representa sem perda.
pub const INTEIRO_MAXIMO: i64 = 1 << 53;

/// `a·b/c` em inteiros, com arredondamento da metade para longe do zero.
///
/// # Errors
/// Divisor zero ou resultado fora de ±2^53 (não cabe no número do Luau sem perda).
pub fn mul_div_arred(a: i64, b: i64, c: i64) -> Result<i64, String> {
    if c == 0 {
        return Err("mul_div_arred: divisor zero".into());
    }
    let n = i128::from(a) * i128::from(b);
    let d = i128::from(c);
    let mut q = n / d;
    let r = n % d;
    if r != 0 && 2 * r.abs() >= d.abs() {
        let sinal = if (n < 0) == (d < 0) { 1 } else { -1 };
        q += sinal;
    }
    let q = i64::try_from(q).map_err(|_| "mul_div_arred: resultado grande demais".to_string())?;
    if q.abs() > INTEIRO_MAXIMO {
        return Err("mul_div_arred: resultado grande demais".into());
    }
    Ok(q)
}

/// `"1.234,56"` → 123456; aceita `R$`, espaços e sinal `-` (antes ou depois do `R$`).
///
/// # Errors
/// Texto que não é um valor em reais com no máximo 2 casas decimais.
pub fn centavos(texto: &str) -> Result<i64, String> {
    let invalido = || {
        format!(
            "centavos: valor inválido ({} caracteres)",
            texto.chars().count()
        )
    };
    let mut t = texto.trim();
    let mut negativo = false;
    loop {
        if let Some(r) = t.strip_prefix("R$") {
            t = r.trim_start();
        } else if let Some(r) = t.strip_prefix('-').or_else(|| t.strip_prefix('\u{2212}')) {
            negativo = !negativo;
            t = r.trim_start();
        } else {
            break;
        }
    }
    if t.is_empty() {
        return Err(invalido());
    }
    let (inteira, fracao) = match t.split_once(',') {
        Some((i, f)) => (i, f),
        None => (t, ""),
    };
    if fracao.len() > 2 || !fracao.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalido());
    }
    // Pontos só como separador de milhar: grupos de 3 depois do primeiro.
    let grupos: Vec<&str> = inteira.split('.').collect();
    let milhar_ok = grupos.iter().enumerate().all(|(i, g)| {
        !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()) && (i == 0 || g.len() == 3)
    });
    if !milhar_ok || (grupos.len() > 1 && grupos[0].len() > 3) {
        return Err(invalido());
    }
    let reais: i64 = grupos.concat().parse().map_err(|_| invalido())?;
    let mut cent: i64 = 0;
    for (i, b) in fracao.bytes().enumerate() {
        cent += i64::from(b - b'0') * if i == 0 { 10 } else { 1 };
    }
    let total = reais
        .checked_mul(100)
        .and_then(|v| v.checked_add(cent))
        .filter(|v| *v <= INTEIRO_MAXIMO)
        .ok_or_else(invalido)?;
    Ok(if negativo { -total } else { total })
}

fn dias_no_mes(ano: i64, mes: i64) -> i64 {
    match mes {
        2 if (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Dias desde 1970-01-01 (algoritmo de Howard Hinnant, calendário gregoriano proléptico).
fn dias_desde_1970(ano: i64, mes: i64, dia: i64) -> i64 {
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = a.div_euclid(400);
    let ano_da_era = a - era * 400;
    let mes_deslocado = (mes + 9) % 12;
    let dia_do_ano = (153 * mes_deslocado + 2) / 5 + dia - 1;
    let dia_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + dia_do_ano;
    era * 146_097 + dia_da_era - 719_468
}

/// O dia da semana ISO (1 = segunda ... 7 = domingo) de uma data de calendário.
fn dia_da_semana_iso(ano: i64, mes: i64, dia: i64) -> i64 {
    // 1970-01-01 foi uma quinta-feira (4).
    (dias_desde_1970(ano, mes, dia) + 3).rem_euclid(7) + 1
}

/// Quantas vezes o dia da semana `dia_semana` (ISO: 1 = segunda ... 7 = domingo) cai no mês.
///
/// # Errors
/// Mês fora de 1 a 12, ano fora de 1 a 9999 ou dia da semana fora de 1 a 7.
pub fn dias_da_semana_no_mes(ano: i64, mes: i64, dia_semana: i64) -> Result<i64, String> {
    if !(1..=9999).contains(&ano) || !(1..=12).contains(&mes) {
        return Err("dias_da_semana_no_mes: mês inválido".into());
    }
    if !(1..=7).contains(&dia_semana) {
        return Err(
            "dias_da_semana_no_mes: o dia da semana vai de 1 (segunda) a 7 (domingo)".into(),
        );
    }
    let dias = dias_no_mes(ano, mes);
    let primeiro = dia_da_semana_iso(ano, mes, 1);
    let primeiro_dia_buscado = 1 + (dia_semana - primeiro).rem_euclid(7);
    if primeiro_dia_buscado > dias {
        return Ok(0);
    }
    Ok((dias - primeiro_dia_buscado) / 7 + 1)
}

/// O nome (sem "-feira", minúsculo) do dia da semana ISO.
///
/// # Errors
/// Dia da semana fora de 1 a 7.
pub fn nome_do_dia_da_semana(dia_semana: i64) -> Result<&'static str, String> {
    Ok(match dia_semana {
        1 => "segunda",
        2 => "terça",
        3 => "quarta",
        4 => "quinta",
        5 => "sexta",
        6 => "sábado",
        7 => "domingo",
        _ => return Err("nome_do_dia_da_semana: o dia vai de 1 a 7".into()),
    })
}

/// `105000` → `"R$ 1.050,00"` (centavos inteiros, sinal na frente).
#[must_use]
pub fn reais(centavos: i64) -> String {
    let negativo = centavos < 0;
    let total = centavos.unsigned_abs();
    let (inteira, fracao) = (total / 100, total % 100);
    let digitos = inteira.to_string();
    let mut agrupada = String::with_capacity(digitos.len() + digitos.len() / 3);
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i).is_multiple_of(3) {
            agrupada.push('.');
        }
        agrupada.push(c);
    }
    format!(
        "{}R$ {agrupada},{fracao:02}",
        if negativo { "-" } else { "" }
    )
}

/// `"06/10/2026"` (ou `-`/`.` como separador, ou já ISO) → `"2026-10-06"`.
///
/// # Errors
/// Texto que não é uma data de calendário válida.
pub fn data(texto: &str) -> Result<String, String> {
    let invalido = || "data: formato inválido".to_string();
    let t = texto.trim();
    let partes: Vec<&str> = t.split(['/', '-', '.']).collect();
    if partes.len() != 3
        || partes
            .iter()
            .any(|p| !p.bytes().all(|b| b.is_ascii_digit()) || p.is_empty())
    {
        return Err(invalido());
    }
    let n = |s: &str| s.parse::<i64>().map_err(|_| invalido());
    let (d, m, a) = if partes[0].len() == 4 {
        (n(partes[2])?, n(partes[1])?, n(partes[0])?)
    } else if partes[2].len() == 4 {
        (n(partes[0])?, n(partes[1])?, n(partes[2])?)
    } else {
        return Err(invalido());
    };
    if !(1..=12).contains(&m) || d < 1 || d > dias_no_mes(a, m) {
        return Err("data: não existe no calendário".into());
    }
    Ok(format!("{a:04}-{m:02}-{d:02}"))
}

fn sem_acento(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "a",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "i",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "u",
        'ç' | 'Ç' => "c",
        'ñ' | 'Ñ' => "n",
        'ý' | 'ÿ' | 'Ý' => "y",
        _ => return None,
    })
}

/// Minúsculas, sem acento e com os espaços colapsados em um.
#[must_use]
pub fn normalizar(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for palavra in texto.split_whitespace() {
        if !saida.is_empty() {
            saida.push(' ');
        }
        for c in palavra.chars() {
            match sem_acento(c) {
                Some(s) => saida.push_str(s),
                None => saida.extend(c.to_lowercase()),
            }
        }
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mul_div_arredonda_metade_para_longe_do_zero() {
        assert_eq!(mul_div_arred(5, 1, 2), Ok(3));
        assert_eq!(mul_div_arred(-5, 1, 2), Ok(-3));
        assert_eq!(mul_div_arred(5, 1, -2), Ok(-3));
        assert_eq!(mul_div_arred(-5, -1, 2), Ok(3));
        assert_eq!(mul_div_arred(7, 1, 3), Ok(2));
        assert_eq!(mul_div_arred(-7, 1, 3), Ok(-2));
        assert_eq!(mul_div_arred(4, 1, 3), Ok(1));
        assert_eq!(mul_div_arred(1, 1, 3), Ok(0));
        assert_eq!(mul_div_arred(0, 99, 7), Ok(0));
        // Fronteira do IRRF: 500.000 × 133145 / 1e6 = 66572,5 → 66573.
        assert_eq!(mul_div_arred(500_000, 133_145, 1_000_000), Ok(66_573));
        // a·b passa de i64 e cabe em i128.
        assert_eq!(mul_div_arred(1 << 50, 1 << 50, 1 << 50), Ok(1 << 50));
    }

    #[test]
    fn mul_div_recusa_divisor_zero_e_estouro() {
        assert!(mul_div_arred(1, 1, 0).is_err());
        assert!(mul_div_arred(i64::MAX, i64::MAX, 1).is_err());
        assert!(mul_div_arred(i64::MAX, 1, 1).is_err());
    }

    #[test]
    fn centavos_le_valores_em_reais() {
        assert_eq!(centavos("1.234,56"), Ok(123_456));
        assert_eq!(centavos("R$ -12,30"), Ok(-1230));
        assert_eq!(centavos("-R$ 12,30"), Ok(-1230));
        assert_eq!(centavos("0,05"), Ok(5));
        assert_eq!(centavos("12,3"), Ok(1230));
        assert_eq!(centavos("12"), Ok(1200));
        assert_eq!(centavos("  R$ 1.000.000,00 "), Ok(100_000_000));
    }

    #[test]
    fn centavos_recusa_lixo() {
        for ruim in [
            "",
            "R$",
            "abc",
            "1,234",
            "1.23,45",
            "12.34",
            "1,2,3",
            "--",
            "1e5",
            "1.2345,00",
            ",50",
        ] {
            assert!(centavos(ruim).is_err(), "{ruim}");
        }
    }

    #[test]
    fn data_converte_e_valida() {
        assert_eq!(data("06/10/2026").as_deref(), Ok("2026-10-06"));
        assert_eq!(data("6-1-2026").as_deref(), Ok("2026-01-06"));
        assert_eq!(data("2026-10-06").as_deref(), Ok("2026-10-06"));
        assert_eq!(data("29/02/2028").as_deref(), Ok("2028-02-29"));
        assert!(data("29/02/2026").is_err());
        assert!(data("31/04/2026").is_err());
        assert!(data("00/01/2026").is_err());
        assert!(data("01/13/2026").is_err());
        assert!(data("01/01/26").is_err());
        assert!(data("amanha").is_err());
    }

    #[test]
    fn dias_da_semana_no_mes_conta_quatro_e_cinco_sextas() {
        // Abril de 2026 tem 4 sextas (3, 10, 17, 24); maio de 2026 tem 5 (1, 8, 15, 22, 29).
        assert_eq!(dias_da_semana_no_mes(2026, 4, 5), Ok(4));
        assert_eq!(dias_da_semana_no_mes(2026, 5, 5), Ok(5));
        assert_eq!(dias_da_semana_no_mes(2026, 1, 5), Ok(5));
        // Fevereiro de 2027 tem 28 dias: toda semana cai 4 vezes.
        for d in 1..=7 {
            assert_eq!(dias_da_semana_no_mes(2027, 2, d), Ok(4), "dia {d}");
        }
        // Fevereiro bissexto: 2028-02-01 é terça, então só a terça cai 5 vezes.
        assert_eq!(dias_da_semana_no_mes(2028, 2, 2), Ok(5));
        assert_eq!(dias_da_semana_no_mes(2028, 2, 3), Ok(4));
        // A soma dos sete dias é o tamanho do mês.
        for (a, m, t) in [(2026, 4, 30), (2026, 5, 31), (2028, 2, 29), (2100, 2, 28)] {
            let soma: i64 = (1..=7)
                .map(|d| dias_da_semana_no_mes(a, m, d).unwrap())
                .sum();
            assert_eq!(soma, t);
        }
    }

    #[test]
    fn dias_da_semana_no_mes_recusa_argumentos_fora_do_intervalo() {
        assert!(dias_da_semana_no_mes(2026, 0, 5).is_err());
        assert!(dias_da_semana_no_mes(2026, 13, 5).is_err());
        assert!(dias_da_semana_no_mes(2026, 5, 0).is_err());
        assert!(dias_da_semana_no_mes(2026, 5, 8).is_err());
        assert!(dias_da_semana_no_mes(0, 5, 5).is_err());
    }

    #[test]
    fn nome_do_dia_e_reais_formatam() {
        assert_eq!(nome_do_dia_da_semana(5), Ok("sexta"));
        assert_eq!(nome_do_dia_da_semana(6), Ok("sábado"));
        assert!(nome_do_dia_da_semana(0).is_err());
        assert_eq!(reais(105_000), "R$ 1.050,00");
        assert_eq!(reais(21_000), "R$ 210,00");
        assert_eq!(reais(5), "R$ 0,05");
        assert_eq!(reais(-123_456_789), "-R$ 1.234.567,89");
        assert_eq!(reais(0), "R$ 0,00");
        assert_eq!(reais(100_000_000), "R$ 1.000.000,00");
    }

    #[test]
    fn normalizar_tira_acento_e_colapsa_espacos() {
        assert_eq!(
            normalizar("  Demonstrativo   de PAGAMENTO \n Ação  "),
            "demonstrativo de pagamento acao"
        );
        assert_eq!(normalizar("Çñ ÀÉÎÕÜ"), "cn aeiou");
    }
}
