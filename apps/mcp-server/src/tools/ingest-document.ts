/**
 * MCP tool: ingest_document
 * Dispara ingestão (ou reprocessamento) de um documento com modo de IA configurável.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionRunStatus, IngestionJobStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";

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
  supabase: SupabaseClient,
  userId: string,
  documentId: string,
  aiMode: AiMode = "auto",
): Promise<IngestDocumentResult> {
  // Verificar que documento existe e pertence ao usuário
  const { data: doc, error: docError } = await supabase
    .from("source_documents")
    .select("id, filename, status")
    .eq("id", documentId)
    .eq("user_id", userId)
    .single();

  if (docError || !doc) {
    return {
      success: false,
      jobId: null,
      runId: null,
      documentId,
      aiMode,
      message: `Documento não encontrado ou sem permissão: ${docError?.message ?? "id inválido"}`,
    };
  }

  // Criar run
  const { data: run, error: runError } = await supabase
    .from("ingestion_runs")
    .insert({
      user_id: userId,
      source_type: SourceDocumentOrigin.MANUAL_UPLOAD,
      status: IngestionRunStatus.RUNNING,
      metadata: {
        triggered_by: "mcp",
        document_id: documentId,
        ai_mode: aiMode,
      },
    })
    .select("id")
    .single();

  if (runError || !run) {
    return {
      success: false,
      jobId: null,
      runId: null,
      documentId,
      aiMode,
      message: `Falha ao criar ingestion run: ${runError?.message ?? "erro desconhecido"}`,
    };
  }

  // Criar job de ingestão
  const { data: job, error: jobError } = await supabase
    .from("ingestion_jobs")
    .insert({
      run_id: run.id,
      user_id: userId,
      source_document_id: documentId,
      status: IngestionJobStatus.QUEUED,
      metadata: {
        ai_mode: aiMode,
        triggered_by: "mcp",
        filename: doc.filename,
      },
    })
    .select("id")
    .single();

  if (jobError || !job) {
    // Limpar run órfã
    await supabase
      .from("ingestion_runs")
      .update({ status: IngestionRunStatus.FAILED })
      .eq("id", run.id);
    return {
      success: false,
      jobId: null,
      runId: run.id,
      documentId,
      aiMode,
      message: `Falha ao criar ingestion job: ${jobError?.message ?? "erro desconhecido"}`,
    };
  }

  // Marcar run como enfileirada (o worker processa de forma assíncrona)
  await supabase
    .from("ingestion_runs")
    .update({ status: IngestionRunStatus.RUNNING })
    .eq("id", run.id);

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
    jobId: job.id,
    runId: run.id,
    documentId,
    aiMode,
    message: `Job de ingestão criado com sucesso para documento "${doc.filename}".${aiModeNote}`,
  };
}
