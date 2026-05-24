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

async function computeContentHash(data: ArrayBuffer | Uint8Array): Promise<string> {
  const buffer = data instanceof Uint8Array ? data : new Uint8Array(data);
  const hashBuffer = await crypto.subtle.digest("SHA-256", buffer as unknown as BufferSource);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export type DeleteSourceDocumentMode = "document_only" | "document_and_ingestion";

function throwIfError(error: { message: string } | null, context: string): void {
  if (error) {
    throw new Error(`Erro ao ${context}: ${error.message}`);
  }
}

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

  if (!filters?.status) {
    query = query.neq("status", "deleted");
  }

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

export async function getLatestIngestionJobByDocumentId(
  sourceDocumentId: string,
): Promise<IngestionJob | null> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("ingestion_jobs")
    .select("*")
    .eq("source_document_id", sourceDocumentId)
    .order("created_at", { ascending: false })
    .limit(1)
    .maybeSingle();

  if (error) throw new Error(error.message);
  return data ?? null;
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
  source_document_id?: string;
}): Promise<DraftBatch[]> {
  const supabase = await createClient();
  let query = supabase.from("draft_batches").select("*").order("created_at", { ascending: false });

  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.source_document_id) {
    query = query.eq("source_document_id", filters.source_document_id);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data ?? [];
}

export async function getDraftBatchByDocumentId(
  sourceDocumentId: string,
): Promise<DraftBatch | null> {
  const batches = await getDraftBatches({ source_document_id: sourceDocumentId, limit: 1 });
  if (batches[0]) return batches[0];

  // Fallback para batches antigos que não tinham source_document_id preenchido.
  const supabase = await createClient();
  const { data: latestDraft, error } = await supabase
    .from("draft_records")
    .select("batch_id")
    .eq("source_document_id", sourceDocumentId)
    .not("batch_id", "is", null)
    .order("created_at", { ascending: false })
    .limit(1)
    .maybeSingle();

  if (error) throw new Error(error.message);
  if (!latestDraft?.batch_id) return null;

  return getDraftBatch(latestDraft.batch_id);
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

// ══════════════════════════════════════════════════════════════
// Upload Manual de Documento (S2-004)
// Cria registro em source_documents + ingestion_job + dispara run
// ══════════════════════════════════════════════════════════════

export async function uploadDocument(formData: FormData): Promise<SourceDocument> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const file = formData.get("file") as File | null;
  if (!file) throw new Error("Arquivo não informado");

  // 0. Deduplicação exata por hash ANTES de criar source_document/job.
  // Isso evita abrir novo fluxo para arquivo já ingerido.
  const fileBytes = new Uint8Array(await file.arrayBuffer());
  const contentHash = await computeContentHash(fileBytes);

  const { data: duplicateFingerprint } = await supabase
    .from("document_fingerprints")
    .select("source_document_id")
    .eq("user_id", user.id)
    .eq("content_hash", contentHash)
    .limit(1)
    .maybeSingle();

  if (duplicateFingerprint?.source_document_id) {
    throw new Error(
      `Documento duplicado detectado (mesmo hash). ID original: ${duplicateFingerprint.source_document_id}`,
    );
  }

  // 1. Upload para Storage (bucket ingestion-originals)
  const ext = file.name.split(".").pop() ?? "bin";
  const storagePath = `${user.id}/manual/${crypto.randomUUID()}.${ext}`;
  const { error: uploadError } = await supabase.storage
    .from("ingestion-originals")
    .upload(storagePath, file, { contentType: file.type, upsert: false });

  if (uploadError) throw new Error(`Erro no upload: ${uploadError.message}`);

  // 2. Criar source_document
  const originKey = `manual::${storagePath}`;
  const { data: srcDoc, error: srcError } = await supabase
    .from("source_documents")
    .insert({
      user_id: user.id,
      origin_type: "manual_upload",
      origin_key: originKey,
      filename: file.name,
      mime_type: file.type || ext,
      file_size_bytes: file.size,
      storage_path: storagePath,
      content_hash: contentHash,
      status: "queued",
    })
    .select()
    .single();

  if (srcError) throw new Error(`Erro ao registrar documento: ${srcError.message}`);

  // 3. Criar ingestion_run + ingestion_job para processar o documento
  const { data: run, error: runError } = await supabase
    .from("ingestion_runs")
    .insert({
      user_id: user.id,
      source_type: "manual_upload",
      status: "running",
      metadata: { source: "manual_upload", filename: file.name },
    })
    .select()
    .single();

  if (runError) throw new Error(`Erro ao criar run: ${runError.message}`);

  await supabase.from("ingestion_jobs").insert({
    run_id: run.id,
    user_id: user.id,
    source_document_id: srcDoc.id,
    status: "discovered",
    metadata: { source: "manual_upload" },
  });

  return srcDoc;
}

