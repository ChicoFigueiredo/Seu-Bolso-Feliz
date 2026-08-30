"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { createStorageClientFromEnv } from "@sbf/storage";
import { and, count, desc, eq, gte, ilike, inArray, isNotNull, lte, ne } from "drizzle-orm";
import type { Json } from "@sbf/shared-types";

async function computeContentHash(data: ArrayBuffer | Uint8Array): Promise<string> {
  const buffer = data instanceof Uint8Array ? data : new Uint8Array(data);
  const hashBuffer = await crypto.subtle.digest("SHA-256", buffer as unknown as BufferSource);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export type DeleteSourceDocumentMode = "document_only" | "document_and_ingestion";

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

type SourceDocument = typeof schema.source_documents.$inferSelect;
type IngestionJob = typeof schema.ingestion_jobs.$inferSelect;
type IngestionLog = typeof schema.ingestion_logs.$inferSelect;
type IngestionRun = typeof schema.ingestion_runs.$inferSelect;
type DraftBatch = typeof schema.draft_batches.$inferSelect;
type DraftRecord = typeof schema.draft_records.$inferSelect;

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
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.source_documents.user_id, userId)];

  if (!filters?.status) {
    conditions.push(ne(schema.source_documents.status, "deleted"));
  }
  if (filters?.status) {
    conditions.push(eq(schema.source_documents.status, filters.status));
  }
  if (filters?.originType) {
    conditions.push(eq(schema.source_documents.origin_type, filters.originType));
  }
  if (filters?.mimeType) {
    conditions.push(eq(schema.source_documents.mime_type, filters.mimeType));
  }
  if (filters?.dateFrom) {
    conditions.push(gte(schema.source_documents.created_at, filters.dateFrom));
  }
  if (filters?.dateTo) {
    conditions.push(lte(schema.source_documents.created_at, filters.dateTo));
  }
  if (filters?.search) {
    conditions.push(ilike(schema.source_documents.filename, `%${filters.search}%`));
  }

  const where = and(...conditions);

  const [totalRow] = await db.select({ value: count() }).from(schema.source_documents).where(where);
  const total = totalRow?.value ?? 0;
  const orderBy = desc(schema.source_documents.created_at);

  let rows: SourceDocument[];
  if (filters?.offset !== undefined) {
    const pageSize = filters?.limit ?? 20;
    rows = await db
      .select()
      .from(schema.source_documents)
      .where(where)
      .orderBy(orderBy)
      .limit(pageSize)
      .offset(filters.offset);
  } else if (filters?.limit) {
    rows = await db
      .select()
      .from(schema.source_documents)
      .where(where)
      .orderBy(orderBy)
      .limit(filters.limit);
  } else {
    rows = await db.select().from(schema.source_documents).where(where).orderBy(orderBy);
  }

  return { data: rows, count: total };
}

export async function getSourceDocument(id: string): Promise<SourceDocument | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.source_documents)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)));
  return row ?? null;
}

export async function getDocumentWithRelations(id: string) {
  const userId = await requireUserId();
  const db = getDb();

  const [docRows, jobs, drafts] = await Promise.all([
    db
      .select()
      .from(schema.source_documents)
      .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId))),
    db
      .select()
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.source_document_id, id),
          eq(schema.ingestion_jobs.user_id, userId),
        ),
      )
      .orderBy(desc(schema.ingestion_jobs.created_at)),
    db
      .select()
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.source_document_id, id),
          eq(schema.draft_records.user_id, userId),
        ),
      )
      .orderBy(desc(schema.draft_records.created_at)),
  ]);

  const document = docRows[0];
  if (!document) return null;
  return { document, jobs, drafts };
}

/**
 * URL temporária para o navegador exibir o arquivo original sem que a
 * credencial do storage saia do servidor. Uma hora é o suficiente para ler um
 * boleto e curto o bastante para o link não virar acesso permanente se vazar
 * num histórico.
 */
/**
 * O balde dos arquivos originais — boletos, faturas e comprovantes, exatamente
 * como chegaram. Singleton preguiçoso pelo mesmo motivo de `getDb()`: montar o
 * assinador SigV4 lê variáveis de ambiente e não é de graça, e uma instância
 * serverless atende muitas requisições antes de morrer.
 */
