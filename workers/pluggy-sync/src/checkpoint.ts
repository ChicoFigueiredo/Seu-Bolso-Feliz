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
 *
 * A mesma pendência vale pro read-modify-write de `recordPageProgress`
 * (SELECT dos contadores + UPDATE com a soma, em vez de `SET col = col + n`
 * no próprio banco): tradução 1:1 do que existia no PostgREST, mantida de
 * propósito nesta migração pra Drizzle.
 */
import { and, eq } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";

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
  db: Database,
  userId: string,
  scopeKey: string,
  defaultWindow: { from: string; to: string },
): Promise<CheckpointState> {
  // `.limit(1)` no lugar do `.maybeSingle()` do PostgREST: a unique
  // (user_id, source_type, scope_key) garante no máximo uma linha.
  const [existing] = await db
    .select({
      id: schema.ingestion_checkpoints.id,
      cursor_value: schema.ingestion_checkpoints.cursor_value,
      window_start: schema.ingestion_checkpoints.window_start,
      window_end: schema.ingestion_checkpoints.window_end,
      status: schema.ingestion_checkpoints.status,
    })
    .from(schema.ingestion_checkpoints)
    .where(
      and(
        eq(schema.ingestion_checkpoints.user_id, userId),
        eq(schema.ingestion_checkpoints.source_type, SOURCE_TYPE),
        eq(schema.ingestion_checkpoints.scope_key, scopeKey),
      ),
    )
    .limit(1);

  if (existing && existing.status !== "completed") {
    return {
      id: existing.id,
      from: existing.window_start ?? defaultWindow.from,
      to: existing.window_end ?? defaultWindow.to,
      startPage: existing.cursor_value ? Number(existing.cursor_value) : 1,
    };
  }

  // Equivalente ao `.upsert(row, { onConflict: "user_id,source_type,scope_key" })`:
  // INSERT ... ON CONFLICT (as três colunas da unique) DO UPDATE SET <as mesmas
  // colunas do INSERT>. As colunas do target ficam fora do `set` porque
  // reescrevê-las com o próprio valor é no-op.
  const insertValues = {
    cursor_kind: CURSOR_KIND,
    cursor_value: null,
    window_start: defaultWindow.from,
    window_end: defaultWindow.to,
    status: "running",
    messages_seen: 0,
    documents_created: 0,
    duplicates_skipped: 0,
    errors: 0,
  } as const;

  let created: { id: string } | undefined;
  try {
    [created] = await db
      .insert(schema.ingestion_checkpoints)
      .values({
        user_id: userId,
        source_type: SOURCE_TYPE,
        scope_key: scopeKey,
        ...insertValues,
      })
      .onConflictDoUpdate({
        target: [
          schema.ingestion_checkpoints.user_id,
          schema.ingestion_checkpoints.source_type,
          schema.ingestion_checkpoints.scope_key,
        ],
        set: { ...insertValues },
      })
      .returning({ id: schema.ingestion_checkpoints.id });
  } catch (err) {
    throw new Error(
      `Falha ao criar checkpoint (${scopeKey}): ${err instanceof Error ? err.message : String(err)}`,
      { cause: err },
    );
  }

  if (!created) {
    throw new Error(`Falha ao criar checkpoint (${scopeKey}): upsert não retornou linha`);
  }

  return { id: created.id, from: defaultWindow.from, to: defaultWindow.to, startPage: 1 };
}

/** Chamar depois de processar cada página — persiste o progresso e onde retomar. */
export async function recordPageProgress(
  db: Database,
  checkpointId: string,
  progress: PageProgress,
): Promise<void> {
  const [current] = await db
    .select({
      messages_seen: schema.ingestion_checkpoints.messages_seen,
      documents_created: schema.ingestion_checkpoints.documents_created,
      duplicates_skipped: schema.ingestion_checkpoints.duplicates_skipped,
      errors: schema.ingestion_checkpoints.errors,
    })
    .from(schema.ingestion_checkpoints)
    .where(eq(schema.ingestion_checkpoints.id, checkpointId))
    .limit(1);

  await db
    .update(schema.ingestion_checkpoints)
    .set({
      cursor_value: String(progress.page + 1),
      messages_seen: (current?.messages_seen ?? 0) + progress.transactionsSeen,
      documents_created: (current?.documents_created ?? 0) + progress.created,
      duplicates_skipped: (current?.duplicates_skipped ?? 0) + progress.duplicates,
      errors: (current?.errors ?? 0) + progress.errors,
    })
    .where(eq(schema.ingestion_checkpoints.id, checkpointId));
}

export async function completeCheckpoint(db: Database, checkpointId: string): Promise<void> {
  await db
    .update(schema.ingestion_checkpoints)
    .set({ status: "completed" })
    .where(eq(schema.ingestion_checkpoints.id, checkpointId));
}
