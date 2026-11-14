//! Operação (spec 10): documentos parados em "A triar" ou "Em dúvida" há mais de 30 dias,
//! coleta que falha repetidamente e disco sem cifra. Lê o inventário.

use rusqlite::params;

use crate::analise::{Analise, Contexto, Limites};
use crate::erro::Resultado;
use crate::tipos::{Area, Candidato, Evidencia, Impacto, Unidade};

pub const DOCUMENTOS_PARADOS: &str = "documentos-parados";
pub const COLETA_FALHANDO: &str = "coleta-falhando";
pub const DISCO_SEM_CIFRA: &str = "disco-sem-cifra";

pub(crate) fn rodar(ctx: &Contexto<'_>, lim: &Limites, a: &mut Analise) -> Resultado<()> {
    parados(ctx, lim, a)?;
    coletas(ctx, lim, a)?;
    if let Some(cifrado) = ctx.disco_cifrado {
        a.avaliadas.push(DISCO_SEM_CIFRA);
        if !cifrado {
            a.candidatos.push(Candidato {
                regra: DISCO_SEM_CIFRA,
                chave: "disco".into(),
                area: Area::Operacao,
                titulo: "O disco do cofre não está cifrado".into(),
                detalhe: "Quem pegar o computador lê seus documentos. Ative BitLocker, FileVault ou LUKS."
                    .into(),
                evidencia: vec![Evidencia::nova("disco", "cofre", "Volume do cofre")],
                impacto: None,
                assinatura: "sem-cifra".into(),
            });
        }
    }
    Ok(())
}

fn parados(ctx: &Contexto<'_>, lim: &Limites, a: &mut Analise) -> Resultado<()> {
    a.avaliadas.push(DOCUMENTOS_PARADOS);
    let corte = ctx.hoje.somar_dias(-lim.dias_parado).to_string();
    for (estado, chave, rotulo) in [
        ("novo", "a-triar", "A triar"),
        ("duvida", "em-duvida", "Em dúvida"),
    ] {
        let mut c = ctx.inventario.prepare(
            "SELECT a.content_hash,
                    COALESCE((SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                               ORDER BY o.id LIMIT 1), a.content_hash),
                    substr(a.primeiro_visto_em, 1, 10)
               FROM achado a WHERE a.estado = ?1 AND a.primeiro_visto_em < ?2
              ORDER BY a.primeiro_visto_em, a.content_hash",
        )?;
        let linhas: Vec<(String, String, String)> = c
            .query_map(params![estado, corte], |l| {
                Ok((l.get(0)?, l.get(1)?, l.get(2)?))
            })?
            .collect::<Result<_, _>>()?;
        if linhas.is_empty() {
            continue;
        }
        let total = linhas.len();
        let minutos = i64::try_from(total).unwrap_or(0) / 2 + 1;
        a.candidatos.push(Candidato {
            regra: DOCUMENTOS_PARADOS,
            chave: chave.into(),
            area: Area::Operacao,
            titulo: format!(
                "{total} documentos parados em {rotulo} há mais de {} dias",
                lim.dias_parado
            ),
            detalhe: format!("O mais antigo chegou em {}.", linhas[0].2),
            evidencia: linhas
                .iter()
                .take(20)
                .map(|(h, nome, _)| Evidencia::nova("achado", h.clone(), nome.clone()))
                .collect(),
            impacto: Some(Impacto {
                valor: minutos,
                unidade: Unidade::Minutos,
                periodo: "unico".into(),
                estimativa: true,
            }),
            assinatura: format!("{total}|{}", linhas[0].0),
        });
    }
    Ok(())
}

fn coletas(ctx: &Contexto<'_>, lim: &Limites, a: &mut Analise) -> Resultado<()> {
    a.avaliadas.push(COLETA_FALHANDO);
    let mut fontes = ctx
        .inventario
        .prepare("SELECT DISTINCT fonte_id FROM varredura ORDER BY fonte_id")?;
    let fontes: Vec<String> = fontes
        .query_map([], |l| l.get(0))?
        .collect::<Result<_, _>>()?;
    let n = i64::try_from(lim.falhas_seguidas).unwrap_or(3);
    for fonte in fontes {
        let mut c = ctx.inventario.prepare(
            "SELECT id, status FROM varredura WHERE fonte_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let ultimas: Vec<(i64, String)> = c
            .query_map(params![fonte, n], |l| Ok((l.get(0)?, l.get(1)?)))?
            .collect::<Result<_, _>>()?;
        if ultimas.len() as i64 == n && ultimas.iter().all(|(_, s)| s == "falhou") {
            a.candidatos.push(Candidato {
                regra: COLETA_FALHANDO,
                chave: fonte.clone(),
                area: Area::Operacao,
                titulo: format!("A coleta de {fonte} falhou {n} vezes seguidas"),
                detalhe: "Confira a conexão, a senha ou o caminho da fonte em Coletas.".into(),
                evidencia: ultimas
                    .iter()
                    .map(|(id, _)| {
                        Evidencia::nova(
                            "coleta",
                            id.to_string(),
                            format!("{fonte}, varredura {id}"),
                        )
                    })
                    .collect(),
                impacto: None,
                assinatura: ultimas
                    .first()
                    .map_or(String::new(), |(id, _)| id.to_string()),
            });
        }
    }
    Ok(())
}
