//! Checkpoints retomáveis em `ingestion_checkpoints` (porte de `checkpoint.ts` do
//! pluggy-sync, e reaproveitado pelo Gmail).
//!
//! Um checkpoint `running`/`paused` existente sempre vence sobre a janela que o chamador
//! passou: retomar uma janela em andamento tem prioridade sobre começar outra. É isso que
//! torna o backfill de 365 dias seguro de reexecutar.
//!
//! Diferença do Bun: os contadores somam no banco (`SET col = col + $n`) em vez de ler e
//! reescrever, o que fecha a janela de corrida que o `checkpoint.ts` registrava como
//! pendência (VPS e máquina local rodando o mesmo backfill).

use chrono::NaiveDate;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::erro::ErroDeColeta;

/// De onde uma varredura retoma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Janela {
    pub id: Uuid,
    pub de: NaiveDate,
    pub ate: NaiveDate,
    /// O cursor de onde retomar, ou `None` para começar do início da janela.
    pub cursor_inicial: Option<String>,
}

/// O que uma página (ou uma varredura) processou.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progresso {
    /// O cursor da PRÓXIMA página, ou `None` quando acabou.
    pub proximo_cursor: Option<String>,
    pub vistos: i32,
    pub criados: i32,
    pub duplicados: i32,
    pub erros: i32,
}

/// Retoma o checkpoint do escopo, ou cria (ou reinicia, se `completed`) com a janela dada.
/// `origem` é `pluggy` ou `gmail`; `tipo_de_cursor` é `page_number`, `internal_date`, ….
pub async fn iniciar_ou_retomar<C: GenericClient>(
    cliente: &C,
    dono: Uuid,
    origem: &str,
    tipo_de_cursor: &str,
    escopo: &str,
    janela_padrao: (NaiveDate, NaiveDate),
) -> Result<Janela, ErroDeColeta> {
    let existente = cliente
        .query_opt(
            "SELECT id, cursor_value, window_start, window_end, status \
             FROM public.ingestion_checkpoints \
             WHERE user_id = $1 AND source_type = $2 AND scope_key = $3 LIMIT 1",
            &[&dono, &origem, &escopo],
        )
        .await?;
    if let Some(l) = &existente {
        let status: String = l.try_get("status")?;
        if status != "completed" {
            let de: Option<NaiveDate> = l.try_get("window_start")?;
            let ate: Option<NaiveDate> = l.try_get("window_end")?;
            return Ok(Janela {
                id: l.try_get("id")?,
                de: de.unwrap_or(janela_padrao.0),
                ate: ate.unwrap_or(janela_padrao.1),
                cursor_inicial: l.try_get("cursor_value")?,
            });
        }
    }
    let criado = cliente
        .query_one(
            "INSERT INTO public.ingestion_checkpoints \
             (user_id, source_type, scope_key, cursor_kind, cursor_value, window_start, window_end, \
              status, messages_seen, documents_created, duplicates_skipped, errors) \
             VALUES ($1, $2, $3, $4, NULL, $5, $6, 'running', 0, 0, 0, 0) \
             ON CONFLICT (user_id, source_type, scope_key) DO UPDATE SET \
               cursor_kind = EXCLUDED.cursor_kind, cursor_value = NULL, \
               window_start = EXCLUDED.window_start, window_end = EXCLUDED.window_end, \
               status = 'running', messages_seen = 0, documents_created = 0, \
               duplicates_skipped = 0, errors = 0 \
             RETURNING id",
            &[
                &dono,
                &origem,
                &escopo,
                &tipo_de_cursor,
                &janela_padrao.0,
                &janela_padrao.1,
            ],
        )
        .await?;
    Ok(Janela {
        id: criado.try_get("id")?,
        de: janela_padrao.0,
        ate: janela_padrao.1,
        cursor_inicial: None,
    })
}

/// Chamar depois de processar cada página: grava onde retomar e soma os contadores.
pub async fn registrar_pagina<C: GenericClient>(
    cliente: &C,
    id: Uuid,
    progresso: &Progresso,
) -> Result<(), ErroDeColeta> {
    cliente
        .execute(
            "UPDATE public.ingestion_checkpoints SET cursor_value = $2, \
             messages_seen = messages_seen + $3, documents_created = documents_created + $4, \
             duplicates_skipped = duplicates_skipped + $5, errors = errors + $6 \
             WHERE id = $1",
            &[
                &id,
                &progresso.proximo_cursor,
                &progresso.vistos,
                &progresso.criados,
                &progresso.duplicados,
                &progresso.erros,
            ],
        )
        .await?;
    Ok(())
}

/// Marca o checkpoint como concluído.
pub async fn concluir<C: GenericClient>(cliente: &C, id: Uuid) -> Result<(), ErroDeColeta> {
    cliente
        .execute(
            "UPDATE public.ingestion_checkpoints SET status = 'completed' WHERE id = $1",
            &[&id],
        )
        .await?;
    Ok(())
}
