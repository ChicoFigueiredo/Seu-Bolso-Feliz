//! A **agenda** do roteiro escrito (spec 18): dado o manifesto, o relógio e o que já rodou,
//! diz quais entradas estão devidas.
//!
//! Regra (rodar quando devido, uma vez ao voltar): a entrada está devida se existe um instante
//! agendado `s` com `base < s <= agora`, onde `base` é o maior entre o início da última
//! execução DAQUELA entrada e o instante em que a versão foi ativada (senão ativar no dia 25
//! dispararia o "dia 20" na hora). Roda **uma única vez**, não uma por horário perdido; o
//! instante devolvido é o mais recente. Duas entradas devidas na mesma passada voltam na ordem
//! da lista.
//!
//! Horários e dias do mês são do fuso do usuário (UTC−3), nunca UTC puro: às 21h do dia 30 em
//! São Paulo o UTC já está no dia 1. O dia 31 num mês sem 31 não dispara nesse mês (sem ajuste).

use crate::escrito::{FUSO_EM_SEGUNDOS, data_dos_dias, dias_da_data};
use crate::manifesto::{EntradaDeAgenda, Manifesto};

/// Quantos dias para trás se procura um instante agendado, no máximo (cobre o `dias_do_mes`
/// mais raro: o dia 29 só existe em fevereiro bissexto, de 4 em 4 anos).
const DIAS_DE_BUSCA: i64 = 3_000;

/// Uma entrada da agenda que está devida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Devida {
    /// A posição da entrada em `Manifesto::agenda`.
    pub entrada: usize,
    /// O instante agendado (unix) que a tornou devida: o mais recente em `(base, agora]`.
    pub previsto_unix: i64,
}

/// O instante agendado mais recente de `entrada` em `(base, agora]`, se houver.
#[must_use]
pub fn ultimo_instante(entrada: &EntradaDeAgenda, base_unix: i64, agora_unix: i64) -> Option<i64> {
    if base_unix >= agora_unix {
        return None;
    }
    let mut horarios = entrada.horarios_em_minutos();
    // Do mais tarde para o mais cedo: o primeiro que cabe é o mais recente do dia.
    horarios.sort_unstable_by(|a, b| b.cmp(a));
    let dia_de_agora = agora_unix
        .saturating_add(FUSO_EM_SEGUNDOS)
        .div_euclid(86_400);
    let dia_da_base = base_unix
        .saturating_add(FUSO_EM_SEGUNDOS)
        .div_euclid(86_400);
    let primeiro_dia = dia_da_base.max(dia_de_agora - DIAS_DE_BUSCA);
    let mut dia = dia_de_agora;
    while dia >= primeiro_dia {
        if dia_vale(entrada, dia) {
            for &min in &horarios {
                let local = dia * 86_400 + i64::from(min) * 60;
                let s = local - FUSO_EM_SEGUNDOS;
                if s <= agora_unix && s > base_unix {
                    return Some(s);
                }
            }
        }
        dia -= 1;
    }
    None
}

/// O dia (local, em dias desde 1970) está nos `dias_do_mes` da entrada (ou ela é de todo dia)?
fn dia_vale(entrada: &EntradaDeAgenda, dia_local: i64) -> bool {
    let Some(dias) = &entrada.dias_do_mes else {
        return true;
    };
    let (_, _, d) = data_dos_dias(dia_local);
    dias.iter().any(|x| i64::from(*x) == d)
}

/// As entradas da agenda do manifesto que estão devidas em `agora_unix`. `bases[i]` é a base da
/// entrada `i` (o maior entre o início da última execução dela e a ativação da versão); uma
/// entrada sem base conhecida não é devida (na dúvida, não dispara).
#[must_use]
pub fn devidas(manifesto: &Manifesto, agora_unix: i64, bases: &[i64]) -> Vec<Devida> {
    manifesto
        .agenda
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let base = *bases.get(i)?;
            ultimo_instante(e, base, agora_unix).map(|s| Devida {
                entrada: i,
                previsto_unix: s,
            })
        })
        .collect()
}

