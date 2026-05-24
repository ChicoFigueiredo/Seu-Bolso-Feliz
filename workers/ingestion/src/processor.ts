/**
 * @sbf/worker-ingestion — Job processor
 * Processa um job individual pela máquina de estados.
 * Fluxo: discovered → downloaded → hashed → queued → parsing → parsed
 *        → ai_lite_enriching (condicional) → classified → reconciled → drafted → pending_review.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionJobStatus, IngestionLogLevel } from "@sbf/ingestion-types";
import { computeContentHash, computeCanonicalFingerprint, checkIdempotency } from "@sbf/operations";
import { isValidTransition } from "./state-machine";
import { writeLog, type LogContext } from "./logger";
import { parseDocument } from "./parsers/parse-orchestrator";
import { generateDrafts } from "./drafts/draft-generator";
import { findReconciliationCandidates, isDuplicateRisk } from "./reconciliation/reconciliation";
import {
  enrichWithAiLite,
  computeCriticalCoverage,
  shouldActivateAiLite,
  type AiLiteInput,
} from "./parsers/ai-lite-enricher";
import { enrichWithAiFull } from "./parsers/ai-full-enricher";

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
  sourceDocumentId: string | null,
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

  if (sourceDocumentId) {
    await supabase
      .from("source_documents")
      .update({
        status: "failed",
        updated_at: new Date().toISOString(),
      })
      .eq("id", sourceDocumentId);
  }
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

    await supabase
      .from("source_documents")
      .update({
        content_hash: contentHash,
        updated_at: new Date().toISOString(),
      })
      .eq("id", job.source_document_id);
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

  if (job.source_document_id) {
    await supabase
      .from("source_documents")
      .update({
        status: "processing",
        updated_at: new Date().toISOString(),
      })
      .eq("id", job.source_document_id);
  }

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
 * Step: AI lite enrichment — enriquece extraction_result via IA barata quando
 * o parser determinístico ficou abaixo do limiar ou deixou campos críticos vazios.
 * Transição: parsed → ai_lite_enriching → classified.
 */
