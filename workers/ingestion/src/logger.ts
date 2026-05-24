/**
 * @sbf/worker-ingestion — Logger estruturado
 * Escreve logs tanto no console quanto na tabela ingestion_logs.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionLogLevel } from "@sbf/ingestion-types";

export interface LogContext {
  userId: string;
  runId?: string;
  jobId?: string;
}

export async function writeLog(
  supabase: SupabaseClient,
  ctx: LogContext,
  level: (typeof IngestionLogLevel)[keyof typeof IngestionLogLevel],
  message: string,
  details?: Record<string, unknown>,
): Promise<void> {
  const timestamp = new Date().toISOString();
  const prefix = `[${level.toUpperCase()}] [run:${ctx.runId ?? "-"}] [job:${ctx.jobId ?? "-"}]`;
  console.log(`${prefix} ${message}`, details ?? "");

  try {
    await supabase.from("ingestion_logs").insert({
      user_id: ctx.userId,
      run_id: ctx.runId ?? null,
      job_id: ctx.jobId ?? null,
      level,
      message,
      details: details ?? null,
      created_at: timestamp,
    });
  } catch {
    // Se falhar ao gravar log no banco, não deve derrubar o worker
    console.error(`[LOGGER] Failed to persist log to DB: ${message}`);
  }
}