let _originais: ReturnType<ReturnType<typeof createStorageClientFromEnv>["from"]> | null = null;
function originais() {
  _originais ??= createStorageClientFromEnv().from("ingestion-originals");
  return _originais;
}

export async function getDocumentStorageUrl(storagePath: string): Promise<string | null> {
  try {
    return await originais().getSignedUrl(storagePath, 3600);
  } catch {
    return null;
  }
}

export async function reprocessDocument(documentId: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  // Bulk: 0 linhas afetadas é normal (nenhum draft para arquivar), sem throw.
  await db
    .update(schema.draft_records)
    .set({ status: "archived" })
    .where(
      and(
        eq(schema.draft_records.source_document_id, documentId),
        eq(schema.draft_records.user_id, userId),
        inArray(schema.draft_records.status, ["pending_review", "rejected"]),
      ),
    );

  const [latestJob] = await db
    .select({ id: schema.ingestion_jobs.id })
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, documentId),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(1);

  if (latestJob) {
    // Fire-and-forget igual ao código atual: não checa linhas afetadas.
    await db
      .update(schema.ingestion_jobs)
      .set({
        status: "queued",
        error_message: null,
        error_details: null,
        retry_count: 0,
        updated_at: new Date().toISOString(),
      })
      .where(
        and(eq(schema.ingestion_jobs.id, latestJob.id), eq(schema.ingestion_jobs.user_id, userId)),
      );
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
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.ingestion_jobs.user_id, userId)];
  if (filters?.sourceDocumentId)
    conditions.push(eq(schema.ingestion_jobs.source_document_id, filters.sourceDocumentId));
  if (filters?.status) conditions.push(eq(schema.ingestion_jobs.status, filters.status));

  const base = db
    .select()
    .from(schema.ingestion_jobs)
    .where(and(...conditions))
    .orderBy(desc(schema.ingestion_jobs.created_at));
  return filters?.limit ? base.limit(filters.limit) : base;
}

export async function getIngestionJob(id: string): Promise<IngestionJob | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.ingestion_jobs)
    .where(and(eq(schema.ingestion_jobs.id, id), eq(schema.ingestion_jobs.user_id, userId)));
  return row ?? null;
}

export async function getLatestIngestionJobByDocumentId(
  sourceDocumentId: string,
): Promise<IngestionJob | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, sourceDocumentId),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(1);
  return row ?? null;
}

// ══════════════════════════════════════════════════════════════
// Ingestion Runs
// ══════════════════════════════════════════════════════════════

export async function getIngestionRuns(limit = 20): Promise<IngestionRun[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.ingestion_runs)
    .where(eq(schema.ingestion_runs.user_id, userId))
    .orderBy(desc(schema.ingestion_runs.started_at))
    .limit(limit);
}

// ══════════════════════════════════════════════════════════════
// Draft Batches
// ══════════════════════════════════════════════════════════════

export async function getDraftBatches(filters?: {
  status?: string;
  limit?: number;
  source_document_id?: string;
}): Promise<DraftBatch[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.draft_batches.user_id, userId)];
  if (filters?.status) conditions.push(eq(schema.draft_batches.status, filters.status));
  if (filters?.source_document_id)
    conditions.push(eq(schema.draft_batches.source_document_id, filters.source_document_id));

  const base = db
    .select()
    .from(schema.draft_batches)
    .where(and(...conditions))
    .orderBy(desc(schema.draft_batches.created_at));
  return filters?.limit ? base.limit(filters.limit) : base;
}

export async function getDraftBatchByDocumentId(
  sourceDocumentId: string,
): Promise<DraftBatch | null> {
  const batches = await getDraftBatches({ source_document_id: sourceDocumentId, limit: 1 });
  if (batches[0]) return batches[0];

  // Fallback para batches antigos que não tinham source_document_id preenchido.
  const userId = await requireUserId();
  const db = getDb();
  const [latestDraft] = await db
    .select({ batch_id: schema.draft_records.batch_id })
    .from(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.source_document_id, sourceDocumentId),
        eq(schema.draft_records.user_id, userId),
        isNotNull(schema.draft_records.batch_id),
      ),
    )
    .orderBy(desc(schema.draft_records.created_at))
    .limit(1);

  if (!latestDraft?.batch_id) return null;
  return getDraftBatch(latestDraft.batch_id);
}

