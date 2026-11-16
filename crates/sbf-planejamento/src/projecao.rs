//! O motor de projeção: puro, sem banco (spec 11, "Motor de projeção").
//!
//! Dado um [`Recorrente`], as exceções e as conciliações dele, calcula as [`Ocorrencia`]s
//! de qualquer competência. Nada é gravado; reprojetar dá sempre os mesmos ids e totais.

use std::collections::HashMap;

use crate::{
    Certeza, Competencia, Conciliacao, Data, EstadoDaOcorrencia, Excecao, ExcecaoGravada,
    Frequencia, IdOcorrencia, Natureza, Ocorrencia, Prazo, Recorrente, Versao,
};

/// A versão vigente numa competência: a de maior `desde` que ainda não passou dela.
pub fn versao_vigente(recorrente: &Recorrente, competencia: Competencia) -> Option<&Versao> {
    recorrente
        .versoes
        .iter()
        .filter(|v| v.desde <= competencia)
        .max_by_key(|v| v.desde)
}

/// A data da regra naquela competência, se a frequência gera ocorrência nela.
fn data_da_regra(frequencia: &Frequencia, c: Competencia) -> Option<Data> {
    match *frequencia {
        Frequencia::Mensal { dia } => Some(c.dia(dia)),
        Frequencia::CadaNMeses { n, dia, primeira } => {
            let passo = i32::from(n);
            let diferenca = primeira.meses_ate(c);
            (passo > 0 && diferenca >= 0 && diferenca % passo == 0).then(|| c.dia(dia))
        }
        Frequencia::Anual { mes, dia } => (c.mes == mes).then(|| c.dia(dia)),
        Frequencia::Unica { data } => (Competencia::de_data(data) == c).then_some(data),
    }
}

/// De quantos em quantos meses a frequência gera, para numerar as parcelas.
fn passo_em_meses(frequencia: &Frequencia) -> i32 {
    match *frequencia {
        Frequencia::CadaNMeses { n, .. } => i32::from(n.max(1)),
        Frequencia::Anual { .. } => 12,
        Frequencia::Mensal { .. } | Frequencia::Unica { .. } => 1,
    }
}

/// As exceções e conciliações de um recorrente, indexadas por ocorrência.
struct Contexto<'a> {
    /// O início do acompanhamento: antes dele, sem pago informado, nada está em atraso.
    inicio: Option<Competencia>,
    excecoes: HashMap<&'a IdOcorrencia, Vec<&'a ExcecaoGravada>>,
    conciliacoes: HashMap<&'a IdOcorrencia, Vec<&'a Conciliacao>>,
}

impl<'a> Contexto<'a> {
    fn novo(
        recorrente: &Recorrente,
        excecoes: &'a [ExcecaoGravada],
        conciliacoes: &'a [Conciliacao],
        inicio: Option<Competencia>,
    ) -> Contexto<'a> {
        let mut por_excecao: HashMap<&IdOcorrencia, Vec<&ExcecaoGravada>> = HashMap::new();
        for e in excecoes
            .iter()
            .filter(|e| !e.desfeita && e.ocorrencia.recorrente == recorrente.id)
        {
            por_excecao.entry(&e.ocorrencia).or_default().push(e);
        }
        let mut por_conciliacao: HashMap<&IdOcorrencia, Vec<&Conciliacao>> = HashMap::new();
        for c in conciliacoes
            .iter()
            .filter(|c| c.ocorrencia.recorrente == recorrente.id)
        {
            por_conciliacao.entry(&c.ocorrencia).or_default().push(c);
        }
        Contexto {
            inicio,
            excecoes: por_excecao,
            conciliacoes: por_conciliacao,
        }
    }
}

/// O que as exceções vigentes (a última de cada tipo vale) dizem de uma ocorrência.
#[derive(Default)]
struct Decisoes {
    adiar: Option<Option<Data>>,
    dispensada: bool,
    valor_so_deste_mes: Option<crate::Centavos>,
    pago_informado: Option<crate::Centavos>,
}

fn decisoes(excecoes: &[&ExcecaoGravada]) -> Decisoes {
    let mut d = Decisoes::default();
    for e in excecoes {
        match &e.excecao {
            Excecao::Adiar { nova_data } => d.adiar = Some(*nova_data),
            Excecao::Dispensar { .. } => d.dispensada = true,
            Excecao::ValorSoDesteMes { valor } => d.valor_so_deste_mes = Some(*valor),
            Excecao::PagoInformado { valor, .. } => d.pago_informado = Some(*valor),
        }
    }
    d
}

