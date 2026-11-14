//! Custo de IA (spec 10): o consumo da semana em `saida_ia` acima do ritmo, e o provedor
//! reserva em uso. Lê `saida_ia` do inventário (nunca o texto: ele não é guardado).

use rusqlite::params;

use crate::analise::{Analise, Contexto, Limites};
use crate::erro::Resultado;
use crate::tipos::{Area, Candidato, Evidencia, Impacto, Unidade};

pub const CONSUMO_ACIMA_DO_RITMO: &str = "ia-consumo-acima-do-ritmo";
pub const PROVEDOR_RESERVA: &str = "ia-provedor-reserva";

pub(crate) fn rodar(ctx: &Contexto<'_>, lim: &Limites, a: &mut Analise) -> Resultado<()> {
    let semana = ctx.hoje.somar_dias(-7).to_string();
    let antes = ctx.hoje.somar_dias(-35).to_string();
    let hoje = ctx.hoje.to_string();

    // Tokens e custo real (não nominal) da última semana e das quatro anteriores.
    let somar = |de: &str, ate: Option<&str>| -> rusqlite::Result<(i64, i64, i64)> {
        ctx.inventario.query_row(
            "SELECT COALESCE(SUM(tokens_entrada + tokens_saida), 0),
                    COALESCE(SUM(CASE WHEN nominal = 0 THEN custo_dezmilesimos_usd ELSE 0 END), 0),
                    COUNT(*)
               FROM saida_ia WHERE quando >= ?1 AND (?2 IS NULL OR quando < ?2)",
            params![de, ate],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
    };
    a.avaliadas.push(CONSUMO_ACIMA_DO_RITMO);
    let (tokens_semana, custo_semana, chamadas) = somar(&semana, None)?;
    let (tokens_antes, _, _) = somar(&antes, Some(&semana))?;
    let ritmo = (tokens_antes / 4).max(lim.ia_tokens_minimos / 5);
    if tokens_semana >= lim.ia_tokens_minimos && tokens_semana > 2 * ritmo {
        // Dez milésimos de dólar para centavos de dólar.
        let centavos_usd = custo_semana / 100;
        a.candidatos.push(Candidato {
            regra: CONSUMO_ACIMA_DO_RITMO,
            chave: "semana".into(),
            area: Area::CustoDeIa,
            titulo: "O consumo de IA da semana está acima do ritmo".into(),
            detalhe: format!(
                "{tokens_semana} tokens em {chamadas} chamadas nos últimos 7 dias, contra uma média de \
                 {} por semana nas quatro anteriores.",
                tokens_antes / 4
            ),
            evidencia: vec![Evidencia::nova("saida_ia", semana.clone(), "Saídas dos últimos 7 dias")],
            impacto: (centavos_usd > 0).then_some(Impacto {
                valor: centavos_usd,
                unidade: Unidade::CentavosUsd,
                periodo: "semana".into(),
                estimativa: false,
            }),
            // A semana que passa muda a janela: o ponto reabre se o consumo seguir alto.
            assinatura: format!("{hoje}|{tokens_semana}"),
        });
    }

    if let Some(reserva) = ctx.provedor_reserva {
        a.avaliadas.push(PROVEDOR_RESERVA);
        let (n, custo): (i64, i64) = ctx.inventario.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN nominal = 0 THEN custo_dezmilesimos_usd ELSE 0 END), 0)
               FROM saida_ia WHERE provedor = ?1 AND quando >= ?2",
            params![reserva, semana],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )?;
        if n > 0 {
            a.candidatos.push(Candidato {
                regra: PROVEDOR_RESERVA,
                chave: reserva.to_owned(),
                area: Area::CustoDeIa,
                titulo: format!("O provedor reserva ({reserva}) está em uso"),
                detalhe: format!(
                    "{n} chamadas nos últimos 7 dias foram para a reserva: o principal pode estar fora \
                     do ar ou no limite."
                ),
                evidencia: vec![Evidencia::nova("saida_ia", reserva, format!("Chamadas para {reserva}"))],
                impacto: (custo / 100 > 0).then_some(Impacto {
                    valor: custo / 100,
                    unidade: Unidade::CentavosUsd,
                    periodo: "semana".into(),
                    estimativa: false,
                }),
                assinatura: format!("{n}"),
            });
        }
    }
    Ok(())
}
