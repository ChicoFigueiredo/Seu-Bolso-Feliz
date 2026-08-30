/**
 * @sbf/mcp-server — Tool: reprocess_document
 * Reprocessa um documento específico com force_reprocess.
 */
import { and, eq } from "drizzle-orm";
import { enqueueJob, schema, type Database } from "@sbf/db";
import { IngestionJobStatus, IngestionRunStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { resolveRequires } from "../job-requires.js";

export async function reprocessDocument(
  db: Database,
  userId: string,
  documentId: string,
): Promise<{ jobId: string; message: string }> {
  // Verificar que o documento pertence ao usuário
  const [doc] = await db
    .select({ id: schema.source_documents.id, filename: schema.source_documents.filename })
    .from(schema.source_documents)
    .where(
      and(eq(schema.source_documents.id, documentId), eq(schema.source_documents.user_id, userId)),
    );

  if (!doc) {
    throw new Error(`Document not found or not owned by user: ${documentId}`);
  }

  // Criar um novo run para o reprocessamento
  const [run] = await db
    .insert(schema.ingestion_runs)
    .values({
      user_id: userId,
      source_type: SourceDocumentOrigin.LOCAL_FILE,
      status: IngestionRunStatus.RUNNING,
      metadata: { reprocess_document_id: documentId, triggered_by: "mcp" },
    })
    .returning({ id: schema.ingestion_runs.id });

  if (!run) throw new Error("Failed to create reprocess run");

  // Criar novo job com force_reprocess via enqueueJob — não insert direto —
  // para nascer com available_at/locked_by corretos e disparar o NOTIFY que
  // acorda o worker (ver packages/db/src/queue.ts).
  const job = await enqueueJob({
    db,
    runId: run.id,
    userId,
    sourceDocumentId: documentId,
    status: IngestionJobStatus.DISCOVERED,
    requires: resolveRequires(doc.filename),
    metadata: { force_reprocess: true, triggered_by: "mcp" },
  });

  await db
    .update(schema.ingestion_runs)
    .set({ status: IngestionRunStatus.COMPLETED })
    .where(eq(schema.ingestion_runs.id, run.id));

  return {
    jobId: job.id,
    message: `Reprocess job created for ${doc.filename}. The worker will pick it up automatically.`,
  };
}
