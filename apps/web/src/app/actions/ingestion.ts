"use server";

import { createClient } from "@/lib/supabase/server";
import type {
  SourceDocument,
  IngestionJob,
  IngestionLog,
  IngestionRun,
  DraftBatch,
  DraftRecord,
} from "@sbf/shared-types";

// ══════════════════════════════════════════════════════════════
// Source Documents
// ══════════════════════════════════════════════════════════════

export async function getSourceDocuments(filters?: {
  status?: string;
  originType?: string;
  mimeType?: string;
  dateFrom?: string;
  dateTo?: string;
  search?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: SourceDocument[]; count: number }> {
  const supabase = await createClient();
  let query = supabase
    .from("source_documents")
    .select("*", { count: "exact" })
    .order("created_at", { ascending: false });

  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.originType) {
    query = query.eq("origin_type", filters.originType);
  }
  if (filters?.mimeType) {
    query = query.eq("mime_type", filters.mimeType);
  }
  if (filters?.dateFrom) {
    query = query.gte("created_at", filters.dateFrom);
  }
  if (filters?.dateTo) {
    query = query.lte("created_at", filters.dateTo);
  }
  if (filters?.search) {
    query = query.ilike("filename", `%${filters.search}%`);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }
  if (filters?.offset) {
    query = query.range(filters.offset, filters.offset + (filters?.limit ?? 20) - 1);
  }

  const { data, error, count } = await query;
  if (error) throw new Error(error.message);
  return { data: data ?? [], count: count ?? 0 };
}

export async function getSourceDocument(id: string): Promise<SourceDocument | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("source_documents").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function getDocumentWithRelations(id: string) {
  const supabase = await createClient();

  const [docResult, jobsResult, draftsResult] = await Promise.all([
    supabase.from("source_documents").select("*").eq("id", id).single(),
    supabase
      .from("ingestion_jobs")
      .select("*")
      .eq("source_document_id", id)
      .order("created_at", { ascending: false }),
    supabase
      .from("draft_records")
      .select("*")
      .eq("source_document_id", id)
      .order("created_at", { ascending: false }),
  ]);

  if (docResult.error) {
    if (docResult.error.code === "PGRST116") return null;
    throw new Error(docResult.error.message);
  }

  return {
    document: docResult.data as SourceDocument,
    jobs: (jobsResult.data ?? []) as IngestionJob[],
    drafts: (draftsResult.data ?? []) as DraftRecord[],
  };
}

export async function getDocumentStorageUrl(storagePath: string): Promise<string | null> {
  const supabase = await createClient();
  const { data } = await supabase.storage
    .from("ingestion-originals")
    .createSignedUrl(storagePath, 3600);
  return data?.signedUrl ?? null;
}

export async function reprocessDocument(documentId: string): Promise<void> {
  const supabase = await createClient();

  // Reset drafts linked to this document
  await supabase
    .from("draft_records")
    .update({ status: "archived" })
    .eq("source_document_id", documentId)
    .in("status", ["pending_review", "rejected"]);

  // Reset the latest job for this document to 'queued'
  const { data: latestJob } = await supabase
    .from("ingestion_jobs")
    .select("id")
    .eq("source_document_id", documentId)
    .order("created_at", { ascending: false })
    .limit(1)
    .single();

  if (latestJob) {
    await supabase
      .from("ingestion_jobs")
      .update({
        status: "queued",
        error_message: null,
        error_details: null,
        retry_count: 0,
        updated_at: new Date().toISOString(),
      })
      .eq("id", latestJob.id);
  }
}

// ══════════════════════════════════════════════════════════════
// Ingestion Jobs
// ══════════════════════════════════════════════════════════════

export async function getIngestionJobs(filters?: {
  sourceDocumentId?: string;
  status?: string;
  limit?: number;
}): Promise<IngestionJob[]> {
  const supabase = await createClient();
  let query = supabase.from("ingestion_jobs").select("*").order("created_at", { ascending: false });

  if (filters?.sourceDocumentId) {
    query = query.eq("source_document_id", filters.sourceDocumentId);
  }
  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data ?? [];
}

export async function getIngestionJob(id: string): Promise<IngestionJob | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("ingestion_jobs").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

// ══════════════════════════════════════════════════════════════
// Ingestion Runs
// ══════════════════════════════════════════════════════════════

export async function getIngestionRuns(limit = 20): Promise<IngestionRun[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("ingestion_runs")
    .select("*")
    .order("started_at", { ascending: false })
    .limit(limit);

  if (error) throw new Error(error.message);
  return data ?? [];
}

