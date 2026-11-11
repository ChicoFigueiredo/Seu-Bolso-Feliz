/**
 * Leitura de jobs da fila **restrita a um usuário**, para os testes de pipeline.
 *
 * `claimJobs()` de `@sbf/db` é o caminho de produção e não filtra por usuário —
 * é o desenho certo lá (um worker atende todo mundo) e o errado aqui: vários
 * arquivos de teste dividem o mesmo Postgres, e um `claim` cego reivindicaria
 * o job que outro arquivo acabou de criar. Estas funções leem por `user_id`,
 * sem tocar no lock, e devolvem o job no mesmo formato camelCase que
 * `processJob` espera.
 *
 * Quando o que está sob teste é a fila em si (lock, SKIP LOCKED, backoff,
 * NOTIFY), o lugar é `__tests__/integration/queue.test.ts` — não aqui.
 */
import { sql } from "drizzle-orm";
import { parsePgTextArray, type Database, type QueueJob } from "@sbf/db";
import { linhas } from "./db";

const COLUNAS = sql`id, run_id, user_id, source_document_id, status, error_message,
  retry_count, max_retries, metadata, requires, locked_by, locked_at,
  available_at, created_at, updated_at`;

function paraIso(valor: unknown): string | null {
  if (valor === null || valor === undefined) return null;
  return valor instanceof Date ? valor.toISOString() : String(valor);
}

function paraObjeto(valor: unknown): Record<string, unknown> {
  if (valor === null || valor === undefined) return {};
  if (typeof valor === "string") {
    try {
      const parsed: unknown = JSON.parse(valor);
      return typeof parsed === "object" && parsed !== null
        ? (parsed as Record<string, unknown>)
        : {};
    } catch {
      return {};
    }
  }
  return typeof valor === "object" ? (valor as Record<string, unknown>) : {};
}

function mapear(row: Record<string, unknown>): QueueJob {
  return {
    id: String(row.id),
    runId: String(row.run_id),
    userId: String(row.user_id),
    sourceDocumentId: row.source_document_id === null ? null : String(row.source_document_id),
    status: String(row.status),
    errorMessage: row.error_message === null ? null : String(row.error_message ?? ""),
    retryCount: Number(row.retry_count ?? 0),
    maxRetries: Number(row.max_retries ?? 3),
    metadata: paraObjeto(row.metadata),
    requires: parsePgTextArray(row.requires),
    lockedBy: row.locked_by === null ? null : String(row.locked_by),
    lockedAt: paraIso(row.locked_at),
    availableAt: paraIso(row.available_at) ?? new Date().toISOString(),
    createdAt: paraIso(row.created_at),
    updatedAt: paraIso(row.updated_at),
  };
}

/** Todos os jobs do usuário, do mais antigo para o mais novo. */
export async function jobsDoUsuario(db: Database, userId: string): Promise<QueueJob[]> {
  const rows = await linhas(
    db,
    sql`select ${COLUNAS} from public.ingestion_jobs
         where user_id = ${userId}::uuid order by created_at asc`,
  );
  return rows.map(mapear);
}

/** Um job específico, relido do banco. Lança se ele não existir mais. */
export async function jobPorId(db: Database, jobId: string): Promise<QueueJob> {
  const rows = await linhas(
    db,
    sql`select ${COLUNAS} from public.ingestion_jobs where id = ${jobId}::uuid`,
  );
  const row = rows[0];
  if (!row) throw new Error(`job ${jobId} não existe mais`);
  return mapear(row);
}