export async function getDraftBatch(id: string): Promise<DraftBatch | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.draft_batches)
    .where(and(eq(schema.draft_batches.id, id), eq(schema.draft_batches.user_id, userId)));
  return row ?? null;
}

// ══════════════════════════════════════════════════════════════
// Draft Records
// ══════════════════════════════════════════════════════════════

export async function getDraftRecords(filters?: {
  batchId?: string;
  status?: string;
  limit?: number;
}): Promise<DraftRecord[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.draft_records.user_id, userId)];
  if (filters?.batchId) conditions.push(eq(schema.draft_records.batch_id, filters.batchId));
  if (filters?.status) conditions.push(eq(schema.draft_records.status, filters.status));

  const base = db
    .select()
    .from(schema.draft_records)
    .where(and(...conditions))
    .orderBy(desc(schema.draft_records.created_at));
  return filters?.limit ? base.limit(filters.limit) : base;
}

/** Aprova um draft. NÃO cria registro financeiro. */
export async function approveDraftRecord(id: string): Promise<DraftRecord> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.draft_records)
    .set({ status: "approved", approved_at: new Date().toISOString(), approved_by: userId })
    .where(
      and(
        eq(schema.draft_records.id, id),
        eq(schema.draft_records.user_id, userId),
        inArray(schema.draft_records.status, ["pending_review", "corrected"]),
      ),
    )
    .returning();

  if (!row) throw new Error("Draft não encontrado");
  return row;
}

export async function updateDraftData(
  id: string,
  draftData: Record<string, unknown>,
): Promise<DraftRecord> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.draft_records)
    .set({
      draft_data: draftData as Json,
      status: "corrected",
      corrections: { manual_edit: true, edited_at: new Date().toISOString() },
      updated_at: new Date().toISOString(),
    })
    .where(and(eq(schema.draft_records.id, id), eq(schema.draft_records.user_id, userId)))
    .returning();

  if (!row) throw new Error("Draft não encontrado");
  return row;
}

export async function rejectDraftRecord(id: string, reason?: string): Promise<DraftRecord> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.draft_records)
    .set({ status: "rejected", corrections: reason ? { rejection_reason: reason } : null })
    .where(and(eq(schema.draft_records.id, id), eq(schema.draft_records.user_id, userId)))
    .returning();

  if (!row) throw new Error("Draft não encontrado");
  return row;
}

