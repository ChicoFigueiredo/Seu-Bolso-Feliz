/**
 * @sbf/mcp-server — Tool: approve_draft_batch
 * Aprova um batch de drafts inteiro ou drafts individuais.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { DraftRecordStatus, DraftBatchStatus } from "@sbf/ingestion-types";

export interface ApproveResult {
  batchId: string;
  approvedCount: number;
  rejectedCount: number;
  batchStatus: string;
}

export async function approveDraftBatch(
  supabase: SupabaseClient,
  userId: string,
  batchId: string,
  opts?: { rejectDraftIds?: string[] },
): Promise<ApproveResult> {
  // Verificar ownership do batch
  const { data: batch, error } = await supabase
    .from("draft_batches")
    .select("id, status")
    .eq("id", batchId)
    .eq("user_id", userId)
    .single();

  if (error || !batch) {
    throw new Error(`Batch not found or not owned by user: ${batchId}`);
  }

  if (batch.status !== DraftBatchStatus.OPEN && batch.status !== DraftBatchStatus.REVIEWING) {
    throw new Error(`Batch ${batchId} is in status ${batch.status} — cannot approve`);
  }

  const rejectIds = new Set(opts?.rejectDraftIds ?? []);

  // Buscar todos os drafts do batch
  const { data: drafts } = await supabase
    .from("draft_records")
    .select("id")
    .eq("batch_id", batchId)
    .eq("user_id", userId)
    .eq("status", DraftRecordStatus.PENDING_REVIEW);

  if (!drafts || drafts.length === 0) {
    throw new Error("No pending drafts found in this batch");
  }

  let approvedCount = 0;
  let rejectedCount = 0;

  for (const draft of drafts) {
    if (rejectIds.has(draft.id)) {
      await supabase
        .from("draft_records")
        .update({ status: DraftRecordStatus.REJECTED, updated_at: new Date().toISOString() })
        .eq("id", draft.id);
      rejectedCount++;
    } else {
      await supabase
        .from("draft_records")
        .update({ status: DraftRecordStatus.APPROVED, updated_at: new Date().toISOString() })
        .eq("id", draft.id);
      approvedCount++;
    }
  }

  const batchStatus = rejectedCount > 0 && approvedCount > 0
    ? DraftBatchStatus.PARTIAL
    : rejectedCount === drafts.length
      ? DraftBatchStatus.REJECTED
      : DraftBatchStatus.APPROVED;

  await supabase
    .from("draft_batches")
    .update({ status: batchStatus, updated_at: new Date().toISOString() })
    .eq("id", batchId);

  return { batchId, approvedCount, rejectedCount, batchStatus };
}
