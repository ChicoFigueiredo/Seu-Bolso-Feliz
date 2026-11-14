//! A agenda dos alertas: horários configuráveis (de São Paulo), silêncio noturno e o laço que
//! roda até o sinal de parada.

use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::Pool;
use sbf_livro::Data;
use tokio::sync::watch;
use uuid::Uuid;

use crate::alertas::{FonteDeAlertas, avaliar};
use crate::config::{ConfigAlertas, Minuto};
use crate::envio::{avisar, resumir};
use crate::erro::ErroDeCanal;
use crate::repo;
use crate::telegram::ClienteTelegram;

/// São Paulo não tem horário de verão desde 2019: UTC−3 fixo (o mesmo de `sbf-livro`).
const DESLOCAMENTO_SAO_PAULO: i64 = -3 * 3600;

/// O dia e o minuto do dia de São Paulo num instante Unix.
pub fn relogio_de_sao_paulo(segundos_unix: i64) -> (Data, Minuto) {
    let local = segundos_unix + DESLOCAMENTO_SAO_PAULO;
    let minuto = u32::try_from(local.rem_euclid(86_400) / 60).unwrap_or(0);
    (Data::de_instante_em_sao_paulo(segundos_unix), minuto)
}

pub fn agora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// Se o minuto cai na janela de silêncio (que pode passar da meia-noite).
pub fn em_silencio(silencio: (Minuto, Minuto), minuto: Minuto) -> bool {
    let (de, ate) = silencio;
    match de.cmp(&ate) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => (de..ate).contains(&minuto),
        std::cmp::Ordering::Greater => minuto >= de || minuto < ate,
    }
}

/// Os horários que já passaram hoje, se agora é fora do silêncio. Um horário que caiu no
/// silêncio sai no primeiro minuto depois dele.
pub fn horarios_devidos(config: &ConfigAlertas, minuto: Minuto) -> Vec<Minuto> {
    if em_silencio(config.silencio, minuto) {
        return Vec::new();
    }
    config
        .horas
        .iter()
        .copied()
        .filter(|h| *h <= minuto)
        .collect()
}

/// Tudo o que a rodada de alertas precisa.
#[derive(Clone)]
pub struct ContextoDeAlertas {
    pub banco: Pool,
    pub telegram: ClienteTelegram,
    pub dono: Uuid,
    pub chats_configurados: Vec<i64>,
    pub config: ConfigAlertas,
    pub extras: Arc<Vec<Box<dyn FonteDeAlertas>>>,
}

impl std::fmt::Debug for ContextoDeAlertas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextoDeAlertas")
            .field("dono", &self.dono)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Uma rodada: avalia as regras e avisa cada destinatário do que mudou. Com `com_resumo`,
/// manda antes o resumo do dia. Devolve quantos alertas foram avisados (somando destinatários).
pub async fn rodar_uma_vez(
    ctx: &ContextoDeAlertas,
    hoje: Data,
    com_resumo: bool,
) -> Result<usize, ErroDeCanal> {
    let livro = repo::carregar_livro(&ctx.banco, ctx.dono).await?;
    let alertas = avaliar(&livro, hoje, &ctx.config.limites, &ctx.extras);
    let destinatarios =
        repo::destinatarios(&ctx.banco, ctx.dono, ctx.chats_configurados.iter().copied()).await?;
    let mut total = 0;
    for d in &destinatarios {
        if com_resumo && let Err(e) = resumir(&ctx.banco, &ctx.telegram, d, hoje, &alertas).await {
            eprintln!("sbf canais: resumo diário não saiu: {e}");
        }
        match avisar(&ctx.banco, &ctx.telegram, d, &alertas).await {
            Ok(n) => total += n,
            Err(e) => eprintln!("sbf canais: avisos não saíram: {e}"),
        }
    }
    Ok(total)
}

fn chave_do_horario(h: Minuto) -> String {
    format!("alertas.rodou.{:02}{:02}", h / 60, h % 60)
}

/// Um passo da agenda: se há horário devido que ainda não rodou hoje, roda. O resumo (se
/// ligado) sai no primeiro horário do dia. Devolve se rodou.
pub async fn passo(ctx: &ContextoDeAlertas, segundos_unix: i64) -> Result<bool, ErroDeCanal> {
    let (hoje, minuto) = relogio_de_sao_paulo(segundos_unix);
    let devidos = horarios_devidos(&ctx.config, minuto);
    let hoje_texto = hoje.to_string();
    let mut pendentes = Vec::new();
    for h in &devidos {
        let ja = repo::ler_estado_do_servico(&ctx.banco, &chave_do_horario(*h)).await?;
        if ja.as_deref() != Some(hoje_texto.as_str()) {
            pendentes.push(*h);
        }
    }
    if pendentes.is_empty() {
        return Ok(false);
    }
    let primeiro_do_dia = ctx
        .config
        .horas
        .first()
        .is_some_and(|p| pendentes.contains(p));
    rodar_uma_vez(ctx, hoje, ctx.config.resumo_diario && primeiro_do_dia).await?;
    for h in pendentes {
        repo::gravar_estado_do_servico(&ctx.banco, &chave_do_horario(h), &hoje_texto).await?;
    }
    Ok(true)
}

/// O laço: confere a agenda a cada 30 s, até o sinal de parada.
pub async fn laco(ctx: ContextoDeAlertas, mut parar: watch::Receiver<bool>) {
    loop {
        if let Err(e) = passo(&ctx, agora_unix()).await {
            eprintln!("sbf canais: agenda de alertas falhou: {e}");
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(30)) => {}
            _ = parar.changed() => {}
        }
        if *parar.borrow() {
            return;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn config(horas: &[Minuto]) -> ConfigAlertas {
        ConfigAlertas {
            horas: horas.to_vec(),
            ..ConfigAlertas::default()
        }
    }

    #[test]
    fn silencio_passa_da_meia_noite() {
        let s = (22 * 60, 7 * 60);
        assert!(em_silencio(s, 23 * 60));
        assert!(em_silencio(s, 3 * 60));
        assert!(!em_silencio(s, 7 * 60));
        assert!(!em_silencio(s, 12 * 60));
        assert!(!em_silencio((0, 0), 100));
        assert!(em_silencio((13 * 60, 14 * 60), 13 * 60 + 30));
    }

    #[test]
    fn horario_no_silencio_sai_quando_o_silencio_acaba() {
        let c = config(&[6 * 60, 18 * 60]);
        assert!(horarios_devidos(&c, 5 * 60).is_empty()); // silêncio
        assert_eq!(horarios_devidos(&c, 7 * 60), vec![6 * 60]); // 06:00 cai no silêncio, sai às 07:00
        assert_eq!(horarios_devidos(&c, 18 * 60 + 1), vec![6 * 60, 18 * 60]);
        assert!(horarios_devidos(&c, 23 * 60).is_empty());
    }

    #[test]
    fn relogio_de_sao_paulo_vira_o_dia_as_tres_da_manha_utc() {
        // 2026-11-14 02:59 UTC = 13/11 23:59 em São Paulo.
        let utc = Data::nova(2026, 11, 14).expect("data").dias_desde_1970() * 86_400;
        let (d, m) = relogio_de_sao_paulo(utc + 2 * 3600 + 59 * 60);
        assert_eq!(d, Data::nova(2026, 11, 13).expect("data"));
        assert_eq!(m, 23 * 60 + 59);
        let (d, m) = relogio_de_sao_paulo(utc + 3 * 3600);
        assert_eq!(d, Data::nova(2026, 11, 14).expect("data"));
        assert_eq!(m, 0);
    }
}