/** Aprova todos os drafts pendentes de um batch. NÃO cria registros financeiros. */
export async function approveDraftBatch(batchId: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  // Bulk: 0 linhas é normal (nenhum draft pendente), sem throw.
  await db
    .update(schema.draft_records)
    .set({ status: "approved", approved_at: new Date().toISOString(), approved_by: userId })
    .where(
      and(
        eq(schema.draft_records.batch_id, batchId),
        eq(schema.draft_records.user_id, userId),
        eq(schema.draft_records.status, "pending_review"),
      ),
    );

  const result = await db
    .update(schema.draft_batches)
    .set({ status: "approved" })
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, userId)))
    .returning({ id: schema.draft_batches.id });

  if (result.length === 0) throw new Error("Batch não encontrado");
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
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.ingestion_logs.user_id, userId)];
  if (filters?.level) conditions.push(eq(schema.ingestion_logs.level, filters.level));
  if (filters?.jobId) conditions.push(eq(schema.ingestion_logs.job_id, filters.jobId));
  if (filters?.runId) conditions.push(eq(schema.ingestion_logs.run_id, filters.runId));

  const where = and(...conditions);
  const [totalRow] = await db.select({ value: count() }).from(schema.ingestion_logs).where(where);
  const total = totalRow?.value ?? 0;
  const orderBy = desc(schema.ingestion_logs.created_at);

  let rows: IngestionLog[];
  if (filters?.offset !== undefined) {
    const pageSize = filters?.limit ?? 50;
    rows = await db
      .select()
      .from(schema.ingestion_logs)
      .where(where)
      .orderBy(orderBy)
      .limit(pageSize)
      .offset(filters.offset);
  } else if (filters?.limit) {
    rows = await db
      .select()
      .from(schema.ingestion_logs)
      .where(where)
      .orderBy(orderBy)
      .limit(filters.limit);
  } else {
    rows = await db.select().from(schema.ingestion_logs).where(where).orderBy(orderBy);
  }

  return { data: rows, count: total };
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
  const userId = await requireUserId();
  const db = getDb();

  const since = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString();

  const [[docs], [pendingJobs], [drafts], [pendingDrafts], [errors]] = await Promise.all([
    db
      .select({ value: count() })
      .from(schema.source_documents)
      .where(eq(schema.source_documents.user_id, userId)),
    db
      .select({ value: count() })
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.user_id, userId),
          eq(schema.ingestion_jobs.status, "pending_review"),
        ),
      ),
    db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(eq(schema.draft_records.user_id, userId)),
    db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.user_id, userId),
          eq(schema.draft_records.status, "pending_review"),
        ),
      ),
    db
      .select({ value: count() })
      .from(schema.ingestion_logs)
      .where(
        and(
          eq(schema.ingestion_logs.user_id, userId),
          eq(schema.ingestion_logs.level, "error"),
          gte(schema.ingestion_logs.created_at, since),
        ),
      ),
  ]);

  return {
    totalDocuments: docs?.value ?? 0,
    pendingReview: pendingJobs?.value ?? 0,
    totalDrafts: drafts?.value ?? 0,
    pendingDrafts: pendingDrafts?.value ?? 0,
    recentErrors: errors?.value ?? 0,
  };
}

// ══════════════════════════════════════════════════════════════
// Upload Manual de Documento (S2-004)
// ══════════════════════════════════════════════════════════════

export async function uploadDocument(formData: FormData): Promise<SourceDocument> {
  const userId = await requireUserId();
  const db = getDb();

  const file = formData.get("file") as File | null;
  if (!file) throw new Error("Arquivo não informado");

  const fileBytes = new Uint8Array(await file.arrayBuffer());
  const contentHash = await computeContentHash(fileBytes);

  const [duplicateFingerprint] = await db
    .select({ source_document_id: schema.document_fingerprints.source_document_id })
    .from(schema.document_fingerprints)
    .where(
      and(
        eq(schema.document_fingerprints.user_id, userId),
        eq(schema.document_fingerprints.content_hash, contentHash),
      ),
    )
    .limit(1);

  if (duplicateFingerprint?.source_document_id) {
    throw new Error(
      `Documento duplicado detectado (mesmo hash). ID original: ${duplicateFingerprint.source_document_id}`,
    );
  }

  const ext = file.name.split(".").pop() ?? "bin";
  const storagePath = `${userId}/manual/${crypto.randomUUID()}.${ext}`;
  await originais().upload(storagePath, file, { contentType: file.type, upsert: false });

  const originKey = `manual::${storagePath}`;
  const [srcDoc] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: originKey,
      filename: file.name,
      mime_type: file.type || ext,
      file_size_bytes: file.size,
      storage_path: storagePath,
      content_hash: contentHash,
      status: "queued",
    })
    .returning();

  if (!srcDoc) throw new Error("Erro ao registrar documento");

  // Apêndice B: 3 inserts sequenciais sem transação — paridade com hoje.
  const [run] = await db
    .insert(schema.ingestion_runs)
    .values({
      user_id: userId,
      source_type: "manual_upload",
      status: "running",
      metadata: { source: "manual_upload", filename: file.name },
    })
    .returning();

  if (!run) throw new Error("Erro ao criar run");

  await db.insert(schema.ingestion_jobs).values({
    run_id: run.id,
    user_id: userId,
    source_document_id: srcDoc.id,
    status: "discovered",
    metadata: { source: "manual_upload" },
  });

  return srcDoc;
}

