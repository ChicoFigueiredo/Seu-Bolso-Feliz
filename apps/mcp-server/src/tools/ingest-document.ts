/**
 * MCP tool: ingest_document
 * Dispara ingestão (ou reprocessamento) de um documento com modo de IA configurável.
 */
import { and, eq } from "drizzle-orm";
import { enqueueJob, schema, type Database } from "@sbf/db";
import { IngestionRunStatus, IngestionJobStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { resolveRequires } from "../job-requires.js";

export type AiMode = "auto" | "lite" | "full" | "skip";

export interface IngestDocumentResult {
  success: boolean;
  jobId: string | null;
  runId: string | null;
  documentId: string;
  aiMode: AiMode;
  message: string;
}

export async function ingestDocument(
  db: Database,
  userId: string,
  documentId: string,
  aiMode: AiMode = "auto",
): Promise<IngestDocumentResult> {
  // Verificar que documento existe e pertence ao usuário
  const [doc] = await db
    .select({ id: schema.source_documents.id, filename: schema.source_documents.filename })
    .from(schema.source_documents)
    .where(
      and(eq(schema.source_documents.id, documentId), eq(schema.source_documents.user_id, userId)),
    );

  if (!doc) {
    return {
      success: false,
      jobId: null,
      runId: null,
      documentId,
      aiMode,
      message: "Documento não encontrado ou sem permissão",
    };
  }

  // Criar run
  let runId: string;
  try {
    const [run] = await db
      .insert(schema.ingestion_runs)
      .values({
        user_id: userId,
        source_type: SourceDocumentOrigin.MANUAL_UPLOAD,
        status: IngestionRunStatus.RUNNING,
        metadata: {
          triggered_by: "mcp",
          document_id: documentId,
          ai_mode: aiMode,
        },
      })
      .returning({ id: schema.ingestion_runs.id });
    if (!run) throw new Error("insert não devolveu linha");
    runId = run.id;
  } catch (err) {
    return {
      success: false,
      jobId: null,
      runId: null,
      documentId,
      aiMode,
      message: `Falha ao criar ingestion run: ${err instanceof Error ? err.message : String(err)}`,
    };
  }

  // Criar job de ingestão via enqueueJob — não insert direto — para nascer
  // com available_at/locked_by corretos e disparar o NOTIFY que acorda o
  // worker (ver packages/db/src/queue.ts).
  let jobId: string;
  try {
    const job = await enqueueJob({
      db,
      runId,
      userId,
      sourceDocumentId: documentId,
      status: IngestionJobStatus.QUEUED,
      requires: resolveRequires(doc.filename),
      metadata: {
        ai_mode: aiMode,
        triggered_by: "mcp",
        filename: doc.filename,
      },
    });
    jobId = job.id;
  } catch (err) {
    // Limpar run órfã
    await db
      .update(schema.ingestion_runs)
      .set({ status: IngestionRunStatus.FAILED })
      .where(eq(schema.ingestion_runs.id, runId));
    return {
      success: false,
      jobId: null,
      runId,
      documentId,
      aiMode,
      message: `Falha ao criar ingestion job: ${err instanceof Error ? err.message : String(err)}`,
    };
  }

  // Marcar run como enfileirada (o worker processa de forma assíncrona)
  await db
    .update(schema.ingestion_runs)
    .set({ status: IngestionRunStatus.RUNNING })
    .where(eq(schema.ingestion_runs.id, runId));

  const aiModeNote =
    aiMode === "full"
      ? " Análise com visão (IA full) será executada."
      : aiMode === "lite"
        ? " Enriquecimento via IA lite será forçado."
        : aiMode === "skip"
          ? " IA será ignorada — apenas parsing determinístico."
          : " Modo automático: IA lite ativada se confiança < 60% ou campos críticos ausentes.";

  return {
    success: true,
    jobId,
    runId,
    documentId,
    aiMode,
    message: `Job de ingestão criado com sucesso para documento "${doc.filename}".${aiModeNote}`,
  };
}
