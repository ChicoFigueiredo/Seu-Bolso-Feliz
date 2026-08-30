/**
 * @sbf/mcp-server — Tool: find_documents_without_password
 * Encontra documentos que falharam por serem PDFs protegidos sem senha cadastrada.
 */
import { and, desc, eq, ilike, or } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
import { IngestionJobStatus } from "@sbf/ingestion-types";

export interface PasswordlessDocument {
  job_id: string;
  document_id: string;
  filename: string;
  error_message: string;
  created_at: string;
}

export async function findDocumentsWithoutPassword(
  db: Database,
  userId: string,
): Promise<PasswordlessDocument[]> {
  const rows = await db
    .select({
      id: schema.ingestion_jobs.id,
      source_document_id: schema.ingestion_jobs.source_document_id,
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
        eq(schema.ingestion_jobs.status, IngestionJobStatus.FAILED),
        or(
          ilike(schema.ingestion_jobs.error_message, "%password%"),
          ilike(schema.ingestion_jobs.error_message, "%encrypted%"),
          ilike(schema.ingestion_jobs.error_message, "%protected%"),
        ),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at));

  // O INNER JOIN já garante source_document_id não-nulo na prática; o
  // `flatMap` com retorno antecipado só torna isso visível para o
  // TypeScript (estreitamento de fluxo padrão, sem `as` nem type predicate).
  return rows.flatMap((row) => {
    if (row.source_document_id === null) return [];
    return [
      {
        job_id: row.id,
        document_id: row.source_document_id,
        filename: row.filename ?? "unknown",
        error_message: row.error_message ?? "",
        created_at: row.created_at ?? "",
      },
    ];
  });
}