// ══════════════════════════════════════════════════════════════
// Deletar Documento
// document_only: remove o arquivo original e oculta o documento da lista,
// preservando trilha de ingestão e auditoria.
// document_and_ingestion: remove documento + artefatos de ingestão vinculados.
// ══════════════════════════════════════════════════════════════

export async function deleteSourceDocument(
  id: string,
  mode: DeleteSourceDocumentMode = "document_and_ingestion",
): Promise<void> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Busca o documento garantindo que pertence ao usuário (RLS)
  const { data: doc, error: fetchError } = await supabase
    .from("source_documents")
    .select("id, storage_path, content_hash, metadata, status")
    .eq("id", id)
    .eq("user_id", user.id)
    .single();

  if (fetchError || !doc) throw new Error("Documento não encontrado ou sem permissão");

  const now = new Date().toISOString();

  // 1. Remove arquivo do Storage.
  if (doc.storage_path) {
    const { error: storageError } = await supabase.storage
      .from("ingestion-originals")
      .remove([doc.storage_path]);
    throwIfError(storageError, "remover arquivo do storage");
  }

  if (mode === "document_only") {
    const metadata = {
      ...((doc.metadata as Record<string, unknown> | null) ?? {}),
      deletion: {
        mode,
        deleted_at: now,
        deleted_by: user.id,
        original_status: doc.status,
        original_storage_path: doc.storage_path,
      },
    };

    const { error: updateError } = await supabase
      .from("source_documents")
      .update({
        status: "deleted",
        storage_path: null,
        updated_at: now,
        metadata: metadata as unknown as Record<string, never>,
      })
      .eq("id", id)
      .eq("user_id", user.id);

    throwIfError(updateError, "marcar documento como removido");
    return;
  }

  const [{ data: jobs, error: jobsError }, { data: parsedVersions, error: versionsError }] =
    await Promise.all([
      supabase
        .from("ingestion_jobs")
        .select("id, run_id")
        .eq("source_document_id", id)
        .eq("user_id", user.id),
      supabase
        .from("parsed_document_versions")
        .select("id")
        .eq("source_document_id", id)
        .eq("user_id", user.id),
    ]);

  throwIfError(jobsError, "buscar jobs de ingestão do documento");
  throwIfError(versionsError, "buscar versões parseadas do documento");

  const jobIds = (jobs ?? []).map((job) => job.id);
  const parsedVersionIds = (parsedVersions ?? []).map((version) => version.id);

  // 2. Remove dependências explícitas na ordem das FKs.
  const { error: draftsError } = await supabase
    .from("draft_records")
    .delete()
    .eq("source_document_id", id)
    .eq("user_id", user.id);
  throwIfError(draftsError, "remover drafts do documento");

  const { error: feedbackError } = await supabase
    .from("pattern_feedback")
    .delete()
    .eq("source_document_id", id)
    .eq("user_id", user.id);
  throwIfError(feedbackError, "remover feedbacks de padrões do documento");

  if (parsedVersionIds.length > 0) {
    const { error: extractionError } = await supabase
      .from("extraction_results")
      .delete()
      .in("parsed_version_id", parsedVersionIds)
      .eq("user_id", user.id);
    throwIfError(extractionError, "remover resultados de extração do documento");
  }

  const { error: parsedVersionsDeleteError } = await supabase
    .from("parsed_document_versions")
    .delete()
    .eq("source_document_id", id)
    .eq("user_id", user.id);
  throwIfError(parsedVersionsDeleteError, "remover versões parseadas do documento");

  const { error: batchesError } = await supabase
    .from("draft_batches")
    .delete()
    .eq("source_document_id", id)
    .eq("user_id", user.id);
  throwIfError(batchesError, "remover batches vinculados ao documento");

  const { error: fingerprintError } = await supabase
    .from("document_fingerprints")
    .delete()
    .eq("source_document_id", id)
    .eq("user_id", user.id);
  throwIfError(fingerprintError, "remover fingerprints do documento");

  if (jobIds.length > 0) {
    const { error: logsByJobError } = await supabase
      .from("ingestion_logs")
      .delete()
      .in("job_id", jobIds)
      .eq("user_id", user.id);
    throwIfError(logsByJobError, "remover logs dos jobs do documento");
  }

  const { error: jobsDeleteError } = await supabase
    .from("ingestion_jobs")
    .delete()
    .eq("source_document_id", id)
    .eq("user_id", user.id);
  throwIfError(jobsDeleteError, "remover jobs de ingestão do documento");

  // Não removemos ingestion_runs aqui para evitar falhas por relacionamentos
  // compartilhados (ex.: draft_batches de outros documentos na mesma run).
  // A limpeza de runs órfãs deve ser feita por rotina de manutenção dedicada.

  // 3. Remove o documento em si. Relacionamentos como transactions usam ON DELETE SET NULL.
  const { error: deleteError } = await supabase
    .from("source_documents")
    .delete()
    .eq("id", id)
    .eq("user_id", user.id);

  throwIfError(deleteError, "deletar documento");
}

