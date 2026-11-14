//! Tempo sem dependência: segundos desde 1970 e RFC 3339 em UTC.

use std::time::{SystemTime, UNIX_EPOCH};

/// Segundos desde 1970-01-01 UTC.
pub(crate) fn agora_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `quando` de agora, em RFC 3339 UTC (`2026-11-14T12:00:00Z`).
pub fn agora_rfc3339() -> String {
    rfc3339_de(agora_unix())
}

/// RFC 3339 UTC de um instante em segundos desde 1970.
pub fn rfc3339_de(segundos: u64) -> String {
    let dias = i64::try_from(segundos / 86_400).unwrap_or(0);
    let resto = segundos % 86_400;
    let (ano, mes, dia) = data_civil(dias);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60
    )
}

/// Dias desde 1970-01-01 para (ano, mês, dia), algoritmo de Howard Hinnant.
fn data_civil(dias: i64) -> (i64, i64, i64) {
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let ano = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    (if mes <= 2 { ano + 1 } else { ano }, mes, dia)
}

/// `AAAA-MM` do instante, a chave do teto mensal.
pub(crate) fn mes_de(segundos: u64) -> String {
    rfc3339_de(segundos)[..7].to_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_conhecidas() {
        assert_eq!(rfc3339_de(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_de(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(rfc3339_de(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(mes_de(1_700_000_000), "2023-11");
    }
}
