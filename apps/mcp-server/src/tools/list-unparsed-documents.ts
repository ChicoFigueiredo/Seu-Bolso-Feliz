/**
 * @sbf/mcp-server — Tool: list_unparsed_documents
 * Lista documentos que ainda não foram parseados ou estão com erro.
 */
import { and, desc, eq, inArray } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
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
  db: Database,
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

  // O modificador `!inner` do driver anterior (só jobs com source_document
  // existente) vira innerJoin aqui. O join é reforçado com `user_id` da
  // tabela ligada —
  // defesa em profundidade agora que não há RLS garantindo isso sozinho.
  const rows = await db
    .select({
      id: schema.ingestion_jobs.id,
      source_document_id: schema.ingestion_jobs.source_document_id,
      status: schema.ingestion_jobs.status,
      error_message: schema.ingestion_jobs.error_message,
      created_at: schema.ingestion_jobs.created_at,
      filename: schema.source_documents.filename,
    })
    .from(schema.ingestion_jobs)
    .innerJoin(
      schema.source_documents,
      and(
        eq(schema.ingestion_jobs.source_document_id, schema.source_documents.id),
        eq(schema.source_documents.user_id, userId),
      ),
    )
    .where(
      and(
        eq(schema.ingestion_jobs.user_id, userId),
        inArray(schema.ingestion_jobs.status, statuses),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(limit);

  return rows.map((row) => ({
    job_id: row.id,
    document_id: row.source_document_id,
    filename: row.filename ?? "unknown",
    status: row.status,
    error_message: row.error_message,
    created_at: row.created_at ?? "",
  }));
}