/// Projeta as ocorrências de `de` a `ate` (inclusive), com as suprimidas marcadas pelos
/// estados `SuprimidaPor*`. Quem monta o controle filtra o que não deve aparecer.
/// Sem início do acompanhamento: veja [`projetar_desde`].
pub fn projetar(
    recorrente: &Recorrente,
    excecoes: &[ExcecaoGravada],
    conciliacoes: &[Conciliacao],
    de: Competencia,
    ate: Competencia,
    hoje: Data,
) -> Vec<Ocorrencia> {
    projetar_desde(recorrente, excecoes, conciliacoes, de, ate, hoje, None)
}

/// Como [`projetar`], com o início do acompanhamento: uma ocorrência de competência
/// anterior a ele, sem pagamento informado, nunca fica `EmAtraso` (pago vazio é "não
/// informado", não "não pago"), salvo se tiver `Excecao::Adiar`, que diz que ainda é devida.
pub fn projetar_desde(
    recorrente: &Recorrente,
    excecoes: &[ExcecaoGravada],
    conciliacoes: &[Conciliacao],
    de: Competencia,
    ate: Competencia,
    hoje: Data,
    inicio: Option<Competencia>,
) -> Vec<Ocorrencia> {
    let total = de.meses_ate(ate);
    projetar_competencias_desde(
        recorrente,
        excecoes,
        conciliacoes,
        (0..=total).map(|i| de.somar_meses(i)),
        hoje,
        inicio,
    )
}

/// Projeta só as competências dadas (a ordem da saída é a da entrada).
pub fn projetar_competencias(
    recorrente: &Recorrente,
    excecoes: &[ExcecaoGravada],
    conciliacoes: &[Conciliacao],
    competencias: impl IntoIterator<Item = Competencia>,
    hoje: Data,
) -> Vec<Ocorrencia> {
    projetar_competencias_desde(recorrente, excecoes, conciliacoes, competencias, hoje, None)
}

/// [`projetar_competencias`] com o início do acompanhamento.
pub fn projetar_competencias_desde(
    recorrente: &Recorrente,
    excecoes: &[ExcecaoGravada],
    conciliacoes: &[Conciliacao],
    competencias: impl IntoIterator<Item = Competencia>,
    hoje: Data,
    inicio: Option<Competencia>,
) -> Vec<Ocorrencia> {
    let contexto = Contexto::novo(recorrente, excecoes, conciliacoes, inicio);
    competencias
        .into_iter()
        .filter_map(|c| ocorrencia_em(recorrente, &contexto, c, hoje))
        .collect()
}

/// A numeração da parcela e o `total`, se o prazo é de parcelas e a competência cabe nele.
/// `Err(())` quando o prazo não deixa gerar nesta competência.
fn numerar(
    prazo: &Prazo,
    frequencia: &Frequencia,
    c: Competencia,
) -> Result<Option<(u32, u32)>, ()> {
    match *prazo {
        Prazo::SemFim | Prazo::Desconhecido => Ok(None),
        Prazo::AteCompetencia(limite) => {
            if c < limite {
                Ok(None)
            } else {
                Err(())
            }
        }
        Prazo::Parcelas { total, primeira } => {
            let diferenca = primeira.meses_ate(c);
            if diferenca < 0 {
                return Err(());
            }
            let numero = u32::try_from(diferenca / passo_em_meses(frequencia)).map_err(|_| ())? + 1;
            if numero > total {
                return Err(());
            }
            Ok(Some((numero, total)))
        }
    }
}