// ══════════════════════════════════════════════════════════════
// Draft Batches
// ══════════════════════════════════════════════════════════════

export async function getDraftBatches(filters?: {
  status?: string;
  limit?: number;
}): Promise<DraftBatch[]> {
  const supabase = await createClient();
  let query = supabase.from("draft_batches").select("*").order("created_at", { ascending: false });

  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data ?? [];
}

export async function getDraftBatch(id: string): Promise<DraftBatch | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("draft_batches").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

// ══════════════════════════════════════════════════════════════
// Draft Records
// ══════════════════════════════════════════════════════════════

export async function getDraftRecords(filters?: {
  batchId?: string;
  status?: string;
  limit?: number;
}): Promise<DraftRecord[]> {
  const supabase = await createClient();
  let query = supabase.from("draft_records").select("*").order("created_at", { ascending: false });

  if (filters?.batchId) {
    query = query.eq("batch_id", filters.batchId);
  }
  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data ?? [];
}

export async function approveDraftRecord(id: string): Promise<DraftRecord> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  const { data, error } = await supabase
    .from("draft_records")
    .update({ status: "approved", approved_at: new Date().toISOString(), approved_by: user?.id })
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateDraftData(
  id: string,
  draftData: Record<string, unknown>,
): Promise<DraftRecord> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("draft_records")
    .update({
      draft_data: draftData as unknown as Record<string, never>,
      status: "corrected",
      corrections: { manual_edit: true, edited_at: new Date().toISOString() },
      updated_at: new Date().toISOString(),
    })
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function rejectDraftRecord(id: string, reason?: string): Promise<DraftRecord> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("draft_records")
    .update({
      status: "rejected",
      corrections: reason ? { rejection_reason: reason } : null,
    })
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function approveDraftBatch(batchId: string): Promise<void> {
  const supabase = await createClient();

  const { error: recordsError } = await supabase
    .from("draft_records")
    .update({ status: "approved", approved_at: new Date().toISOString() })
    .eq("batch_id", batchId)
    .eq("status", "pending_review");

  if (recordsError) throw new Error(recordsError.message);

  const { error: batchError } = await supabase
    .from("draft_batches")
    .update({ status: "approved" })
    .eq("id", batchId);

  if (batchError) throw new Error(batchError.message);
}

// ══════════════════════════════════════════════════════════════
// Ingestion Logs
// ══════════════════════════════════════════════════════════════

export async function getIngestionLogs(filters?: {
  level?: string;
  jobId?: string;
  runId?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: IngestionLog[]; count: number }> {
  const supabase = await createClient();
  let query = supabase
    .from("ingestion_logs")
    .select("*", { count: "exact" })
    .order("created_at", { ascending: false });

  if (filters?.level) {
    query = query.eq("level", filters.level);
  }
  if (filters?.jobId) {
    query = query.eq("job_id", filters.jobId);
  }
  if (filters?.runId) {
    query = query.eq("run_id", filters.runId);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }
  if (filters?.offset) {
    query = query.range(filters.offset, filters.offset + (filters?.limit ?? 50) - 1);
  }

  const { data, error, count } = await query;
  if (error) throw new Error(error.message);
  return { data: data ?? [], count: count ?? 0 };
}

// ══════════════════════════════════════════════════════════════
// Ingestion Stats (para dashboard)
// ══════════════════════════════════════════════════════════════

export async function getIngestionStats(): Promise<{
  totalDocuments: number;
  pendingReview: number;
  totalDrafts: number;
  pendingDrafts: number;
  recentErrors: number;
}> {
  const supabase = await createClient();

  const [docsResult, pendingJobsResult, draftsResult, pendingDraftsResult, errorsResult] =
    await Promise.all([
      supabase.from("source_documents").select("*", { count: "exact", head: true }),
      supabase
        .from("ingestion_jobs")
        .select("*", { count: "exact", head: true })
        .eq("status", "pending_review"),
      supabase.from("draft_records").select("*", { count: "exact", head: true }),
      supabase
        .from("draft_records")
        .select("*", { count: "exact", head: true })
        .eq("status", "pending_review"),
      supabase
        .from("ingestion_logs")
        .select("*", { count: "exact", head: true })
        .eq("level", "error")
        .gte("created_at", new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString()),
    ]);

  return {
    totalDocuments: docsResult.count ?? 0,
    pendingReview: pendingJobsResult.count ?? 0,
    totalDrafts: draftsResult.count ?? 0,
    pendingDrafts: pendingDraftsResult.count ?? 0,
    recentErrors: errorsResult.count ?? 0,
  };
}
