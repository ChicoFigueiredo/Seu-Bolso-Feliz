//! Instantes em RFC 3339 (UTC) e dinheiro em texto, sem ponto flutuante.

use bancada_rede::Instante;

/// `2026-11-13T18:04:05Z` (UTC, segundos inteiros).
pub fn instante_iso(instante: Instante) -> String {
    let ms = instante.em_milissegundos();
    let segundos = ms.div_euclid(1000);
    let dias = segundos.div_euclid(86_400);
    let resto = segundos.rem_euclid(86_400);
    let (ano, mes, dia) = civil_de_dias(dias);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60
    )
}

/// Dias desde 1970-01-01 → (ano, mês, dia) (algoritmo de Howard Hinnant).
fn civil_de_dias(dias: i64) -> (i64, i64, i64) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `R$ 1.234,56` (negativo: `-R$ 10,00`).
pub fn dinheiro(centavos: i64) -> String {
    let sinal = if centavos < 0 { "-" } else { "" };
    let absoluto = centavos.unsigned_abs();
    let reais = (absoluto / 100).to_string();
    let mut agrupado = String::new();
    for (i, c) in reais.chars().enumerate() {
        if i > 0 && (reais.len() - i).is_multiple_of(3) {
            agrupado.push('.');
        }
        agrupado.push(c);
    }
    format!("{sinal}R$ {agrupado},{:02}", absoluto % 100)
}

/// `AAAA-MM-DD` → `DD/MM`.
pub fn dia_e_mes(data: &str) -> String {
    match (data.get(5..7), data.get(8..10)) {
        (Some(m), Some(d)) => format!("{d}/{m}"),
        _ => data.to_owned(),
    }
}

/// `AAAA-MM-DD` → `DD/MM/AAAA`.
pub fn data_br(data: &str) -> String {
    match (data.get(0..4), data.get(5..7), data.get(8..10)) {
        (Some(a), Some(m), Some(d)) => format!("{d}/{m}/{a}"),
        _ => data.to_owned(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn instante_em_rfc_3339() {
        assert_eq!(
            instante_iso(Instante::de_milissegundos(0)),
            "1970-01-01T00:00:00Z"
        );
        assert_eq!(
            instante_iso(Instante::de_milissegundos(1_794_589_445_000)),
            "2026-11-13T17:04:05Z"
        );
    }

    #[test]
    fn dinheiro_sem_ponto_flutuante() {
        assert_eq!(dinheiro(10 + 20), "R$ 0,30");
        assert_eq!(dinheiro(123_456_789), "R$ 1.234.567,89");
        assert_eq!(dinheiro(-1000), "-R$ 10,00");
        assert_eq!(dinheiro(100_000), "R$ 1.000,00");
    }

    #[test]
    fn datas_curtas() {
        assert_eq!(dia_e_mes("2026-11-04"), "04/11");
        assert_eq!(data_br("2026-11-04"), "04/11/2026");
    }
}
