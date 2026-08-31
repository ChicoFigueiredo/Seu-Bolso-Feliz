/**
 * @sbf/mcp-server — Tool: approve_draft_batch
 * Aprova um batch de drafts inteiro ou drafts individuais.
 */
import { and, eq, inArray } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
import { DraftRecordStatus, DraftBatchStatus } from "@sbf/ingestion-types";

export interface ApproveResult {
  batchId: string;
  approvedCount: number;
  rejectedCount: number;
  batchStatus: string;
}

export async function approveDraftBatch(
  db: Database,
  userId: string,
  batchId: string,
  opts?: { rejectDraftIds?: string[] },
): Promise<ApproveResult> {
  // Verificar ownership do batch
  const [batch] = await db
    .select({ id: schema.draft_batches.id, status: schema.draft_batches.status })
    .from(schema.draft_batches)
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, userId)));

  if (!batch) {
    throw new Error(`Batch not found or not owned by user: ${batchId}`);
  }

  if (batch.status !== DraftBatchStatus.OPEN && batch.status !== DraftBatchStatus.REVIEWING) {
    throw new Error(`Batch ${batchId} is in status ${batch.status} — cannot approve`);
  }

  // Buscar todos os drafts pendentes do batch, já escopados por dono
  const drafts = await db
    .select({ id: schema.draft_records.id })
    .from(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.batch_id, batchId),
        eq(schema.draft_records.user_id, userId),
        eq(schema.draft_records.status, DraftRecordStatus.PENDING_REVIEW),
      ),
    );

  if (drafts.length === 0) {
    throw new Error("No pending drafts found in this batch");
  }

  const rejectIds = new Set(opts?.rejectDraftIds ?? []);
  const toReject = drafts.filter((d) => rejectIds.has(d.id)).map((d) => d.id);
  const toApprove = drafts.filter((d) => !rejectIds.has(d.id)).map((d) => d.id);
  const now = new Date().toISOString();

  // Dois UPDATEs em lote (por IN) em vez do loop item-a-item do código
  // original — mesmo resultado, menos round-trips. `user_id` reforçado em
  // cada WHERE mesmo os ids já vindo de uma leitura escopada: não há mais
  // RLS de graça, então toda escrita filtra dono explicitamente.
  if (toReject.length > 0) {
    await db
      .update(schema.draft_records)
      .set({ status: DraftRecordStatus.REJECTED, updated_at: now })
      .where(
        and(inArray(schema.draft_records.id, toReject), eq(schema.draft_records.user_id, userId)),
      );
  }
  if (toApprove.length > 0) {
    await db
      .update(schema.draft_records)
      .set({ status: DraftRecordStatus.APPROVED, updated_at: now })
      .where(
        and(inArray(schema.draft_records.id, toApprove), eq(schema.draft_records.user_id, userId)),
      );
  }

  const approvedCount = toApprove.length;
  const rejectedCount = toReject.length;

  const batchStatus =
    rejectedCount > 0 && approvedCount > 0
      ? DraftBatchStatus.PARTIAL
      : rejectedCount === drafts.length
        ? DraftBatchStatus.REJECTED
        : DraftBatchStatus.APPROVED;

  await db
    .update(schema.draft_batches)
    .set({ status: batchStatus, updated_at: now })
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, userId)));

  return { batchId, approvedCount, rejectedCount, batchStatus };
}
