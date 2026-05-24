/**
 * @sbf/mcp-server — Tool: reprocess_document
 * Reprocessa um documento específico com force_reprocess.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionJobStatus, IngestionRunStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";

export async function reprocessDocument(
  supabase: SupabaseClient,
  userId: string,
  documentId: string,
): Promise<{ jobId: string; message: string }> {
  // Verificar que o documento pertence ao usuário
  const { data: doc, error } = await supabase
    .from("source_documents")
    .select("id, filename, user_id")
    .eq("id", documentId)
    .eq("user_id", userId)
    .single();

  if (error || !doc) {
    throw new Error(`Document not found or not owned by user: ${documentId}`);
  }

  // Criar um novo run para o reprocessamento
  const { data: run } = await supabase
    .from("ingestion_runs")
    .insert({
      user_id: userId,
      source_type: SourceDocumentOrigin.LOCAL_FILE,
      status: IngestionRunStatus.RUNNING,
      metadata: { reprocess_document_id: documentId, triggered_by: "mcp" },
    })
    .select("id")
    .single();

  if (!run) throw new Error("Failed to create reprocess run");

  // Criar novo job com force_reprocess
  const { data: job } = await supabase
    .from("ingestion_jobs")
    .insert({
      run_id: run.id,
      user_id: userId,
      source_document_id: documentId,
      status: IngestionJobStatus.DISCOVERED,
      metadata: { force_reprocess: true, triggered_by: "mcp" },
    })
    .select("id")
    .single();

  if (!job) throw new Error("Failed to create reprocess job");

  await supabase
    .from("ingestion_runs")
    .update({ status: IngestionRunStatus.COMPLETED })
    .eq("id", run.id);

  return {
    jobId: job.id,
    message: `Reprocess job created for ${doc.filename}. The worker will pick it up automatically.`,
  };
}
