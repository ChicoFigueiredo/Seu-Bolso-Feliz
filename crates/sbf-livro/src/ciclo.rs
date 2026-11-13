//! O ciclo financeiro: começa no dia de corte e termina na véspera do mesmo dia no mês
//! seguinte. Chamado pelo mês em que começa (`CONTEXT.md`, Ciclo).
//!
//! Portado de `packages/domain/src/financial-cycle/index.ts` (`getCurrentPeriod`), com o
//! dia de corte que excede o mês preso ao último dia do mês, como lá. Diferença
//! intencional: o TS usa os getters locais de `Date` e, na Vercel (UTC), devolve o início e
//! o fim um dia antes e troca de ciclo às 21h de São Paulo. Aqui o instante vira primeiro
//! o dia de São Paulo ([`Data::de_instante_em_sao_paulo`]), como o inventário da fatia 1.
//! Teste de ouro: `tests/ouro_ciclo.rs`.

use serde::{Deserialize, Serialize};

use crate::data::{Data, dias_no_mes};

/// O dia do mês em que o ciclo começa (1 a 31). Padrão 20 (grill Q14); o valor do
/// usuário vem de `user_financial_preferences.financial_cycle_start_day`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct DiaDeCorte(u8);

impl DiaDeCorte {
    /// O padrão quando o usuário não gravou preferência (grill Q14).
    pub const PADRAO: DiaDeCorte = DiaDeCorte(20);

    pub fn novo(dia: u8) -> Option<DiaDeCorte> {
        (1..=31).contains(&dia).then_some(DiaDeCorte(dia))
    }

    /// O corte gravado, ou o padrão 20 quando não há (ou quando o valor é inválido).
    pub fn da_preferencia(dia: Option<i32>) -> DiaDeCorte {
        dia.and_then(|d| u8::try_from(d).ok())
            .and_then(DiaDeCorte::novo)
            .unwrap_or(DiaDeCorte::PADRAO)
    }

    pub fn dia(self) -> u8 {
        self.0
    }

    /// O dia em que o ciclo começa naquele mês: o corte, ou o último dia do mês.
    fn no_mes(self, ano: i32, mes: u8) -> u8 {
        self.0.min(dias_no_mes(ano, mes))
    }
}

impl Default for DiaDeCorte {
    fn default() -> Self {
        DiaDeCorte::PADRAO
    }
}

impl TryFrom<u8> for DiaDeCorte {
    type Error = String;
    fn try_from(dia: u8) -> Result<Self, Self::Error> {
        DiaDeCorte::novo(dia).ok_or_else(|| format!("dia de corte fora de 1..=31: {dia}"))
    }
}

impl From<DiaDeCorte> for u8 {
    fn from(c: DiaDeCorte) -> u8 {
        c.0
    }
}

/// Um ciclo: do `inicio` ao `fim`, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Ciclo {
    pub inicio: Data,
    pub fim: Data,
    pub corte: DiaDeCorte,
}

fn mes_seguinte(ano: i32, mes: u8) -> (i32, u8) {
    if mes >= 12 {
        (ano + 1, 1)
    } else {
        (ano, mes + 1)
    }
}

fn mes_anterior(ano: i32, mes: u8) -> (i32, u8) {
    if mes <= 1 {
        (ano - 1, 12)
    } else {
        (ano, mes - 1)
    }
}

impl Ciclo {
    /// O ciclo que começa no mês `ano`-`mes`.
    pub fn do_mes(ano: i32, mes: u8, corte: DiaDeCorte) -> Ciclo {
        let inicio = Data {
            ano,
            mes,
            dia: corte.no_mes(ano, mes),
        };
        let (a, m) = mes_seguinte(ano, mes);
        let proximo = Data {
            ano: a,
            mes: m,
            dia: corte.no_mes(a, m),
        };
        Ciclo {
            inicio,
            fim: proximo.somar_dias(-1),
            corte,
        }
    }

    /// O ciclo a que pertence uma data civil.
    pub fn da_data(data: Data, corte: DiaDeCorte) -> Ciclo {
        let (ano, mes) = if data.dia < corte.no_mes(data.ano, data.mes) {
            mes_anterior(data.ano, data.mes)
        } else {
            (data.ano, data.mes)
        };
        Ciclo::do_mes(ano, mes, corte)
    }

