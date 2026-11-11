/**
 * @sbf/worker-ingestion — Logger estruturado
 * Escreve logs tanto no console quanto na tabela ingestion_logs.
 */
import { ingestion_logs, type Database } from "@sbf/db";

/** O tipo que o schema declara para a coluna jsonb. */
type JsonColumn = (typeof ingestion_logs.$inferInsert)["details"];
import { IngestionLogLevel } from "@sbf/ingestion-types";

export interface LogContext {
  userId: string;
  runId?: string;
  jobId?: string;
}

export async function writeLog(
  db: Database,
  ctx: LogContext,
  level: (typeof IngestionLogLevel)[keyof typeof IngestionLogLevel],
  message: string,
  details?: Record<string, unknown>,
): Promise<void> {
  const timestamp = new Date().toISOString();
  const prefix = `[${level.toUpperCase()}] [run:${ctx.runId ?? "-"}] [job:${ctx.jobId ?? "-"}]`;
  console.log(`${prefix} ${message}`, details ?? "");

  try {
    await db.insert(ingestion_logs).values({
      user_id: ctx.userId,
      run_id: ctx.runId ?? null,
      job_id: ctx.jobId ?? null,
      level,
      message,
      // `details` é sempre um objeto de log montado aqui no código, nunca um
      // valor arbitrário do usuário — o cast só reconcilia `unknown` com o
      // `Json` recursivo que o schema declara.
      details: (details ?? null) as JsonColumn,
      created_at: timestamp,
    });
  } catch {
    // Se falhar ao gravar log no banco, não deve derrubar o worker
    console.error(`[LOGGER] Failed to persist log to DB: ${message}`);
  }
}
