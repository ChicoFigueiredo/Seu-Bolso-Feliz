/**
 * @sbf/mcp-server — Tool: find_documents_without_password
 * Encontra documentos que falharam por serem PDFs protegidos sem senha cadastrada.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionJobStatus } from "@sbf/ingestion-types";

export interface PasswordlessDocument {
  job_id: string;
  document_id: string;
  filename: string;
  error_message: string;
  created_at: string;
}

export async function findDocumentsWithoutPassword(
  supabase: SupabaseClient,
  userId: string,
): Promise<PasswordlessDocument[]> {
  const { data, error } = await supabase
    .from("ingestion_jobs")
    .select(`
      id,
      source_document_id,
      error_message,
      created_at,
      source_documents!inner(filename)
    `)
    .eq("user_id", userId)
    .eq("status", IngestionJobStatus.FAILED)
    .or("error_message.ilike.%password%,error_message.ilike.%encrypted%,error_message.ilike.%protected%")
    .order("created_at", { ascending: false });

  if (error || !data) return [];

  return data.map((row: any) => ({
    job_id: row.id,
    document_id: row.source_document_id,
    filename: row.source_documents?.filename ?? "unknown",
    error_message: row.error_message ?? "",
    created_at: row.created_at,
  }));
}