    /// O ciclo de um instante (segundos Unix), pelo dia de São Paulo.
    pub fn do_instante(segundos_unix: i64, corte: DiaDeCorte) -> Ciclo {
        Ciclo::da_data(Data::de_instante_em_sao_paulo(segundos_unix), corte)
    }

    /// O ciclo de hoje em São Paulo.
    pub fn atual(corte: DiaDeCorte) -> Ciclo {
        Ciclo::da_data(Data::hoje_em_sao_paulo(), corte)
    }

    /// O ciclo pelo nome `AAAA-MM`.
    pub fn do_nome(nome: &str, corte: DiaDeCorte) -> Option<Ciclo> {
        if nome.len() != 7 || nome.as_bytes().get(4) != Some(&b'-') {
            return None;
        }
        let ano = nome.get(0..4)?.parse().ok()?;
        let mes: u8 = nome.get(5..7)?.parse().ok()?;
        (1..=12)
            .contains(&mes)
            .then(|| Ciclo::do_mes(ano, mes, corte))
    }

    /// O nome do ciclo: o mês em que começa, `AAAA-MM`.
    pub fn nome(&self) -> String {
        format!("{:04}-{:02}", self.inicio.ano, self.inicio.mes)
    }

    /// O rótulo do TS (`formatPeriodLabel`): `20/10/2026 a 19/11/2026`.
    pub fn rotulo(&self) -> String {
        let f = |d: Data| format!("{:02}/{:02}/{:04}", d.dia, d.mes, d.ano);
        format!("{} a {}", f(self.inicio), f(self.fim))
    }

    pub fn contem(&self, data: Data) -> bool {
        self.inicio <= data && data <= self.fim
    }

    pub fn seguinte(&self) -> Ciclo {
        let (a, m) = mes_seguinte(self.inicio.ano, self.inicio.mes);
        Ciclo::do_mes(a, m, self.corte)
    }

    pub fn anterior(&self) -> Ciclo {
        let (a, m) = mes_anterior(self.inicio.ano, self.inicio.mes);
        Ciclo::do_mes(a, m, self.corte)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(t: &str) -> Data {
        Data::de_iso(t).expect("data")
    }

    #[test]
    fn ciclo_2026_10_vai_de_20_de_outubro_a_19_de_novembro() {
        let c = Ciclo::da_data(d("2026-11-19"), DiaDeCorte::PADRAO);
        assert_eq!(
            (c.nome(), c.inicio, c.fim),
            ("2026-10".into(), d("2026-10-20"), d("2026-11-19"))
        );
        assert_eq!(
            Ciclo::da_data(d("2026-11-20"), DiaDeCorte::PADRAO).nome(),
            "2026-11"
        );
    }

    #[test]
    fn corte_31_em_fevereiro_vira_o_ultimo_dia() {
        let corte = DiaDeCorte::novo(31).expect("corte");
        let c = Ciclo::da_data(d("2026-03-01"), corte);
        assert_eq!((c.inicio, c.fim), (d("2026-02-28"), d("2026-03-30")));
        assert_eq!(c.seguinte().inicio, d("2026-03-31"));
        assert_eq!(c.anterior().inicio, d("2026-01-31"));
    }

    #[test]
    fn sem_preferencia_o_corte_e_20() {
        assert_eq!(DiaDeCorte::da_preferencia(None).dia(), 20);
        assert_eq!(DiaDeCorte::da_preferencia(Some(0)).dia(), 20);
        assert_eq!(DiaDeCorte::da_preferencia(Some(5)).dia(), 5);
    }

    #[test]
    fn le_o_ciclo_pelo_nome() {
        let c = Ciclo::do_nome("2026-12", DiaDeCorte::PADRAO).expect("nome");
        assert_eq!((c.inicio, c.fim), (d("2026-12-20"), d("2027-01-19")));
        assert!(Ciclo::do_nome("2026-13", DiaDeCorte::PADRAO).is_none());
    }
}