// ══════════════════════════════════════════════════════════════
// Deletar Documento
// ══════════════════════════════════════════════════════════════

export async function deleteSourceDocument(
  id: string,
  mode: DeleteSourceDocumentMode = "document_and_ingestion",
): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const [doc] = await db
    .select({
      id: schema.source_documents.id,
      storage_path: schema.source_documents.storage_path,
      metadata: schema.source_documents.metadata,
      status: schema.source_documents.status,
    })
    .from(schema.source_documents)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)));

  if (!doc) throw new Error("Documento não encontrado ou sem permissão");

  const now = new Date().toISOString();

  if (doc.storage_path) {
    await originais().remove([doc.storage_path]);
  }

  if (mode === "document_only") {
    const metadata = {
      ...((doc.metadata as Record<string, unknown> | null) ?? {}),
      deletion: {
        mode,
        deleted_at: now,
        deleted_by: userId,
        original_status: doc.status,
        original_storage_path: doc.storage_path,
      },
    };

    const result = await db
      .update(schema.source_documents)
      .set({ status: "deleted", storage_path: null, updated_at: now, metadata })
      .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)))
      .returning({ id: schema.source_documents.id });

    if (result.length === 0) throw new Error("Documento não encontrado ou sem permissão");
    return;
  }

  const [jobs, parsedVersions] = await Promise.all([
    db
      .select({ id: schema.ingestion_jobs.id })
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.source_document_id, id),
          eq(schema.ingestion_jobs.user_id, userId),
        ),
      ),
    db
      .select({ id: schema.parsed_document_versions.id })
      .from(schema.parsed_document_versions)
      .where(
        and(
          eq(schema.parsed_document_versions.source_document_id, id),
          eq(schema.parsed_document_versions.user_id, userId),
        ),
      ),
  ]);

  const jobIds = jobs.map((job) => job.id);
  const parsedVersionIds = parsedVersions.map((version) => version.id);

  await db
    .delete(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.source_document_id, id),
        eq(schema.draft_records.user_id, userId),
      ),
    );
  await db
    .delete(schema.pattern_feedback)
    .where(
      and(
        eq(schema.pattern_feedback.source_document_id, id),
        eq(schema.pattern_feedback.user_id, userId),
      ),
    );

  if (parsedVersionIds.length > 0) {
    await db
      .delete(schema.extraction_results)
      .where(
        and(
          inArray(schema.extraction_results.parsed_version_id, parsedVersionIds),
          eq(schema.extraction_results.user_id, userId),
        ),
      );
  }

  await db
    .delete(schema.parsed_document_versions)
    .where(
      and(
        eq(schema.parsed_document_versions.source_document_id, id),
        eq(schema.parsed_document_versions.user_id, userId),
      ),
    );
  await db
    .delete(schema.draft_batches)
    .where(
      and(
        eq(schema.draft_batches.source_document_id, id),
        eq(schema.draft_batches.user_id, userId),
      ),
    );
  await db
    .delete(schema.document_fingerprints)
    .where(
      and(
        eq(schema.document_fingerprints.source_document_id, id),
        eq(schema.document_fingerprints.user_id, userId),
      ),
    );

  if (jobIds.length > 0) {
    await db
      .delete(schema.ingestion_logs)
      .where(
        and(
          inArray(schema.ingestion_logs.job_id, jobIds),
          eq(schema.ingestion_logs.user_id, userId),
        ),
      );
  }

  await db
    .delete(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, id),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    );

  // Não removemos ingestion_runs aqui (mesma nota do código atual).

  const result = await db
    .delete(schema.source_documents)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)))
    .returning({ id: schema.source_documents.id });

  if (result.length === 0) throw new Error("Documento não encontrado ou sem permissão");
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
  drafts: { count: number; batch_id: string | null };
  issues: string[];
  decision_suggestion: "approve" | "review_required" | "reject";
}

