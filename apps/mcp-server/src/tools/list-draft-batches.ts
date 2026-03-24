/**
 * @sbf/mcp-server — Tool: list_draft_batches
 * Lista batches de drafts pendentes de revisão.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { DraftBatchStatus } from "@sbf/ingestion-types";

export interface DraftBatchSummary {
  id: string;
  status: string;
  draft_count: number;
  source_document_filename: string | null;
  created_at: string;
  drafts: Array<{
    id: string;
    record_type: string;
    status: string;
    confidence: number | null;
    suggested_description: string | null;
  }>;
}

export async function listDraftBatches(
  supabase: SupabaseClient,
  userId: string,
  opts?: { status?: string; limit?: number },
): Promise<DraftBatchSummary[]> {
  const limit = opts?.limit ?? 20;
  const status = opts?.status ?? DraftBatchStatus.OPEN;

  const { data: batches, error } = await supabase
    .from("draft_batches")
    .select(`
      id,
      status,
      source_document_id,
      created_at,
      source_documents(filename),
      draft_records(id, record_type, status, confidence, payload)
    `)
    .eq("user_id", userId)
    .eq("status", status)
    .order("created_at", { ascending: false })
    .limit(limit);

  if (error || !batches) return [];

  return batches.map((b: any) => ({
    id: b.id,
    status: b.status,
    draft_count: b.draft_records?.length ?? 0,
    source_document_filename: b.source_documents?.filename ?? null,
    created_at: b.created_at,
    drafts: (b.draft_records ?? []).map((d: any) => ({
      id: d.id,
      record_type: d.record_type,
      status: d.status,
      confidence: d.confidence,
      suggested_description: d.payload?.description ?? d.payload?.suggested_description ?? null,
    })),
  }));
}
