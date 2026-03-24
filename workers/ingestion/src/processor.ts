/**
 * @sbf/worker-ingestion — Job processor
 * Processa um job individual pela máquina de estados.
 * Fluxo completo: discovered → downloaded → hashed → queued → parsing → parsed.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionJobStatus, IngestionLogLevel } from "@sbf/ingestion-types";
import { computeContentHash, computeCanonicalFingerprint, checkIdempotency } from "@sbf/operations";
import { isValidTransition } from "./state-machine";
import { writeLog, type LogContext } from "./logger";
import { parseDocument } from "./parsers/parse-orchestrator";
import { generateDrafts } from "./drafts/draft-generator";

interface JobRow {
  id: string;
  run_id: string;
  user_id: string;
  source_document_id: string | null;
  status: string;
  retry_count: number;
  max_retries: number;
  metadata: Record<string, unknown>;
}

/**
 * Atualiza status de um job com validação de transição.
 */
async function transitionJob(
  supabase: SupabaseClient,
  jobId: string,
  from: string,
  to: string,
  extra?: Record<string, unknown>,
): Promise<boolean> {
  if (!isValidTransition(from as IngestionJobStatus, to as IngestionJobStatus)) {
    console.error(`Invalid transition: ${from} → ${to}`);
    return false;
  }

  const { error } = await supabase
    .from("ingestion_jobs")
    .update({ status: to, updated_at: new Date().toISOString(), ...extra })
    .eq("id", jobId)
    .eq("status", from); // Optimistic locking

  return !error;
}

/**
 * Marca job como failed com detalhes do erro.
 */
async function failJob(
  supabase: SupabaseClient,
  ctx: LogContext,
  jobId: string,
  currentStatus: string,
  error: unknown,
): Promise<void> {
  const msg = error instanceof Error ? error.message : String(error);
  await writeLog(supabase, ctx, IngestionLogLevel.ERROR, `Job failed: ${msg}`, {
    error: msg,
    previous_status: currentStatus,
  });

  await supabase
    .from("ingestion_jobs")
    .update({
      status: IngestionJobStatus.FAILED,
      error_message: msg.slice(0, 1000),
      error_details: { previous_status: currentStatus },
      updated_at: new Date().toISOString(),
    })
    .eq("id", jobId);
}

/**
 * Step: download — busca o arquivo do Storage e mantém em memória.
 * No MVP, para local_file, o arquivo já está no Storage (upload pelo scanner).
 */
async function stepDownload(
  supabase: SupabaseClient,
  ctx: LogContext,
  job: JobRow,
): Promise<{ data: ArrayBuffer; mimeType: string | null } | null> {
  if (!job.source_document_id) {
    throw new Error("Job without source_document_id cannot download");
  }

  const { data: doc } = await supabase
    .from("source_documents")
    .select("storage_path, mime_type, filename")
    .eq("id", job.source_document_id)
    .single();

  if (!doc?.storage_path) {
    throw new Error(`Source document ${job.source_document_id} has no storage_path`);
  }

  const { data: blob, error } = await supabase.storage
    .from("ingestion-originals")
    .download(doc.storage_path);

  if (error || !blob) {
    throw new Error(`Failed to download ${doc.storage_path}: ${error?.message}`);
  }

  const arrayBuffer = await blob.arrayBuffer();
  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `Downloaded ${doc.filename} (${arrayBuffer.byteLength} bytes)`,
  );
  return { data: arrayBuffer, mimeType: doc.mime_type };
}

/**
 * Step: hash — calcula content_hash e canonical_fingerprint, executa dedup.
 */