async function stepAiLiteEnrichment(
  supabase: SupabaseClient,
  ctx: LogContext,
  job: JobRow,
): Promise<void> {
  const meta = job.metadata as Record<string, unknown>;
  let extractionResultId = meta.extraction_result_id as string | null;
  const currentConfidence = (meta.confidence as number) ?? 0;
  const aiMode = String(meta.ai_mode ?? "auto");

  if (aiMode === "skip") {
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      "IA desabilitada por ai_mode=skip — seguindo com camada determinística",
    );
    return;
  }

  if (!job.source_document_id) {
    throw new Error("Job sem source_document_id não pode executar enriquecimento por IA");
  }

  const { data: sourceDoc } = await supabase
    .from("source_documents")
    .select("storage_path, mime_type")
    .eq("id", job.source_document_id)
    .single();

  if (!extractionResultId) {
    // Sem extraction_result: documento sem dados estruturados. Escalar direto para IA full.
    if (!sourceDoc?.storage_path) {
      await writeLog(
        supabase,
        ctx,
        IngestionLogLevel.WARN,
        "IA full não pôde iniciar: source_document sem storage_path",
      );
      return;
    }

    const { data: createdEr } = await supabase
      .from("extraction_results")
      .insert({
        parsed_version_id: meta.parsed_version_id as string,
        user_id: ctx.userId,
        metadata: { created_by: "ai_full_fallback" },
      })
      .select("id")
      .single();

    extractionResultId = createdEr?.id ?? null;
    meta.extraction_result_id = extractionResultId;

    if (!extractionResultId) {
      await writeLog(
        supabase,
        ctx,
        IngestionLogLevel.WARN,
        "IA full não pôde iniciar: falha ao criar extraction_result fallback",
      );
      return;
    }

    const ok = await transitionJob(
      supabase,
      job.id,
      IngestionJobStatus.PARSED,
      IngestionJobStatus.AI_LITE_ENRICHING,
    );
    if (!ok) return;
    job.status = IngestionJobStatus.AI_LITE_ENRICHING;

    const { data: pv } = await supabase
      .from("parsed_document_versions")
      .select("raw_text")
      .eq("id", meta.parsed_version_id as string)
      .single();

    const full = await enrichWithAiFull(supabase, ctx, {
      extractionResultId,
      sourceDocumentId: job.source_document_id,
      storagePath: sourceDoc.storage_path,
      mimeType: sourceDoc.mime_type ?? "application/octet-stream",
      rawText: pv?.raw_text ?? "",
    });

    meta.confidence = full.overallConfidence;
    meta.ai_enriched = full.enriched;
    meta.ai_enrichment_type = "full";
    meta.needs_full_ai_review = false;
    job.metadata = meta;

    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      "IA full executada por ausência de extraction_result na camada determinística",
    );
    return;
  }

  // Carregar extraction_result para verificar campos
  const { data: er } = await supabase
    .from("extraction_results")
    .select("total_amount, due_date, supplier_name_raw, competence_date, document_number, metadata")
    .eq("id", extractionResultId)
    .single();

  if (!er) return;

  // barcode_digitable_line e supplier_cnpj são salvos no metadata (sem coluna dedicada)
  const erMeta = (er.metadata as Record<string, unknown>) ?? {};
  const erDeterministic = (erMeta.deterministic_extras as Record<string, unknown>) ?? {};

  const extractedFields: AiLiteInput["extractedFields"] = {
    total_amount: er.total_amount ?? null,
    due_date: er.due_date ?? null,
    supplier_name_raw: er.supplier_name_raw ?? null,
    competence_date: er.competence_date ?? null,
    document_number: er.document_number ?? null,
    supplier_cnpj: (erDeterministic.supplier_cnpj as string | null) ?? null,
    barcode_digitable_line: (erDeterministic.barcode_digitable_line as string | null) ?? null,
  };

  const criticalCoverage = computeCriticalCoverage(extractedFields);

  // Modo full força escalonamento direto para a camada 3.
  const shouldForceFull = aiMode === "full";
  // Em modo auto/lite, ativa camada lite se confiança fraca, campo crítico faltante ou suspeito.
  const shouldRunLite =
    !shouldForceFull && shouldActivateAiLite(currentConfidence, extractedFields);

  // Força full quando a cobertura crítica é insuficiente, mesmo com confiança alta.
  const shouldEscalateByCoverage = criticalCoverage < 1;

  if (!shouldForceFull && !shouldRunLite && !shouldEscalateByCoverage) {
    await writeLog(
      supabase,
      ctx,
      IngestionLogLevel.INFO,
      `IA: confiança ${(currentConfidence * 100).toFixed(0)}% e cobertura crítica 100% — sem escalonamento`,
    );
    return;
  }

  // Transição parsed → ai_lite_enriching
  const ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.PARSED,
    IngestionJobStatus.AI_LITE_ENRICHING,
  );
  if (!ok) return;
  job.status = IngestionJobStatus.AI_LITE_ENRICHING;

  // Buscar texto bruto para o prompt
  const { data: pv } = await supabase
    .from("parsed_document_versions")
    .select("raw_text")
    .eq("id", meta.parsed_version_id as string)
    .single();

  const rawText = pv?.raw_text ?? "";

  let updatedConf = currentConfidence;
  let needsFullReview = shouldEscalateByCoverage || shouldForceFull;
  let enrichmentType: "lite" | "full" | null = null;

  if (shouldRunLite) {
    const result = await enrichWithAiLite(supabase, ctx, {
      extractionResultId,
      rawText,
      currentConfidence,
      extractedFields,
    });

    updatedConf = result.enriched ? result.overallConfidence : currentConfidence;
    needsFullReview = needsFullReview || result.needsFullReview;
    enrichmentType = "lite";
  }

  if (needsFullReview && sourceDoc?.storage_path) {
    const full = await enrichWithAiFull(supabase, ctx, {
      extractionResultId,
      sourceDocumentId: job.source_document_id,
      storagePath: sourceDoc.storage_path,
      mimeType: sourceDoc.mime_type ?? "application/octet-stream",
      rawText,
    });

    updatedConf = full.enriched ? full.overallConfidence : updatedConf;
    needsFullReview = !full.enriched;
    enrichmentType = full.enriched ? "full" : enrichmentType;
  }

  if (needsFullReview) {
    await supabase
      .from("ingestion_jobs")
      .update({ needs_full_ai_review: true, updated_at: new Date().toISOString() })
      .eq("id", job.id);
  }

  // Transição ai_lite_enriching → classified via stepDraft (chamado após)
  meta.confidence = updatedConf;
  meta.ai_enriched = enrichmentType !== null;
  meta.ai_enrichment_type = enrichmentType;
  meta.needs_full_ai_review = needsFullReview;
  job.metadata = meta;
}

/**
 * Step: draft — classifica, reconcilia e gera drafts.
 * Transições: parsed|ai_lite_enriching → classified → reconciled → drafted → pending_review.
 * No MVP, classify e reconcile são passthrough automáticos.
 */
