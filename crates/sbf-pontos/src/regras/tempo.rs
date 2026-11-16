//! Tempo do usuário (spec 10): a fila de sugestões da IA crescendo mais que as confirmações.

use crate::analise::{Analise, Contexto, Limites};
use crate::erro::Resultado;
use crate::tipos::{Area, Candidato, Evidencia, Impacto, Unidade};

pub const FILA_CRESCENDO: &str = "fila-de-sugestoes-crescendo";

/// Quanto custa confirmar uma sugestão em lote, em segundos (estimativa).
const SEGUNDOS_POR_SUGESTAO: i64 = 15;

pub(crate) fn rodar(ctx: &Contexto<'_>, lim: &Limites, a: &mut Analise) -> Resultado<()> {
    a.avaliadas.push(FILA_CRESCENDO);
    let semana = ctx.hoje.somar_dias(-7).to_string();
    let inv = ctx.inventario;
    let abertas: i64 = inv.query_row(
        "SELECT COUNT(*) FROM sugestao_ia WHERE estado = 'aberta' AND campo <> 'nao_classificado'",
        [],
        |l| l.get(0),
    )?;
    let criadas: i64 = inv.query_row(
        "SELECT COUNT(*) FROM sugestao_ia WHERE criada_em >= ?1 AND campo <> 'nao_classificado'",
        [&semana],
        |l| l.get(0),
    )?;
    let confirmadas: i64 = inv.query_row(
        "SELECT COUNT(*) FROM decisao WHERE autor = 'usuario' AND motivo = ?1 AND em >= ?2",
        rusqlite::params![MOTIVO_DO_ACEITE, semana],
        |l| l.get(0),
    )?;
    if abertas >= lim.fila_minima && criadas >= confirmadas + lim.fila_folga {
        a.candidatos.push(Candidato {
            regra: FILA_CRESCENDO,
            chave: "fila".into(),
            area: Area::TempoDoUsuario,
            titulo: format!("{abertas} sugestões esperam sua confirmação e a fila está crescendo"),
            detalhe: format!(
                "Chegaram {criadas} sugestões nos últimos 7 dias e você confirmou {confirmadas}."
            ),
            evidencia: vec![Evidencia::nova("area", "triagem-ia", "Triagem por IA")],
            impacto: Some(Impacto {
                valor: (abertas * SEGUNDOS_POR_SUGESTAO + 59) / 60,
                unidade: Unidade::Minutos,
                periodo: "unico".into(),
                estimativa: true,
            }),
            // Cada faixa de 20 sugestões a mais é uma mudança da evidência.
            assinatura: format!("{}", abertas / 20),
        });
    }
    Ok(())
}

/// O motivo que o aceite de sugestão da IA grava em `decisao` (`sbf_triagem::aceite`). É um
/// texto, copiado aqui para a análise não depender da triagem; um teste confere os dois.
pub const MOTIVO_DO_ACEITE: &str = "sugestão da IA";
