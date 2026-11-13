//! Ciclo financeiro (CONTEXT.md › Ciclo), a mesma regra de `workers/inventario/src/ciclo.ts`:
//! do dia de corte à véspera do mesmo dia no mês seguinte, inclusive, chamado pelo mês em
//! que começa. O ciclo 2026-10 vai de 2026-10-20 a 2026-11-19.
//!
//! Trabalha só com datas do calendário (`AAAA-MM-DD`), sem fuso: a data de um documento é
//! um dia, não um instante. O fuso America/Sao_Paulo só importa para "hoje", que o núcleo
//! não calcula.

/// O dia de corte do usuário.
pub const DIA_DE_CORTE: u32 = 20;

/// Um ciclo: id `AAAA-MM` do início e os limites inclusivos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ciclo {
    pub id: String,
    pub inicio: String,
    pub fim: String,
}

fn bissexto(ano: i32) -> bool {
    (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0
}

fn dias_no_mes(ano: i32, mes: u32) -> u32 {
    match mes {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if bissexto(ano) => 29,
        _ => 28,
    }
}

fn mes_anterior(ano: i32, mes: u32) -> (i32, u32) {
    if mes == 1 {
        (ano - 1, 12)
    } else {
        (ano, mes - 1)
    }
}

fn mes_seguinte(ano: i32, mes: u32) -> (i32, u32) {
    if mes == 12 {
        (ano + 1, 1)
    } else {
        (ano, mes + 1)
    }
}

/// Lê `AAAA-MM-DD` de uma data que existe no calendário.
pub fn ler_data(texto: &str) -> Option<(i32, u32, u32)> {
    let b = texto.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digitos = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    let (a, m, d) = (&texto[0..4], &texto[5..7], &texto[8..10]);
    if !(digitos(a) && digitos(m) && digitos(d)) {
        return None;
    }
    let ano: i32 = a.parse().ok()?;
    let mes: u32 = m.parse().ok()?;
    let dia: u32 = d.parse().ok()?;
    if !(1..=12).contains(&mes) || dia < 1 || dia > dias_no_mes(ano, mes) {
        return None;
    }
    Some((ano, mes, dia))
}

/// O ciclo (`AAAA-MM` do início) ao qual a data pertence. `None` se a data não existe.
pub fn ciclo_de(data: &str) -> Option<String> {
    let (ano, mes, dia) = ler_data(data)?;
    let (a, m) = if dia < DIA_DE_CORTE {
        mes_anterior(ano, mes)
    } else {
        (ano, mes)
    };
    Some(format!("{a:04}-{m:02}"))
}

/// Primeiro e último dia (inclusive) do ciclo `AAAA-MM`.
pub fn limites_do_ciclo(id: &str) -> Option<Ciclo> {
    let b = id.as_bytes();
    if b.len() != 7 || b[4] != b'-' {
        return None;
    }
    let ano: i32 = id[0..4].parse().ok()?;
    let mes: u32 = id[5..7].parse().ok()?;
    if !(1..=12).contains(&mes) {
        return None;
    }
    let (a, m) = mes_seguinte(ano, mes);
    Some(Ciclo {
        id: id.to_owned(),
        inicio: format!("{ano:04}-{mes:02}-{DIA_DE_CORTE:02}"),
        fim: format!("{a:04}-{m:02}-{:02}", DIA_DE_CORTE - 1),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ciclo_comeca_no_dia_de_corte() {
        assert_eq!(ciclo_de("2026-10-20").as_deref(), Some("2026-10"));
        assert_eq!(ciclo_de("2026-10-19").as_deref(), Some("2026-09"));
        assert_eq!(ciclo_de("2026-11-19").as_deref(), Some("2026-10"));
        assert_eq!(ciclo_de("2026-01-05").as_deref(), Some("2025-12"));
        assert_eq!(ciclo_de("2024-02-29").as_deref(), Some("2024-02"));
    }

    #[test]
    fn data_inexistente_nao_tem_ciclo() {
        assert_eq!(ciclo_de("2025-02-29"), None);
        assert_eq!(ciclo_de("2026-13-01"), None);
        assert_eq!(ciclo_de("20261001"), None);
        assert_eq!(ciclo_de("2026-1-01"), None);
    }

    #[test]
    fn limites_do_ciclo_de_outubro() {
        let c = limites_do_ciclo("2026-10").expect("ciclo válido");
        assert_eq!(c.inicio, "2026-10-20");
        assert_eq!(c.fim, "2026-11-19");
        let dezembro = limites_do_ciclo("2025-12").expect("ciclo válido");
        assert_eq!(dezembro.fim, "2026-01-19");
    }

    /// O mesmo teste dia a dia de 2024 a 2026 do `ciclo.test.ts`: toda data cai num ciclo
    /// cujos limites a contêm, e dias seguidos nunca pulam um ciclo.
    #[test]
    fn todo_dia_de_2024_a_2026_cabe_no_seu_ciclo() {
        let mut anterior: Option<String> = None;
        for ano in 2024..=2026 {
            for mes in 1..=12 {
                for dia in 1..=dias_no_mes(ano, mes) {
                    let data = format!("{ano:04}-{mes:02}-{dia:02}");
                    let id = ciclo_de(&data).expect("data válida");
                    let c = limites_do_ciclo(&id).expect("ciclo válido");
                    assert!(c.inicio <= data && data <= c.fim, "{data} fora de {c:?}");
                    if let Some(a) = &anterior
                        && *a != id
                    {
                        assert_eq!(dia, DIA_DE_CORTE, "troca de ciclo fora do corte em {data}");
                    }
                    anterior = Some(id);
                }
            }
        }
    }
}
