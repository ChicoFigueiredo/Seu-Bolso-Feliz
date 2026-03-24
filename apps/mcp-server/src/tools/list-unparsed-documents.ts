/**
 * @sbf/mcp-server — Tool: list_unparsed_documents
 * Lista documentos que ainda não foram parseados ou estão com erro.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionJobStatus } from "@sbf/ingestion-types";

export interface UnparsedDocument {
  job_id: string;
  document_id: string | null;
  filename: string;
  status: string;
  error_message: string | null;
  created_at: string;
}

export async function listUnparsedDocuments(
  supabase: SupabaseClient,
  userId: string,
  opts?: { limit?: number; includeErrors?: boolean },
): Promise<UnparsedDocument[]> {
  const limit = opts?.limit ?? 50;
  const statuses: string[] = [
    IngestionJobStatus.DISCOVERED,
    IngestionJobStatus.DOWNLOADED,
    IngestionJobStatus.HASHED,
    IngestionJobStatus.QUEUED,
  ];

  if (opts?.includeErrors) {
    statuses.push(IngestionJobStatus.FAILED);
  }

  const { data, error } = await supabase
    .from("ingestion_jobs")
    .select(`
      id,
      source_document_id,
      status,
      error_message,
      created_at,
      source_documents!inner(filename)
    `)
    .eq("user_id", userId)
    .in("status", statuses)
    .order("created_at", { ascending: false })
    .limit(limit);

  if (error || !data) return [];

  return data.map((row: any) => ({
    job_id: row.id,
    document_id: row.source_document_id,
    filename: row.source_documents?.filename ?? "unknown",
    status: row.status,
    error_message: row.error_message,
    created_at: row.created_at,
  }));
}