async function stepDraft(supabase: SupabaseClient, ctx: LogContext, job: JobRow): Promise<void> {
  if (!job.source_document_id) {
    throw new Error("Job without source_document_id cannot generate drafts");
  }

  const meta = job.metadata as Record<string, unknown>;

  // parsed|ai_lite_enriching → classified
  const fromStatus = job.status as IngestionJobStatus;
  let ok = await transitionJob(supabase, job.id, fromStatus, IngestionJobStatus.CLASSIFIED, {
    metadata: { ...meta, classified_at: new Date().toISOString() },
  });
  if (!ok) return;
  job.status = IngestionJobStatus.CLASSIFIED;

  // classified → reconciled (engine determinística de reconciliação)
  await writeLog(supabase, ctx, IngestionLogLevel.INFO, "Executando reconciliação...");

  // Buscar draft_data do primeiro draft gerado para rodar reconciliação
  // (neste ponto os drafts ainda não existem — reconciliação ocorre antes de gerar)
  // Carregamos extraction_result para obter os dados necessários
  let reconciliationConflicts = 0;
  if (meta.extraction_result_id) {
    const { data: er } = await supabase
      .from("extraction_results")
      .select("*")
      .eq("id", meta.extraction_result_id as string)
      .single();

    if (er) {
      const draftDataForReconciliation: Record<string, unknown> = {
        amount: er.total_amount,
        due_date: er.due_date,
        competence_date: er.competence_date,
        supplier_id: er.supplier_id,
        supplier_name: er.supplier_name_raw,
      };

      const reconcResult = await findReconciliationCandidates(
        supabase,
        ctx.userId,
        draftDataForReconciliation,
        job.source_document_id,
      );

      if (isDuplicateRisk(reconcResult)) {
        reconciliationConflicts = 1;
        await writeLog(
          supabase,
          ctx,
          IngestionLogLevel.WARN,
          `Risco de duplicata detectado: ${reconcResult.status} (${reconcResult.candidates.length} candidatos)`,
        );
      } else if (reconcResult.status !== "no_match") {
        await writeLog(
          supabase,
          ctx,
          IngestionLogLevel.INFO,
          `Reconciliação: ${reconcResult.status} (${reconcResult.candidates.length} candidatos)`,
        );
      }

      // Salvar resultado da reconciliação em metadados do job
      meta.reconciliation_status = reconcResult.status;
      meta.reconciliation_candidates = reconcResult.candidates;
    }
  }

  ok = await transitionJob(
    supabase,
    job.id,
    IngestionJobStatus.CLASSIFIED,
    IngestionJobStatus.RECONCILED,
    {
      metadata: {
        ...meta,
        reconciled_at: new Date().toISOString(),
        reconcile_conflicts: reconciliationConflicts,
      },
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
        draft_types: result.drafts.map((d) => d.draftType),
        draft_ids: result.drafts.map((d) => d.id),
      },
    },
  );
  if (!ok) return;
  job.status = IngestionJobStatus.DRAFTED;

  // Persistir status de reconciliação nos draft_records gerados
  if (meta.reconciliation_status && result.drafts.length > 0) {
    const reconcStatus = meta.reconciliation_status as string;
    const reconcCandidates = meta.reconciliation_candidates ?? [];
    const draftIds = result.drafts.map((d) => d.id);

    await supabase
      .from("draft_records")
      .update({
        reconciliation_status: reconcStatus,
        reconciliation_candidates: reconcCandidates,
        reconciled_at: new Date().toISOString(),
      })
      .in("id", draftIds);
  }

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

  if (job.source_document_id) {
    await supabase
      .from("source_documents")
      .update({
        status: "pending_review",
        updated_at: new Date().toISOString(),
      })
      .eq("id", job.source_document_id);
  }
}

/**
 * Processa um único job pela state machine.
 * Fluxo: discovered → downloaded → hashed → queued → parsing → parsed
 *        → ai_lite_enriching (condicional) → classified → reconciled → drafted → pending_review.
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

    // Step 5: parsed → ai_lite_enriching (condicional) → classified
    if (job.status === IngestionJobStatus.PARSED) {
      await stepAiLiteEnrichment(supabase, ctx, job);
    }

    // Step 6: parsed|ai_lite_enriching → classified → reconciled → drafted → pending_review
    if (
      job.status === IngestionJobStatus.PARSED ||
      job.status === IngestionJobStatus.AI_LITE_ENRICHING
    ) {
      await stepDraft(supabase, ctx, job);
    }
  } catch (err) {
    await failJob(supabase, ctx, job.id, job.source_document_id, job.status, err);
  }
}