fn ocorrencia_em(
    rec: &Recorrente,
    contexto: &Contexto<'_>,
    c: Competencia,
    hoje: Data,
) -> Option<Ocorrencia> {
    let versao = versao_vigente(rec, c)?;
    let vencimento_original = data_da_regra(&versao.frequencia, c)?;
    let parcela = numerar(&rec.prazo, &versao.frequencia, c).ok()?;
    let id = IdOcorrencia {
        recorrente: rec.id.clone(),
        competencia: c,
        parcela: parcela.map(|(n, _)| n),
    };

    let excecoes = contexto.excecoes.get(&id).map_or(&[][..], Vec::as_slice);
    let decisoes = decisoes(excecoes);
    let conciliacoes = contexto
        .conciliacoes
        .get(&id)
        .map_or(&[][..], Vec::as_slice);

    let sugestao_de_conciliacao = conciliacoes.iter().any(|x| !x.confirmada);
    let confirmadas: Vec<&&Conciliacao> = conciliacoes.iter().filter(|x| x.confirmada).collect();
    let conciliado = (!confirmadas.is_empty()).then(|| confirmadas.iter().map(|x| x.valor).sum());

    let previsto = decisoes.valor_so_deste_mes.or(versao.valor);
    let vencimento = match decisoes.adiar {
        Some(Some(nova)) => nova,
        _ => vencimento_original,
    };

    let motivo_da_supressao = supressao(rec, c);
    let tem_pago = decisoes.pago_informado.is_some() || conciliado.is_some();
    let estado = if let (Some(s), false) = (motivo_da_supressao, tem_pago) {
        s
    } else {
        let antes_do_inicio = contexto.inicio.is_some_and(|i| c < i);
        // Antes do início, só o adiar diz que ainda é devida.
        let pode_atrasar = !antes_do_inicio || decisoes.adiar.is_some();
        estado_da(
            &decisoes,
            previsto,
            conciliado,
            vencimento,
            hoje,
            pode_atrasar,
        )
    };

    Some(Ocorrencia {
        id,
        nome: rec.nome.clone(),
        grupo: rec.grupo.clone(),
        subgrupo: rec.subgrupo.clone(),
        posicao: rec.posicao,
        natureza: rec.natureza,
        vencimento,
        vencimento_original,
        previsto,
        certeza: versao.certeza,
        fonte: versao.fonte,
        pago_informado: decisoes.pago_informado,
        conciliado,
        estado,
        parcela,
        conta: versao.conta.clone(),
        centro: versao.centro.clone(),
        fornecedor: versao.fornecedor.clone(),
        versao: versao.id.clone(),
        prazo_desconhecido: matches!(rec.prazo, Prazo::Desconhecido),
        sugestao_de_conciliacao,
    })
}

/// Pausa e cancelamento suprimem; o cancelamento tem a palavra final.
fn supressao(rec: &Recorrente, c: Competencia) -> Option<EstadoDaOcorrencia> {
    if rec.cancelamento.as_ref().is_some_and(|x| c >= x.desde) {
        return Some(EstadoDaOcorrencia::SuprimidaPorCancelamento);
    }
    rec.pausas
        .iter()
        .any(|p| p.desde <= c && p.ate.is_none_or(|ate| c < ate))
        .then_some(EstadoDaOcorrencia::SuprimidaPorPausa)
}

fn estado_da(
    d: &Decisoes,
    previsto: Option<crate::Centavos>,
    conciliado: Option<crate::Centavos>,
    vencimento: Data,
    hoje: Data,
    pode_atrasar: bool,
) -> EstadoDaOcorrencia {
    let menor_que_o_previsto = |pago: crate::Centavos| previsto.is_some_and(|p| pago < p);
    if let Some(c) = conciliado {
        return if menor_que_o_previsto(c) {
            EstadoDaOcorrencia::Parcial
        } else {
            EstadoDaOcorrencia::Conciliada
        };
    }
    if let Some(p) = d.pago_informado {
        return if menor_que_o_previsto(p) {
            EstadoDaOcorrencia::Parcial
        } else {
            EstadoDaOcorrencia::PagoInformado
        };
    }
    if d.dispensada {
        return EstadoDaOcorrencia::Dispensada;
    }
    if pode_atrasar && vencimento < hoje {
        return EstadoDaOcorrencia::EmAtraso;
    }
    if d.adiar.is_some() {
        return EstadoDaOcorrencia::Adiada;
    }
    EstadoDaOcorrencia::Prevista
}

/// Atalhos de leitura que o serviço e os testes usam.
impl Ocorrencia {
    /// O pago que conta: a conciliação, que é o fato, antes do pago informado.
    pub fn pago(&self) -> Option<crate::Centavos> {
        self.conciliado.or(self.pago_informado)
    }

    pub fn suprimida(&self) -> bool {
        matches!(
            self.estado,
            EstadoDaOcorrencia::SuprimidaPorPausa | EstadoDaOcorrencia::SuprimidaPorCancelamento
        )
    }

