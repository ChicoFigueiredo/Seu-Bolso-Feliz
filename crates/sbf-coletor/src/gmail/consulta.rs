//! A composição da consulta do Gmail (`query-builder.ts`): `--query`, `--de` e `--ate`
//! viram uma só string. Pura e testada de propósito.

use chrono::{Days, NaiveDate};

/// As partes da consulta.
#[derive(Debug, Default, Clone)]
pub struct PartesDaConsulta<'a> {
    /// Consulta livre do usuário, ex.: `from:nubank has:attachment`.
    pub consulta: Option<&'a str>,
    /// Início do período, `AAAA-MM-DD` (inclusivo).
    pub de: Option<&'a str>,
    /// Fim do período, `AAAA-MM-DD` (inclusivo).
    pub ate: Option<&'a str>,
}

/// `AAAA-MM-DD` válida (rejeita `2026-02-31`, o que o regex do Bun deixava passar para o
/// `Date`, que normaliza para março).
fn ler_data(iso: &str) -> Option<NaiveDate> {
    let t = iso.trim();
    if t.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(t, "%Y-%m-%d").ok()
}

/// `AAAA-MM-DD` para `AAAA/MM/DD`, que o Gmail exige. `None` para entrada inválida, para que
/// uma data malformada não vire silenciosamente um filtro inesperado.
pub fn data_do_gmail(iso: Option<&str>) -> Option<String> {
    ler_data(iso?).map(|d| d.format("%Y/%m/%d").to_string())
}

/// Soma um dia. O `before:` do Gmail é EXCLUSIVO: sem isto, `--ate 2026-04-30` perderia
/// todas as mensagens do próprio dia 30.
pub fn somar_um_dia(iso: &str) -> Option<String> {
    ler_data(iso)?
        .checked_add_days(Days::new(1))
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Compõe a consulta final; vazia quando não há filtro (o chamador busca só pelo rótulo).
pub fn montar_consulta(partes: &PartesDaConsulta<'_>) -> String {
    let mut termos: Vec<String> = Vec::new();
    if let Some(usuario) = partes.consulta.map(str::trim).filter(|t| !t.is_empty()) {
        termos.push(usuario.to_owned());
    }
    if let Some(depois) = data_do_gmail(partes.de) {
        termos.push(format!("after:{depois}"));
    }
    if let Some(ate) = partes.ate
        && let Some(antes) = somar_um_dia(ate).and_then(|d| data_do_gmail(Some(&d)))
    {
        termos.push(format!("before:{antes}"));
    }
    termos.join(" ")
}

#[cfg(test)]
mod testes {
    use super::*;

    fn p<'a>(
        c: Option<&'a str>,
        de: Option<&'a str>,
        ate: Option<&'a str>,
    ) -> PartesDaConsulta<'a> {
        PartesDaConsulta {
            consulta: c,
            de,
            ate,
        }
    }

    #[test]
    fn data_vira_com_barras_e_invalida_vira_nada() {
        assert_eq!(
            data_do_gmail(Some("2026-04-30")).as_deref(),
            Some("2026/04/30")
        );
        assert_eq!(
            data_do_gmail(Some(" 2026-04-30 ")).as_deref(),
            Some("2026/04/30")
        );
        assert_eq!(data_do_gmail(Some("30/04/2026")), None);
        assert_eq!(data_do_gmail(Some("2026-4-3")), None);
        assert_eq!(data_do_gmail(None), None);
    }

    #[test]
    fn somar_um_dia_vira_o_mes_e_o_ano() {
        assert_eq!(somar_um_dia("2026-04-30").as_deref(), Some("2026-05-01"));
        assert_eq!(somar_um_dia("2026-12-31").as_deref(), Some("2027-01-01"));
        assert_eq!(somar_um_dia("2024-02-28").as_deref(), Some("2024-02-29"));
        assert_eq!(somar_um_dia("lixo"), None);
    }

    #[test]
    fn consulta_composta() {
        assert_eq!(montar_consulta(&p(None, None, None)), "");
        assert_eq!(
            montar_consulta(&p(
                Some("from:nubank has:attachment"),
                Some("2026-01-01"),
                Some("2026-04-30")
            )),
            "from:nubank has:attachment after:2026/01/01 before:2026/05/01"
        );
        assert_eq!(
            montar_consulta(&p(Some("  "), None, Some("2026-04-30"))),
            "before:2026/05/01"
        );
        assert_eq!(montar_consulta(&p(None, Some("amanha"), None)), "");
    }
}