// ══════════════════════════════════════════════════════════════
// Autopilot / IA
// ══════════════════════════════════════════════════════════════

export interface AutopilotResult {
  documentId: string;
  status: "complete" | "needs_review" | "failed";
  extraction: {
    supplier_name: string | null;
    total_amount: number | null;
    due_date: string | null;
    competence_date: string | null;
    financial_intent: string | null;
    document_type: string | null;
    ai_enrichment_type: string | null;
    confidence_per_field: Record<string, number> | null;
    reasoning: string | null;
  } | null;
  pipeline: {
    parser_used: string;
    ai_enriched: boolean;
    needs_full_ai_review: boolean;
    job_status: string | null;
  };
  drafts: {
    count: number;
    batch_id: string | null;
  };
  issues: string[];
  decision_suggestion: "approve" | "review_required" | "reject";
}

/**
 * Retorna um resumo autopilot do estado atual de ingestão de um documento.
 * Não dispara novo processamento — apenas consolida o estado atual.
 */
export async function getAutopilotStatus(documentId: string): Promise<AutopilotResult> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Buscar documento
  const { data: doc, error: docError } = await supabase
    .from("source_documents")
    .select("id, filename, mime_type, status")
    .eq("id", documentId)
    .eq("user_id", user.id)
    .single();

  if (docError || !doc) throw new Error("Documento não encontrado");

  // Último job
  const { data: job } = await supabase
    .from("ingestion_jobs")
    .select("id, status, metadata, needs_full_ai_review")
    .eq("source_document_id", documentId)
    .eq("user_id", user.id)
    .order("created_at", { ascending: false })
    .limit(1)
    .maybeSingle();

  const meta = (job?.metadata ?? {}) as Record<string, unknown>;
  const extractionResultId = meta.extraction_result_id as string | null;

  // Extraction result
  let extraction: AutopilotResult["extraction"] = null;
  if (extractionResultId) {
    const { data: er } = await supabase
      .from("extraction_results")
      .select(
        "supplier_name_raw, total_amount, due_date, competence_date, financial_intent, ai_enrichment_type, confidence_per_field, reasoning",
      )
      .eq("id", extractionResultId)
      .single();

    if (er) {
      extraction = {
        supplier_name: er.supplier_name_raw,
        total_amount: er.total_amount,
        due_date: er.due_date,
        competence_date: er.competence_date,
        financial_intent: er.financial_intent ?? null,
        document_type: (meta.document_type as string) ?? null,
        ai_enrichment_type: er.ai_enrichment_type ?? null,
        confidence_per_field: (er.confidence_per_field as Record<string, number>) ?? null,
        reasoning: er.reasoning ?? null,
      };
    }
  }

  // Drafts
  const { data: draftBatches } = await supabase
    .from("draft_batches")
    .select("id")
    .eq("source_document_id", documentId)
    .eq("user_id", user.id)
    .limit(1)
    .maybeSingle();

  const batchId = draftBatches?.id ?? null;

  let draftCount = 0;
  if (batchId) {
    const { count } = await supabase
      .from("draft_records")
      .select("id", { count: "exact", head: true })
      .eq("batch_id", batchId)
      .eq("user_id", user.id);
    draftCount = count ?? 0;
  }

  // Montar issues
  const issues: string[] = [];
  if (!extraction?.total_amount) issues.push("Valor total não identificado");
  if (!extraction?.due_date) issues.push("Data de vencimento não identificada");
  if (!extraction?.supplier_name) issues.push("Fornecedor não identificado");
  if (job?.needs_full_ai_review) issues.push("Requer análise de IA completa (visão)");
  if (draftCount === 0) issues.push("Nenhum draft gerado para revisão");

  const needsFullReview = (job?.needs_full_ai_review as boolean) ?? false;
  const jobStatus = job?.status ?? null;
  const aiEnriched = (meta.ai_enriched as boolean) ?? false;

  const decisionSuggestion: AutopilotResult["decision_suggestion"] =
    issues.length === 0
      ? "approve"
      : issues.some((i) => i.includes("Valor") || i.includes("Nenhum draft"))
        ? "reject"
        : "review_required";

  return {
    documentId,
    status: issues.length === 0 ? "complete" : jobStatus === "failed" ? "failed" : "needs_review",
    extraction,
    pipeline: {
      parser_used: (meta.parser_type as string) ?? "unknown",
      ai_enriched: aiEnriched,
      needs_full_ai_review: needsFullReview,
      job_status: jobStatus,
    },
    drafts: { count: draftCount, batch_id: batchId },
    issues,
    decision_suggestion: decisionSuggestion,
  };
}

