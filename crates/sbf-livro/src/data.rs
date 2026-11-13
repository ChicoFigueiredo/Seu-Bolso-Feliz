//! Data civil (`AAAA-MM-DD`) e o dia de São Paulo de um instante.
//!
//! O dinheiro deste app acontece em America/Sao_Paulo (`apps/web/src/lib/datas.ts`). O
//! Brasil não tem horário de verão desde 2019, então o fuso é UTC−3 fixo para toda data
//! que o livro-caixa usa.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Deslocamento de São Paulo em relação ao UTC, em segundos (UTC−3).
pub const DESLOCAMENTO_SAO_PAULO: i64 = -3 * 3600;

/// Uma data civil, sem hora e sem fuso. Ordena cronologicamente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Data {
    pub ano: i32,
    pub mes: u8,
    pub dia: u8,
}

/// Quantos dias tem o mês.
pub fn dias_no_mes(ano: i32, mes: u8) -> u8 {
    match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0 => 29,
        _ => 28,
    }
}

impl Data {
    /// A data, se existir no calendário.
    pub fn nova(ano: i32, mes: u8, dia: u8) -> Option<Data> {
        ((1..=12).contains(&mes) && dia >= 1 && dia <= dias_no_mes(ano, mes)).then_some(Data {
            ano,
            mes,
            dia,
        })
    }

    /// Lê `AAAA-MM-DD` (o formato do `date` do Postgres).
    pub fn de_iso(texto: &str) -> Option<Data> {
        let b = texto.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let ano = texto.get(0..4)?.parse().ok()?;
        let mes = texto.get(5..7)?.parse().ok()?;
        let dia = texto.get(8..10)?.parse().ok()?;
        Data::nova(ano, mes, dia)
    }

    /// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
    pub fn dias_desde_1970(self) -> i64 {
        let ano = i64::from(self.ano) - i64::from(self.mes <= 2);
        let era = ano.div_euclid(400);
        let yoe = ano.rem_euclid(400);
        let mes = i64::from(self.mes);
        let mp = if mes > 2 { mes - 3 } else { mes + 9 };
        let doy = (153 * mp + 2) / 5 + i64::from(self.dia) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// A data de um número de dias desde 1970-01-01.
    pub fn de_dias_desde_1970(dias: i64) -> Data {
        let z = dias + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let dia = doy - (153 * mp + 2) / 5 + 1;
        let mes = if mp < 10 { mp + 3 } else { mp - 9 };
        let ano = yoe + era * 400 + i64::from(mes <= 2);
        Data {
            ano: i32::try_from(ano).unwrap_or(i32::MAX),
            mes: u8::try_from(mes).unwrap_or(1),
            dia: u8::try_from(dia).unwrap_or(1),
        }
    }

    pub fn somar_dias(self, dias: i64) -> Data {
        Data::de_dias_desde_1970(self.dias_desde_1970() + dias)
    }

    /// Dias de `self` até `outra` (positivo se `outra` vem depois).
    pub fn dias_ate(self, outra: Data) -> i64 {
        outra.dias_desde_1970() - self.dias_desde_1970()
    }

    /// O dia de São Paulo em que cai um instante (segundos desde a época Unix).
    pub fn de_instante_em_sao_paulo(segundos_unix: i64) -> Data {
        Data::de_dias_desde_1970((segundos_unix + DESLOCAMENTO_SAO_PAULO).div_euclid(86_400))
    }

    /// Hoje em São Paulo, pelo relógio do sistema.
    pub fn hoje_em_sao_paulo() -> Data {
        let segundos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
        Data::de_instante_em_sao_paulo(segundos)
    }
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.ano, self.mes, self.dia)
    }
}

impl Serialize for Data {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Data {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Data, D::Error> {
        let texto = String::deserialize(d)?;
        Data::de_iso(&texto)
            .ok_or_else(|| serde::de::Error::custom(format!("data inválida: {texto:?}")))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_e_escreve_iso() {
        let d = Data::de_iso("2024-02-29").expect("bissexto");
        assert_eq!(d.to_string(), "2024-02-29");
        assert!(Data::de_iso("2025-02-29").is_none());
        assert!(Data::de_iso("2025-13-01").is_none());
        assert!(Data::de_iso("2025-1-01").is_none());
    }

    #[test]
    fn conta_dias_pela_epoca() {
        assert_eq!(
            Data::de_iso("1970-01-01").map(Data::dias_desde_1970),
            Some(0)
        );
        assert_eq!(
            Data::de_iso("2000-03-01").map(Data::dias_desde_1970),
            Some(11_017)
        );
        let d = Data::de_iso("2026-12-31").expect("data");
        assert_eq!(d.somar_dias(1).to_string(), "2027-01-01");
        assert_eq!(
            Data::de_iso("2024-03-01").map(|d| d.somar_dias(-1).to_string()),
            Some("2024-02-29".into())
        );
    }

    #[test]
    fn as_21h_de_sao_paulo_ainda_e_o_mesmo_dia() {
        // 2026-11-19 21:00 em São Paulo = 2026-11-20 00:00 UTC.
        let d = Data::de_iso("2026-11-20").expect("data");
        let meia_noite_utc = d.dias_desde_1970() * 86_400;
        assert_eq!(
            Data::de_instante_em_sao_paulo(meia_noite_utc).to_string(),
            "2026-11-19"
        );
        assert_eq!(
            Data::de_instante_em_sao_paulo(meia_noite_utc + 3 * 3600).to_string(),
            "2026-11-20"
        );
    }
}