    /// Quanto ainda falta pagar (0 quando paga, dispensada ou suprimida; o previsto inteiro
    /// quando nada foi pago; `None` sem valor previsto e sem pago).
    pub fn falta(&self) -> Option<crate::Centavos> {
        use crate::Centavos;
        match self.estado {
            EstadoDaOcorrencia::Conciliada
            | EstadoDaOcorrencia::PagoInformado
            | EstadoDaOcorrencia::Dispensada
            | EstadoDaOcorrencia::SuprimidaPorPausa
            | EstadoDaOcorrencia::SuprimidaPorCancelamento => Some(Centavos(0)),
            EstadoDaOcorrencia::Parcial => match (self.previsto, self.pago()) {
                (Some(p), Some(pago)) => Some(p - pago),
                _ => None,
            },
            EstadoDaOcorrencia::Prevista
            | EstadoDaOcorrencia::EmAtraso
            | EstadoDaOcorrencia::Adiada => self.previsto,
        }
    }

    /// Falta pagar e já passou do vencimento (em atraso, ou parcial vencida).
    pub fn devida_vencida(&self, hoje: Data) -> bool {
        match self.estado {
            EstadoDaOcorrencia::EmAtraso => true,
            EstadoDaOcorrencia::Parcial => self.vencimento < hoje,
            _ => false,
        }
    }

    /// Valor estimado ou desconhecido: o número que sai dele é estimativa.
    pub fn e_incerta(&self) -> bool {
        self.previsto.is_none() || self.certeza == Certeza::Estimado
    }