/**
 * Dispara análise de IA full (visão) para um documento específico.
 * Cria um novo job com flag ai_mode = 'full' para o worker processar.
 */
export async function triggerAiFullAnalysis(documentId: string): Promise<{ jobId: string }> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Verificar que documento pertence ao usuário
  const { data: doc, error: docError } = await supabase
    .from("source_documents")
    .select("id, filename")
    .eq("id", documentId)
    .eq("user_id", user.id)
    .single();

  if (docError || !doc) throw new Error("Documento não encontrado");

  // Buscar ou criar run para o job
  const { data: run } = await supabase
    .from("ingestion_runs")
    .insert({
      user_id: user.id,
      source_type: "manual_upload",
      status: "running",
      metadata: { triggered_by: "ai_full_request", document_id: documentId },
    })
    .select("id")
    .single();

  if (!run) throw new Error("Falha ao criar run para análise full");

  // Criar job com ai_mode full a partir do estado parsed
  const { data: job } = await supabase
    .from("ingestion_jobs")
    .insert({
      run_id: run.id,
      user_id: user.id,
      source_document_id: documentId,
      status: "queued",
      metadata: {
        ai_mode: "full",
        force_reprocess: true,
        triggered_by: "user_request",
      },
    })
    .select("id")
    .single();

  if (!job) throw new Error("Falha ao criar job de análise full");

  await supabase.from("ingestion_runs").update({ status: "completed" }).eq("id", run.id);

  return { jobId: job.id };
}
