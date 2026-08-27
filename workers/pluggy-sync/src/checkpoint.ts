/**
 * Checkpoint de sincronização por conta Pluggy, reusando `ingestion_checkpoints`
 * (P0-9, desenhada pra "varredura retomável" do Gmail/pasta local — mesmo
 * problema, fonte nova). `cursor_kind = "page_number"`: a Pluggy pagina por
 * número de página, durável entre execuções (ao contrário de um page_token).
 *
 * Um checkpoint "running"/"paused" existente sempre vence sobre a janela que
 * o chamador passou — retomar uma janela em andamento tem prioridade sobre
 * começar uma nova. Isso é o que torna o backfill de 365 dias seguro de
 * reexecutar: uma interrupção no meio retoma da última página persistida,
 * não do início.
 *
 * PENDÊNCIA REGISTRADA (Fase 5, não Fase 3): startOrResumeCheckpoint faz
 * SELECT seguido de upsert, sem claim atômico — não é `UPDATE ...
 * WHERE status = 'running' RETURNING id` como o locking otimista de
 * `transitionJob` (workers/ingestion). Hoje só existe um processo rodando
 * este worker por vez (sem VPS ainda), então a janela de corrida é teórica.
 * Só vira relevante quando VPS + máquina local rodarem o mesmo backfill em
 * paralelo de verdade (Fase 5, bloqueada em acesso SSH — 🤚 CEO). Solução
 * já desenhada com a Veronica-Bolso quando chegar lá: coluna `locked_at` +
 * `UPDATE ... WHERE locked_at IS NULL OR locked_at < now() - interval
 * '10 minutes' RETURNING id`. sync-runner.ts separa o scope_key por modo
 * (incremental nunca usa checkpoint; só backfill) especificamente pra evitar
 * a OUTRA corrida óbvia — um sync incremental "herdar" silenciosamente o
 * checkpoint de um backfill em andamento na mesma conta.
 */
import type { SupabaseClient } from "@supabase/supabase-js";

const SOURCE_TYPE = "pluggy";
const CURSOR_KIND = "page_number";

export interface CheckpointState {
  id: string;
  from: string;
  to: string;
  startPage: number;
}

export interface PageProgress {
  page: number;
  transactionsSeen: number;
  created: number;
  duplicates: number;
  errors: number;
}

export async function startOrResumeCheckpoint(
  supabase: SupabaseClient,
  userId: string,
  scopeKey: string,
  defaultWindow: { from: string; to: string },
): Promise<CheckpointState> {
  const { data: existing } = await supabase
    .from("ingestion_checkpoints")
    .select("id, cursor_value, window_start, window_end, status")
    .eq("user_id", userId)
    .eq("source_type", SOURCE_TYPE)
    .eq("scope_key", scopeKey)
    .maybeSingle();

  if (existing && existing.status !== "completed") {
    return {
      id: existing.id as string,
      from: (existing.window_start as string | null) ?? defaultWindow.from,
      to: (existing.window_end as string | null) ?? defaultWindow.to,
      startPage: existing.cursor_value ? Number(existing.cursor_value) : 1,
    };
  }

  const { data: created, error } = await supabase
    .from("ingestion_checkpoints")
    .upsert(
      {
        user_id: userId,
        source_type: SOURCE_TYPE,
        scope_key: scopeKey,
        cursor_kind: CURSOR_KIND,
        cursor_value: null,
        window_start: defaultWindow.from,
        window_end: defaultWindow.to,
        status: "running",
        messages_seen: 0,
        documents_created: 0,
        duplicates_skipped: 0,
        errors: 0,
      },
      { onConflict: "user_id,source_type,scope_key" },
    )
    .select("id")
    .single();

  if (error || !created) {
    throw new Error(`Falha ao criar checkpoint (${scopeKey}): ${error?.message}`);
  }

  return { id: created.id as string, from: defaultWindow.from, to: defaultWindow.to, startPage: 1 };
}

/** Chamar depois de processar cada página — persiste o progresso e onde retomar. */
export async function recordPageProgress(
  supabase: SupabaseClient,
  checkpointId: string,
  progress: PageProgress,
): Promise<void> {
  const { data: current } = await supabase
    .from("ingestion_checkpoints")
    .select("messages_seen, documents_created, duplicates_skipped, errors")
    .eq("id", checkpointId)
    .single();

  await supabase
    .from("ingestion_checkpoints")
    .update({
      cursor_value: String(progress.page + 1),
      messages_seen: (current?.messages_seen ?? 0) + progress.transactionsSeen,
      documents_created: (current?.documents_created ?? 0) + progress.created,
      duplicates_skipped: (current?.duplicates_skipped ?? 0) + progress.duplicates,
      errors: (current?.errors ?? 0) + progress.errors,
    })
    .eq("id", checkpointId);
}

export async function completeCheckpoint(
  supabase: SupabaseClient,
  checkpointId: string,
): Promise<void> {
  await supabase
    .from("ingestion_checkpoints")
    .update({ status: "completed" })
    .eq("id", checkpointId);
}