async function stepHash(
  supabase: SupabaseClient,
  ctx: LogContext,
  job: JobRow,
  fileData: ArrayBuffer,
): Promise<{ contentHash: string; shouldProceed: boolean }> {
  const contentHash = await computeContentHash(new Uint8Array(fileData));
  const canonicalFingerprint = await computeCanonicalFingerprint(
    new TextDecoder("utf-8", { fatal: false }).decode(new Uint8Array(fileData)),
  );

  // Buscar duplicatas existentes
  const { data: existing } = await supabase
    .from("document_fingerprints")
    .select("id, source_document_id, content_hash, canonical_fingerprint")
    .eq("user_id", ctx.userId)
    .or(`content_hash.eq.${contentHash},canonical_fingerprint.eq.${canonicalFingerprint}`);

  const forceReprocess = !!(job.metadata as Record<string, unknown>)?.force_reprocess;

  // Salvar fingerprints
  if (job.source_document_id) {
    await supabase.from("document_fingerprints").upsert(
      {
        source_document_id: job.source_document_id,
        user_id: ctx.userId,
        content_hash: contentHash,
        canonical_fingerprint: canonicalFingerprint,
        hash_algorithm: "sha256",
      },
      { onConflict: "user_id,content_hash" },
    );
  }

  if (!existing || existing.length === 0) {
    await writeLog(supabase, ctx, IngestionLogLevel.INFO, "No duplicates found — proceeding");
    return { contentHash, shouldProceed: true };
  }

  // force_reprocess: pular verificação de idempotência
  if (forceReprocess) {
    await writeLog(supabase, ctx, IngestionLogLevel.INFO, "force_reprocess — skipping dedup check");
    return { contentHash, shouldProceed: true };
  }

  // Verificar idempotência
  const matchByHash = existing.find((e) => e.content_hash === contentHash);
  const matchByFP = existing.find((e) => e.canonical_fingerprint === canonicalFingerprint);

  const result = checkIdempotency(
    {
      contentHash,
      canonicalFingerprint,
      originKey: "", // origin key já verificado no scanner
      forceReprocess: false,
    },
    {
      byContentHash: matchByHash ? { sourceDocumentId: matchByHash.source_document_id } : undefined,
      byCanonicalFingerprint: matchByFP
        ? { sourceDocumentId: matchByFP.source_document_id }
        : undefined,
      byOriginKey: undefined,
    },
  );

  await writeLog(supabase, ctx, IngestionLogLevel.INFO, `Idempotency check: ${result.action}`, {
    action: result.action,
    reason: result.reason,
  });

  return {
    contentHash,
    shouldProceed: result.action === "proceed" || result.action === "update_origin",
  };
}

/**
 * Step: parse — extrai texto e dados estruturados do documento.
 * Transição: queued → parsing → parsed.
 */
async function stepParse(supabase: SupabaseClient, ctx: LogContext, job: JobRow): Promise<void> {
  if (!job.source_document_id) {
    throw new Error("Job without source_document_id cannot parse");
  }

  // Transition queued → parsing
  const ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.QUEUED,
    IngestionJobStatus.PARSING,
  );
  if (!ok) return;
  job.status = IngestionJobStatus.PARSING;

  // Fetch source document metadata
  const { data: doc } = await supabase
    .from("source_documents")
    .select("storage_path, mime_type, filename")
    .eq("id", job.source_document_id)
    .single();

  if (!doc?.storage_path) {
    throw new Error(`Source document ${job.source_document_id} has no storage_path`);
  }

  // Re-download file from storage
  const { data: blob, error: dlError } = await supabase.storage
    .from("ingestion-originals")
    .download(doc.storage_path);

  if (dlError || !blob) {
    throw new Error(`Failed to download for parsing: ${dlError?.message}`);
  }

  const fileData = await blob.arrayBuffer();
  const mimeType =
    ((job.metadata as Record<string, unknown>)?.mime_type as string) ??
    doc.mime_type ??
    "application/octet-stream";

  await writeLog(supabase, ctx, IngestionLogLevel.INFO, `Parsing ${doc.filename} (${mimeType})`);

  const result = await parseDocument(
    {
      supabase,
      ctx,
      userId: ctx.userId,
      sourceDocumentId: job.source_document_id,
      mimeType,
    },
    fileData,
  );

  // Transition parsing → parsed
  await transitionJob(supabase, job.id, IngestionJobStatus.PARSING, IngestionJobStatus.PARSED, {
    metadata: {
      ...(job.metadata ?? {}),
      parsed_version_id: result.parsedVersionId,
      extraction_result_id: result.extractionResultId,
      parser_type: result.parserType,
      confidence: result.confidence,
    },
  });

  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `Parsed with ${result.parserType} (confidence: ${result.confidence})`,
    { parsedVersionId: result.parsedVersionId, extractionResultId: result.extractionResultId },
  );

  job.status = IngestionJobStatus.PARSED;
  job.metadata = {
    ...(job.metadata ?? {}),
    parsed_version_id: result.parsedVersionId,
    extraction_result_id: result.extractionResultId,
    parser_type: result.parserType,
    confidence: result.confidence,
  };
}

/**
 * Step: draft — classifica, reconcilia e gera drafts.
 * Transições: parsed → classified → reconciled → drafted → pending_review.
 * No MVP, classify e reconcile são passthrough automáticos.
 */