/// O instante unix de uma data e hora **locais** (UTC−3). Para os testes e a documentação.
#[must_use]
pub fn instante_local(ano: i64, mes: i64, dia: i64, hora: i64, minuto: i64) -> i64 {
    dias_da_data(ano, mes, dia) * 86_400 + hora * 3_600 + minuto * 60 - FUSO_EM_SEGUNDOS
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use std::collections::BTreeMap;

    fn entrada(horarios: &[&str], dias: Option<&[u8]>) -> EntradaDeAgenda {
        EntradaDeAgenda {
            horarios: horarios.iter().map(|h| (*h).to_owned()).collect(),
            dias_do_mes: dias.map(<[u8]>::to_vec),
            parametros: BTreeMap::new(),
        }
    }

    /// Unix de uma data/hora local.
    fn l(a: i64, m: i64, d: i64, h: i64, mi: i64) -> i64 {
        instante_local(a, m, d, h, mi)
    }

    #[test]
    fn o_instante_local_confere_com_o_utc() {
        // 2026-11-25T12:00:00Z = 09:00 de 25/11 em UTC−3
        assert_eq!(l(2026, 11, 25, 9, 0), 1_795_608_000);
        // 21h do dia 30 em UTC−3 = 00h do dia 1 em UTC
        assert_eq!(l(2026, 11, 30, 21, 0), l(2026, 12, 1, 0, 0) - 3 * 3600);
        assert_eq!(l(2026, 11, 30, 21, 0), 1_796_083_200);
    }

    #[test]
    fn todo_dia_devolve_o_horario_mais_recente_ja_passado() {
        let e = entrada(&["04:00", "12:00", "20:00"], None);
        let base = l(2026, 11, 20, 0, 0);
        // 13h: o mais recente é o das 12h do mesmo dia
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 11, 25, 13, 0)),
            Some(l(2026, 11, 25, 12, 0))
        );
        // exatamente às 12:00 vale (s <= agora)
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 11, 25, 12, 0)),
            Some(l(2026, 11, 25, 12, 0))
        );
        // 03:00: o mais recente é o das 20h de ontem
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 11, 25, 3, 0)),
            Some(l(2026, 11, 24, 20, 0))
        );
    }

    #[test]
    fn catch_up_roda_uma_vez_so_com_o_instante_mais_recente() {
        let e = entrada(&["04:00", "12:00", "20:00"], None);
        // Última execução às 04:00 de 22/11; voltou no dia 25 às 21h: perdeu uns 9 horários,
        // mas é UMA devida (s = 20:00 de 25/11).
        let base = l(2026, 11, 22, 4, 0);
        let s = ultimo_instante(&e, base, l(2026, 11, 25, 21, 0)).unwrap();
        assert_eq!(s, l(2026, 11, 25, 20, 0));
        // Depois de rodar (início em 21:00), nada devido até o próximo horário.
        let base2 = l(2026, 11, 25, 21, 0);
        assert_eq!(ultimo_instante(&e, base2, l(2026, 11, 25, 23, 59)), None);
        assert_eq!(
            ultimo_instante(&e, base2, l(2026, 11, 26, 4, 0)),
            Some(l(2026, 11, 26, 4, 0))
        );
    }

    #[test]
    fn instante_antes_da_base_nao_conta() {
        let e = entrada(&["12:00"], None);
        // Rodou às 12:00:05 (depois do instante): o das 12:00 já foi cumprido.
        let base = l(2026, 11, 25, 12, 0) + 5;
        assert_eq!(ultimo_instante(&e, base, l(2026, 11, 25, 18, 0)), None);
        // base == agora nunca é devido
        assert_eq!(ultimo_instante(&e, 100, 100), None);
        assert_eq!(ultimo_instante(&e, 200, 100), None);
    }

    #[test]
    fn dias_do_mes_so_nos_dias_listados() {
        let e = entrada(&["09:00"], Some(&[20, 21, 30, 1]));
        let base = l(2026, 11, 1, 10, 0);
        // dia 25: o último devido foi o do dia 21
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 11, 25, 10, 0)),
            Some(l(2026, 11, 21, 9, 0))
        );
        // dia 30 às 10h
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 11, 30, 10, 0)),
            Some(l(2026, 11, 30, 9, 0))
        );
        // dia 30 às 8h: ainda o do 21
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 11, 30, 8, 0)),
            Some(l(2026, 11, 21, 9, 0))
        );
        // entre o dia 21 (já rodou) e o 30: nada
        let base = l(2026, 11, 21, 9, 1);
        assert_eq!(ultimo_instante(&e, base, l(2026, 11, 29, 23, 0)), None);
    }

    #[test]
    fn dia_31_nao_dispara_em_mes_sem_31() {
        let e = entrada(&["09:00"], Some(&[31]));
        // novembro tem 30 dias: nada no mês; o último 31 foi 31/10
        let base = l(2026, 10, 31, 9, 1);
        assert_eq!(ultimo_instante(&e, base, l(2026, 11, 30, 23, 59)), None);
        assert_eq!(ultimo_instante(&e, base, l(2026, 12, 1, 0, 0)), None);
        assert_eq!(
            ultimo_instante(&e, base, l(2026, 12, 31, 9, 0)),
            Some(l(2026, 12, 31, 9, 0))
        );
        // procurando para trás a partir de novembro acha o 31/10
        let antes = l(2026, 10, 1, 0, 0);
        assert_eq!(
            ultimo_instante(&e, antes, l(2026, 11, 20, 12, 0)),
            Some(l(2026, 10, 31, 9, 0))
        );
        // dia 29/30 em fevereiro de ano comum não existem; 29 existe em bissexto
        let e29 = entrada(&["09:00"], Some(&[29]));
        let base = l(2027, 1, 29, 10, 0);
        assert_eq!(ultimo_instante(&e29, base, l(2027, 3, 28, 12, 0)), None);
        assert_eq!(
            ultimo_instante(&e29, base, l(2027, 3, 29, 12, 0)),
            Some(l(2027, 3, 29, 9, 0))
        );
        let base = l(2028, 1, 29, 10, 0);
        assert_eq!(
            ultimo_instante(&e29, base, l(2028, 2, 29, 12, 0)),
            Some(l(2028, 2, 29, 9, 0))
        );
    }

    #[test]
    fn virada_de_mes_e_de_ano() {
        let e = entrada(&["00:30"], Some(&[1]));
        let base = l(2026, 12, 1, 1, 0);
        assert_eq!(
            ultimo_instante(&e, base, l(2027, 1, 1, 0, 30)),
            Some(l(2027, 1, 1, 0, 30))
        );
        assert_eq!(ultimo_instante(&e, base, l(2027, 1, 1, 0, 29)), None);
        // virada para um ano bissexto, procurando o dia 1 de janeiro
        let e = entrada(&["23:59"], Some(&[31, 1]));
        let base = l(2027, 12, 31, 23, 59);
        assert_eq!(
            ultimo_instante(&e, base, l(2028, 1, 1, 23, 59)),
            Some(l(2028, 1, 1, 23, 59))
        );
        // de dezembro para janeiro: ontem às 23:59 foi o dia 31
        let base = l(2027, 12, 1, 0, 0);
        assert_eq!(
            ultimo_instante(&e, base, l(2028, 1, 1, 12, 0)),
            Some(l(2027, 12, 31, 23, 59))
        );
    }

    #[test]
    fn fuso_21h_do_dia_30_nao_e_dia_1() {
        // 21:00 do dia 30 (UTC−3) já é 00:00 do dia 1 em UTC.
        let dia30 = entrada(&["21:00"], Some(&[30]));
        let dia1 = entrada(&["21:00"], Some(&[1]));
        let base = l(2026, 11, 25, 0, 0);
        let agora = l(2026, 11, 30, 21, 0);
        assert_eq!(agora, l(2026, 12, 1, 0, 0) - 3 * 3600);
        assert_eq!(ultimo_instante(&dia30, base, agora), Some(agora));
        // o "dia 1" ainda não chegou para o usuário
        assert_eq!(ultimo_instante(&dia1, base, agora), None);
        // e a meia-noite do usuário (03:00Z do dia 1) vale para o dia 1, às 21:00 do dia 1
        let meia_noite = l(2026, 12, 1, 0, 0);
        assert_eq!(ultimo_instante(&dia1, agora, meia_noite), None);
        assert_eq!(
            ultimo_instante(&dia1, agora, l(2026, 12, 1, 21, 0)),
            Some(l(2026, 12, 1, 21, 0))
        );
    }

    #[test]
    fn ativacao_posterior_ao_instante_nao_dispara() {
        let e = entrada(&["04:00"], Some(&[20]));
        // Ativada no dia 25 às 10h: o "dia 20" ficou para trás e não dispara.
        let ativacao = l(2026, 11, 25, 10, 0);
        assert_eq!(ultimo_instante(&e, ativacao, l(2026, 11, 25, 10, 1)), None);
        assert_eq!(ultimo_instante(&e, ativacao, l(2026, 12, 19, 23, 0)), None);
        assert_eq!(
            ultimo_instante(&e, ativacao, l(2026, 12, 20, 4, 0)),
            Some(l(2026, 12, 20, 4, 0))
        );
    }

    #[test]
    fn dia_29_em_ano_nao_bissexto_cai_so_nos_meses_que_tem() {
        // o gap mais longo entre dois 29/02 é de 8 anos (2096 -> 2104)
        let e = entrada(&["09:00"], Some(&[29]));
        let base = l(2100, 1, 1, 0, 0);
        // 2100 não é bissexto (fevereiro não tem 29), mas janeiro tem: o 29/01 às 9h.
        assert_eq!(
            ultimo_instante(&e, base, l(2100, 1, 30, 0, 0)),
            Some(l(2100, 1, 29, 9, 0))
        );
    }

    fn manifesto_de(agenda: Vec<EntradaDeAgenda>) -> Manifesto {
        Manifesto {
            nome: "x".into(),
            descricao: String::new(),
            url_inicial: "https://portal.exemplo.test/".into(),
            hosts: vec!["portal.exemplo.test".into()],
            rotulo_senha: None,
            rotulo_cpf: None,
            mfa: crate::manifesto::Mfa::Nenhum,
            movimenta_dinheiro: false,
            classe: crate::manifesto::Classe::Consulta,
            agenda_horas: None,
            agenda,
            parametros: Vec::new(),
        }
    }

    #[test]
    fn devidas_volta_na_ordem_da_lista_com_o_instante_de_cada_uma() {
        let m = manifesto_de(vec![
            entrada(&["20:00"], None),
            entrada(&["09:00"], Some(&[1])),
            entrada(&["10:00"], None),
        ]);
        let agora = l(2026, 11, 25, 21, 0);
        let base_velha = l(2026, 11, 1, 10, 0);
        let d = devidas(&m, agora, &[base_velha, base_velha, agora]);
        assert_eq!(
            d,
            vec![
                Devida {
                    entrada: 0,
                    previsto_unix: l(2026, 11, 25, 20, 0)
                },
                // a de dia 1: o 01/11 às 09:00 ficou antes da base (01/11 10:00); a de todo dia
                // às 10:00 já rodou às 21:00 (base == agora)
            ]
        );
        // base faltando: não dispara
        assert!(devidas(&m, agora, &[]).is_empty());
        // as três velhas
        let d = devidas(&m, agora, &[base_velha - 86_400 * 2; 3]);
        assert_eq!(
            d.iter().map(|x| x.entrada).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(d[1].previsto_unix, l(2026, 11, 1, 9, 0));
    }
}