    pub fn e_entrada(&self) -> bool {
        matches!(self.natureza, Natureza::Entrada)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{Autor, Cancelamento, Centavos, Fonte, Pausa};

    const HOJE: Data = Data {
        ano: 2026,
        mes: 11,
        dia: 16,
    };

    fn comp(ano: i32, mes: u8) -> Competencia {
        Competencia { ano, mes }
    }

    fn versao(desde: Competencia, valor: i64, frequencia: Frequencia) -> Versao {
        Versao {
            id: format!("v{desde}"),
            desde,
            valor: Some(Centavos(valor)),
            certeza: Certeza::Confirmado,
            fonte: Fonte::Usuario,
            frequencia,
            conta: None,
            centro: None,
            fornecedor: None,
            categoria: None,
            nota: None,
        }
    }

    fn regra(prazo: Prazo, versoes: Vec<Versao>) -> Recorrente {
        Recorrente {
            id: "rec".into(),
            nome: "Streaming Exemplo".into(),
            grupo: "Assinaturas".into(),
            subgrupo: None,
            posicao: 1,
            natureza: Natureza::Saida,
            prazo,
            versoes,
            pausas: vec![],
            cancelamento: None,
            arquivado: false,
            excluido: None,
            versao_do_registro: 1,
        }
    }

    fn mensal(dia: u8) -> Recorrente {
        regra(
            Prazo::SemFim,
            vec![versao(comp(2026, 1), 3500, Frequencia::Mensal { dia })],
        )
    }

    fn excecao(c: Competencia, e: Excecao) -> ExcecaoGravada {
        ExcecaoGravada {
            id: format!("e{c}"),
            ocorrencia: IdOcorrencia {
                recorrente: "rec".into(),
                competencia: c,
                parcela: None,
            },
            excecao: e,
            quando: "2026-11-01T00:00:00Z".into(),
            autor: Autor::Usuario,
            desfeita: false,
        }
    }

    fn pago(c: Competencia, valor: i64) -> ExcecaoGravada {
        excecao(
            c,
            Excecao::PagoInformado {
                valor: Centavos(valor),
                data: None,
            },
        )
    }

    fn proj(
        r: &Recorrente,
        e: &[ExcecaoGravada],
        de: Competencia,
        ate: Competencia,
    ) -> Vec<Ocorrencia> {
        projetar(r, e, &[], de, ate, HOJE)
    }

    #[test]
    fn versao_vigente_e_a_de_maior_desde_e_o_id_nao_muda() {
        let mut r = mensal(5);
        r.versoes
            .push(versao(comp(2027, 3), 4000, Frequencia::Mensal { dia: 5 }));
        let o = proj(&r, &[], comp(2027, 1), comp(2027, 5));
        let valores: Vec<_> = o.iter().map(|x| x.previsto).collect();
        assert_eq!(
            valores,
            [3500, 3500, 4000, 4000, 4000].map(|v| Some(Centavos(v)))
        );
        assert_eq!(o[1].id.to_string(), "rec:2027-02");
        assert_eq!(o[2].id.to_string(), "rec:2027-03");
        assert_eq!(o[1].versao, "v2026-01");
        assert_eq!(o[2].versao, "v2027-03");
    }

    #[test]
    fn antes_da_primeira_versao_nao_ha_ocorrencia() {
        let o = proj(&mensal(5), &[], comp(2025, 11), comp(2026, 2));
        assert_eq!(o.len(), 2);
    }

    #[test]
    fn dia_31_vira_o_ultimo_dia_valido_inclusive_bissexto() {
        let o = proj(&mensal(31), &[], comp(2027, 1), comp(2028, 2));
        let dia = |a, m| {
            o.iter()
                .find(|x| x.id.competencia == comp(a, m))
                .map(|x| x.vencimento.dia)
        };
        assert_eq!(dia(2027, 2), Some(28));
        assert_eq!(dia(2028, 2), Some(29));
        assert_eq!(dia(2027, 4), Some(30));
        assert_eq!(dia(2027, 1), Some(31));
    }

    #[test]
    fn trimestral_cobre_a_virada_de_ano() {
        let r = regra(
            Prazo::SemFim,
            vec![versao(
                comp(2026, 1),
                10000,
                Frequencia::CadaNMeses {
                    n: 3,
                    dia: 10,
                    primeira: comp(2026, 11),
                },
            )],
        );
        let o = proj(&r, &[], comp(2026, 10), comp(2027, 12));
        let comps: Vec<_> = o.iter().map(|x| x.id.competencia).collect();
        assert_eq!(
            comps,
            [
                comp(2026, 11),
                comp(2027, 2),
                comp(2027, 5),
                comp(2027, 8),
                comp(2027, 11)
            ]
        );
    }

    #[test]
    fn anual_gera_uma_vez_por_ano() {
        let r = regra(
            Prazo::SemFim,
            vec![versao(
                comp(2026, 1),
                50000,
                Frequencia::Anual { mes: 2, dia: 29 },
            )],
        );
        let o = proj(&r, &[], comp(2026, 1), comp(2029, 12));
        let datas: Vec<_> = o.iter().map(|x| x.vencimento.to_string()).collect();
        assert_eq!(
            datas,
            ["2026-02-28", "2027-02-28", "2028-02-29", "2029-02-28"]
        );
    }

    #[test]
    fn unica_aparece_uma_vez_so() {
        let data = Data::nova(2027, 6, 12).expect("data");
        let r = regra(
            Prazo::SemFim,
            vec![versao(comp(2026, 1), 9000, Frequencia::Unica { data })],
        );
        let o = proj(&r, &[], comp(2026, 11), comp(2056, 10));
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].vencimento, data);
    }

    #[test]
    fn parcelas_geram_exatamente_o_total_numeradas() {
        let r = regra(
            Prazo::Parcelas {
                total: 48,
                primeira: comp(2026, 11),
            },
            vec![versao(
                comp(2026, 1),
                120000,
                Frequencia::Mensal { dia: 10 },
            )],
        );
        let o = proj(&r, &[], comp(2026, 1), comp(2056, 12));
        assert_eq!(o.len(), 48);
        assert_eq!(o[0].parcela, Some((1, 48)));
        assert_eq!(o[47].parcela, Some((48, 48)));
        assert_eq!(o[47].id.competencia, comp(2030, 10));
        assert_eq!(o[47].id.to_string(), "rec:2030-10:p48");
    }

    #[test]
    fn ate_competencia_e_exclusivo_e_desconhecido_vai_ao_fim_marcado() {
        let mut r = mensal(5);
        r.prazo = Prazo::AteCompetencia(comp(2027, 3));
        let o = proj(&r, &[], comp(2027, 1), comp(2027, 6));
        assert_eq!(o.len(), 2);
        r.prazo = Prazo::Desconhecido;
        let o = proj(&r, &[], comp(2027, 1), comp(2027, 6));
        assert_eq!(o.len(), 6);
        assert!(o.iter().all(|x| x.prazo_desconhecido));
    }

    #[test]
    fn pausa_suprime_o_intervalo_e_cancelamento_suprime_dali_em_diante() {
        let mut r = mensal(5);
        r.pausas.push(Pausa {
            id: "p".into(),
            desde: comp(2026, 12),
            ate: Some(comp(2027, 2)),
            motivo: "viagem".into(),
        });
        let o = proj(&r, &[], comp(2026, 11), comp(2027, 3));
        let estados: Vec<_> = o.iter().map(|x| x.estado).collect();
        use EstadoDaOcorrencia::*;
        assert_eq!(
            estados,
            [
                EmAtraso,
                SuprimidaPorPausa,
                SuprimidaPorPausa,
                Prevista,
                Prevista
            ]
        );
        r.cancelamento = Some(Cancelamento {
            desde: comp(2027, 2),
            motivo: "acabou".into(),
            comprovado: false,
        });
        let o = proj(&r, &[], comp(2027, 1), comp(2027, 3));
        assert_eq!(
            o.iter().map(|x| x.estado).collect::<Vec<_>>(),
            [
                SuprimidaPorPausa,
                SuprimidaPorCancelamento,
                SuprimidaPorCancelamento
            ]
        );
    }

    #[test]
    fn supressao_nao_esconde_o_que_tem_pago_informado_ou_conciliacao_confirmada() {
        let mut r = mensal(5);
        r.pausas.push(Pausa {
            id: "p".into(),
            desde: comp(2026, 12),
            ate: None,
            motivo: "x".into(),
        });
        let e = [pago(comp(2026, 12), 3500)];
        let c = [Conciliacao {
            ocorrencia: IdOcorrencia {
                recorrente: "rec".into(),
                competencia: comp(2027, 1),
                parcela: None,
            },
            movimento: "pluggy:txn_1".into(),
            valor: Centavos(3500),
            data: Data::nova(2027, 1, 5).expect("data"),
            confirmada: true,
        }];
        let o = projetar(&r, &e, &c, comp(2026, 12), comp(2027, 2), HOJE);
        use EstadoDaOcorrencia::*;
        assert_eq!(
            o.iter().map(|x| x.estado).collect::<Vec<_>>(),
            [PagoInformado, Conciliada, SuprimidaPorPausa]
        );
        // sugestão (não confirmada) não segura a ocorrência suprimida
        let sugestao = [Conciliacao {
            confirmada: false,
            ..c[0].clone()
        }];
        let o = projetar(&r, &[], &sugestao, comp(2027, 1), comp(2027, 1), HOJE);
        assert_eq!(o[0].estado, SuprimidaPorPausa);
        assert!(o[0].sugestao_de_conciliacao);
    }

    #[test]
    fn adiar_com_data_muda_o_vencimento_e_guarda_o_original() {
        let r = mensal(5);
        let nova = Data::nova(2027, 1, 20).expect("data");
        let e = [excecao(
            comp(2027, 1),
            Excecao::Adiar {
                nova_data: Some(nova),
            },
        )];
        let o = proj(&r, &e, comp(2027, 1), comp(2027, 1));
        assert_eq!(o[0].vencimento, nova);
        assert_eq!(o[0].vencimento_original.dia, 5);
        assert_eq!(o[0].estado, EstadoDaOcorrencia::Adiada);
    }

    #[test]
    fn adiar_sem_data_continua_devida_e_vira_atraso_depois_do_original() {
        let r = mensal(5);
        let e = [
            excecao(comp(2026, 10), Excecao::Adiar { nova_data: None }),
            excecao(comp(2026, 12), Excecao::Adiar { nova_data: None }),
        ];
        let o = proj(&r, &e, comp(2026, 10), comp(2026, 12));
        assert_eq!(o[0].estado, EstadoDaOcorrencia::EmAtraso);
        assert_eq!(o[2].estado, EstadoDaOcorrencia::Adiada);
        assert_eq!(o[2].vencimento, o[2].vencimento_original);
    }

    #[test]
    fn dispensar_valor_so_deste_mes_e_excecao_desfeita() {
        let r = mensal(5);
        let mut desfeita = excecao(
            comp(2027, 3),
            Excecao::ValorSoDesteMes { valor: Centavos(1) },
        );
        desfeita.desfeita = true;
        let e = [
            excecao(
                comp(2027, 1),
                Excecao::Dispensar {
                    motivo: "brinde".into(),
                },
            ),
            excecao(
                comp(2027, 2),
                Excecao::ValorSoDesteMes {
                    valor: Centavos(5000),
                },
            ),
            desfeita,
        ];
        let o = proj(&r, &e, comp(2027, 1), comp(2027, 3));
        assert_eq!(o[0].estado, EstadoDaOcorrencia::Dispensada);
        assert_eq!(o[1].previsto, Some(Centavos(5000)));
        assert_eq!(o[2].previsto, Some(Centavos(3500)));
    }

    #[test]
    fn estados_pago_parcial_atraso_e_futuro_nao_herda_pago() {
        let r = mensal(5);
        let e = [pago(comp(2026, 10), 3500), pago(comp(2026, 9), 1000)];
        let o = proj(&r, &e, comp(2026, 8), comp(2026, 12));
        use EstadoDaOcorrencia::*;
        let estados: Vec<_> = o.iter().map(|x| x.estado).collect();
        assert_eq!(
            estados,
            [EmAtraso, Parcial, PagoInformado, EmAtraso, Prevista]
        );
        // pago em outubro não aparece em novembro nem no futuro
        assert_eq!(o[3].pago_informado, None);
        assert_eq!(o[4].pago_informado, None);
    }

    #[test]
    fn sem_valor_nunca_vira_zero_e_vencida_hoje_ainda_nao_e_atraso() {
        let mut r = mensal(16);
        r.versoes[0].valor = None;
        let o = proj(&r, &[], comp(2026, 11), comp(2026, 11));
        assert_eq!(o[0].previsto, None);
        assert_eq!(o[0].estado, EstadoDaOcorrencia::Prevista);
    }

    #[test]
    fn conciliacao_menor_que_o_previsto_e_parcial_e_sugestao_marca_sem_pagar() {
        let r = mensal(5);
        let id = IdOcorrencia {
            recorrente: "rec".into(),
            competencia: comp(2027, 1),
            parcela: None,
        };
        let c = |confirmada, valor| Conciliacao {
            ocorrencia: id.clone(),
            movimento: format!("m{valor}"),
            valor: Centavos(valor),
            data: Data::nova(2027, 1, 5).expect("data"),
            confirmada,
        };
        let o = projetar(
            &r,
            &[],
            &[c(true, 1000)],
            comp(2027, 1),
            comp(2027, 1),
            HOJE,
        );
        assert_eq!(o[0].estado, EstadoDaOcorrencia::Parcial);
        assert_eq!(o[0].conciliado, Some(Centavos(1000)));
        let o = projetar(
            &r,
            &[],
            &[c(false, 3500)],
            comp(2027, 1),
            comp(2027, 1),
            HOJE,
        );
        assert_eq!(o[0].estado, EstadoDaOcorrencia::Prevista);
        assert!(o[0].sugestao_de_conciliacao);
        assert_eq!(o[0].conciliado, None);
    }

    #[test]
    fn reprojetar_da_os_mesmos_ids_e_totais() {
        let r = mensal(31);
        let a = proj(&r, &[], comp(2026, 11), comp(2056, 10));
        let b = proj(&r, &[], comp(2026, 11), comp(2056, 10));
        assert_eq!(a, b);
        assert_eq!(a.len(), 360);
    }

    #[test]
    fn antes_do_inicio_do_acompanhamento_nada_atrasa_salvo_adiar() {
        let r = mensal(5);
        let e = [excecao(comp(2026, 9), Excecao::Adiar { nova_data: None })];
        let o = projetar_desde(
            &r,
            &e,
            &[],
            comp(2026, 9),
            comp(2026, 12),
            HOJE,
            Some(comp(2026, 11)),
        );
        use EstadoDaOcorrencia::*;
        let estados: Vec<_> = o.iter().map(|x| x.estado).collect();
        // setembro (adiada sem data) segue as regras de sempre; outubro não atrasa.
        assert_eq!(estados, [EmAtraso, Prevista, EmAtraso, Prevista]);
        // Pago informado antes do início continua valendo.
        let e = [pago(comp(2026, 10), 3500)];
        let o = projetar_desde(
            &r,
            &e,
            &[],
            comp(2026, 10),
            comp(2026, 10),
            HOJE,
            Some(comp(2026, 11)),
        );
        assert_eq!(o[0].estado, PagoInformado);
    }
}