export async function getAutopilotStatus(documentId: string): Promise<AutopilotResult> {
  const userId = await requireUserId();
  const db = getDb();

  const [doc] = await db
    .select({ id: schema.source_documents.id })
    .from(schema.source_documents)
    .where(
      and(eq(schema.source_documents.id, documentId), eq(schema.source_documents.user_id, userId)),
    );
  if (!doc) throw new Error("Documento não encontrado");

  const [job] = await db
    .select({
      id: schema.ingestion_jobs.id,
      status: schema.ingestion_jobs.status,
      metadata: schema.ingestion_jobs.metadata,
      needs_full_ai_review: schema.ingestion_jobs.needs_full_ai_review,
    })
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, documentId),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(1);

  const meta = (job?.metadata ?? {}) as Record<string, unknown>;
  const extractionResultId = meta.extraction_result_id as string | null;

  let extraction: AutopilotResult["extraction"] = null;
  if (extractionResultId) {
    const [er] = await db
      .select({
        supplier_name_raw: schema.extraction_results.supplier_name_raw,
        total_amount: schema.extraction_results.total_amount,
        due_date: schema.extraction_results.due_date,
        competence_date: schema.extraction_results.competence_date,
        financial_intent: schema.extraction_results.financial_intent,
        ai_enrichment_type: schema.extraction_results.ai_enrichment_type,
        confidence_per_field: schema.extraction_results.confidence_per_field,
        reasoning: schema.extraction_results.reasoning,
      })
      .from(schema.extraction_results)
      .where(
        and(
          eq(schema.extraction_results.id, extractionResultId),
          eq(schema.extraction_results.user_id, userId),
        ),
      );

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

  const [draftBatch] = await db
    .select({ id: schema.draft_batches.id })
    .from(schema.draft_batches)
    .where(
      and(
        eq(schema.draft_batches.source_document_id, documentId),
        eq(schema.draft_batches.user_id, userId),
      ),
    )
    .limit(1);

  const batchId = draftBatch?.id ?? null;

  let draftCount = 0;
  if (batchId) {
    const [draftCountRow] = await db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(
        and(eq(schema.draft_records.batch_id, batchId), eq(schema.draft_records.user_id, userId)),
      );
    draftCount = draftCountRow?.value ?? 0;
  }

  const issues: string[] = [];
  if (!extraction?.total_amount) issues.push("Valor total não identificado");
  if (!extraction?.due_date) issues.push("Data de vencimento não identificada");
  if (!extraction?.supplier_name) issues.push("Fornecedor não identificado");
  if (job?.needs_full_ai_review) issues.push("Requer análise de IA completa (visão)");
  if (draftCount === 0) issues.push("Nenhum draft gerado para revisão");

  const needsFullReview = job?.needs_full_ai_review ?? false;
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

export async function triggerAiFullAnalysis(documentId: string): Promise<{ jobId: string }> {
  const userId = await requireUserId();
  const db = getDb();

  const [doc] = await db
    .select({ id: schema.source_documents.id })
    .from(schema.source_documents)
    .where(
      and(eq(schema.source_documents.id, documentId), eq(schema.source_documents.user_id, userId)),
    );
  if (!doc) throw new Error("Documento não encontrado");

  const [run] = await db
    .insert(schema.ingestion_runs)
    .values({
      user_id: userId,
      source_type: "manual_upload",
      status: "running",
      metadata: { triggered_by: "ai_full_request", document_id: documentId },
    })
    .returning({ id: schema.ingestion_runs.id });

  if (!run) throw new Error("Falha ao criar run para análise full");

  const [job] = await db
    .insert(schema.ingestion_jobs)
    .values({
      run_id: run.id,
      user_id: userId,
      source_document_id: documentId,
      status: "queued",
      metadata: { ai_mode: "full", force_reprocess: true, triggered_by: "user_request" },
    })
    .returning({ id: schema.ingestion_jobs.id });

  if (!job) throw new Error("Falha ao criar job de análise full");

  // Fire-and-forget igual ao código atual: não checa linhas afetadas.
  await db
    .update(schema.ingestion_runs)
    .set({ status: "completed" })
    .where(and(eq(schema.ingestion_runs.id, run.id), eq(schema.ingestion_runs.user_id, userId)));

  return { jobId: job.id };
}
