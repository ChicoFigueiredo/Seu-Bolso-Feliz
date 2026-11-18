//! `AAAA-MM-DD` para segundos unix (meia-noite de São Paulo, UTC−3, como o resto do app).

/// Dias desde 1970-01-01 (calendário gregoriano; algoritmo de Howard Hinnant).
fn dias_desde_a_epoca(ano: i64, mes: i64, dia: i64) -> i64 {
    let a = if mes <= 2 { ano - 1 } else { ano };
    let era = a.div_euclid(400);
    let ano_da_era = a - era * 400;
    let mes_deslocado = (mes + 9) % 12;
    let dia_do_ano = (153 * mes_deslocado + 2) / 5 + dia - 1;
    let dia_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + dia_do_ano;
    era * 146_097 + dia_da_era - 719_468
}

/// A meia-noite (de São Paulo) de `texto` (`AAAA-MM-DD`), em segundos unix; `None` se o texto
/// não é uma data.
#[must_use]
pub fn desde_iso(texto: &str) -> Option<u64> {
    if !texto.is_ascii() || texto.len() != 10 {
        return None;
    }
    let b = texto.as_bytes();
    if b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let ano: i64 = texto[0..4].parse().ok()?;
    let mes: i64 = texto[5..7].parse().ok()?;
    let dia: i64 = texto[8..10].parse().ok()?;
    if ano < 2000 || !(1..=12).contains(&mes) || !(1..=31).contains(&dia) {
        return None;
    }
    let segundos = dias_desde_a_epoca(ano, mes, dia) * 86_400 + 3 * 3_600;
    u64::try_from(segundos).ok()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_conhecidas() {
        // 2000-01-01T00:00:00Z = 946684800; meia-noite de São Paulo é 3 h depois.
        assert_eq!(desde_iso("2000-01-01"), Some(946_684_800 + 10_800));
        // 2026-11-18T00:00:00Z = 1794960000.
        assert_eq!(desde_iso("2026-11-18"), Some(1_794_960_000 + 10_800));
        // Ano bissexto: 2024-03-01 é 60 dias depois de 2024-01-01.
        let a = desde_iso("2024-01-01").unwrap();
        assert_eq!(desde_iso("2024-03-01"), Some(a + 60 * 86_400));
    }

    #[test]
    fn texto_que_nao_e_data() {
        for t in [
            "",
            "ontem",
            "2026-13-01",
            "2026/11/18",
            "1999-01-01",
            "2026-11-é8",
            "2026-00-10",
        ] {
            assert_eq!(desde_iso(t), None, "{t}");
        }
    }
}