async function stepDraft(supabase: SupabaseClient, ctx: LogContext, job: JobRow): Promise<void> {
  if (!job.source_document_id) {
    throw new Error("Job without source_document_id cannot generate drafts");
  }

  const meta = job.metadata as Record<string, unknown>;

  // parsed → classified (auto: determinado pelo tipo de extração)
  let ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.PARSED,
    IngestionJobStatus.CLASSIFIED,
    {
      metadata: { ...meta, classified_at: new Date().toISOString() },
    },
  );
  if (!ok) return;
  job.status = IngestionJobStatus.CLASSIFIED;

  // classified → reconciled (MVP: passthrough, sem conflitos)
  ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.CLASSIFIED,
    IngestionJobStatus.RECONCILED,
    {
      metadata: { ...meta, reconciled_at: new Date().toISOString(), reconcile_conflicts: 0 },
    },
  );
  if (!ok) return;
  job.status = IngestionJobStatus.RECONCILED;

  // reconciled → drafted (gera draft_batch + draft_records)
  const result = await generateDrafts({
    supabase,
    ctx,
    userId: ctx.userId,
    sourceDocumentId: job.source_document_id,
    extractionResultId: meta.extraction_result_id as string | null,
    parsedVersionId: meta.parsed_version_id as string,
    parserType: meta.parser_type as string,
    confidence: (meta.confidence as number) ?? 0.3,
  });

  ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.RECONCILED,
    IngestionJobStatus.DRAFTED,
    {
      metadata: {
        ...meta,
        batch_id: result.batchId,
        draft_count: result.drafts.length,
        draft_ids: result.drafts.map((d) => d.id),
      },
    },
  );
  if (!ok) return;
  job.status = IngestionJobStatus.DRAFTED;

  // drafted → pending_review
  ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.DRAFTED,
    IngestionJobStatus.PENDING_REVIEW,
  );
  if (!ok) return;
  job.status = IngestionJobStatus.PENDING_REVIEW;

  await writeLog(
    supabase,
    ctx,
    IngestionLogLevel.INFO,
    `Drafts gerados e aguardando revisão (batch: ${result.batchId}, ${result.drafts.length} drafts)`,
  );
}

/**
 * Processa um único job pela state machine.
 * Fluxo completo: discovered → downloaded → hashed → queued → parsing → parsed
 *                 → classified → reconciled → drafted → pending_review.
 */
export async function processJob(supabase: SupabaseClient, job: JobRow): Promise<void> {
  const ctx: LogContext = {
    userId: job.user_id,
    runId: job.run_id,
    jobId: job.id,
  };

  try {
    // Step 1: discovered → downloaded
    if (job.status === IngestionJobStatus.DISCOVERED) {
      const file = await stepDownload(supabase, ctx, job);
      if (!file) return;

      const ok = await transitionJob(
        supabase,
        job.id,
        IngestionJobStatus.DISCOVERED,
        IngestionJobStatus.DOWNLOADED,
        {
          metadata: {
            ...(job.metadata ?? {}),
            file_size: file.data.byteLength,
            mime_type: file.mimeType,
          },
        },
      );
      if (!ok) return;
      job.status = IngestionJobStatus.DOWNLOADED;
      job.metadata = {
        ...(job.metadata ?? {}),
        file_size: file.data.byteLength,
        mime_type: file.mimeType,
      };

      // Step 2: downloaded → hashed
      const hashResult = await stepHash(supabase, ctx, job, file.data);

      const nextStatus = hashResult.shouldProceed
        ? IngestionJobStatus.HASHED
        : IngestionJobStatus.FAILED;
      const extraData = hashResult.shouldProceed
        ? { metadata: { ...(job.metadata ?? {}), content_hash: hashResult.contentHash } }
        : { error_message: "Duplicate document detected" };

      await transitionJob(supabase, job.id, IngestionJobStatus.DOWNLOADED, nextStatus, extraData);

      if (!hashResult.shouldProceed) return;
      job.status = IngestionJobStatus.HASHED;
      job.metadata = { ...(job.metadata ?? {}), content_hash: hashResult.contentHash };

      // Continue to queued
      await transitionJob(supabase, job.id, IngestionJobStatus.HASHED, IngestionJobStatus.QUEUED);
      job.status = IngestionJobStatus.QUEUED;
    }

    // Step 3: hashed → queued (legacy: para jobs que pararam em HASHED)
    if (job.status === IngestionJobStatus.HASHED) {
      await transitionJob(supabase, job.id, IngestionJobStatus.HASHED, IngestionJobStatus.QUEUED);
      job.status = IngestionJobStatus.QUEUED;
    }

    // Step 4: queued → parsing → parsed
    if (job.status === IngestionJobStatus.QUEUED) {
      await stepParse(supabase, ctx, job);
    }

    // Step 5: parsed → classified → reconciled → drafted → pending_review
    if (job.status === IngestionJobStatus.PARSED) {
      await stepDraft(supabase, ctx, job);
    }
  } catch (err) {
    await failJob(supabase, ctx, job.id, job.status, err);
  }
}
