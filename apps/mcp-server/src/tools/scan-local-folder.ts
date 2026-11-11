/**
 * @sbf/mcp-server — Tool: scan_local_folder
 * Escaneia diretório local por documentos e cria jobs de ingestão.
 */
import { readdir, stat, readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import { and, eq } from "drizzle-orm";
import { enqueueJob, schema, type Database } from "@sbf/db";
import type { StorageBucket } from "@sbf/storage";
import { IngestionRunStatus, IngestionJobStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { buildOriginKey } from "@sbf/operations";
import { EXTENSOES_ACEITAS, MIME_POR_EXTENSAO } from "@sbf/contracts";
import { getIngestionBucket } from "../storage.js";
import { resolveRequires } from "../job-requires.js";

// Registro único de formatos aceitos (@sbf/contracts), em vez da lista local
// que existia aqui antes — que aceitava `.xls` (o pipeline não abre) e
// rotulava `.ofx` como `application/octet-stream` (mesma divergência
// descrita no cabeçalho de workers/local-scanner/src/scanner.ts).
const ACCEPTED_EXTENSIONS = EXTENSOES_ACEITAS;
const MIME_MAP = MIME_POR_EXTENSAO;

export async function scanLocalFolder(
  db: Database,
  userId: string,
  dirPath: string,
  storage?: StorageBucket,
): Promise<{ discovered: number; skipped: number; errors: string[] }> {
  let entries: string[];
  try {
    entries = await readdir(dirPath);
  } catch {
    return { discovered: 0, skipped: 0, errors: [`Directory not found: ${dirPath}`] };
  }

  const bucket = storage ?? getIngestionBucket();

  let runId: string;
  try {
    const [run] = await db
      .insert(schema.ingestion_runs)
      .values({
        user_id: userId,
        source_type: SourceDocumentOrigin.LOCAL_FILE,
        status: IngestionRunStatus.RUNNING,
        metadata: { directory: dirPath, triggered_by: "mcp" },
      })
      .returning({ id: schema.ingestion_runs.id });
    if (!run) throw new Error("insert não devolveu linha");
    runId = run.id;
  } catch (err) {
    return {
      discovered: 0,
      skipped: 0,
      errors: [`Failed to create run: ${err instanceof Error ? err.message : String(err)}`],
    };
  }

  let discovered = 0;
  let skipped = 0;
  const errors: string[] = [];

  for (const entry of entries) {
    const ext = extname(entry).toLowerCase();
    if (!ACCEPTED_EXTENSIONS.has(ext)) continue;

    const filePath = join(dirPath, entry);
    try {
      const fileStat = await stat(filePath);
      if (!fileStat.isFile()) continue;

      const originKey = buildOriginKey({
        type: "local_file",
        filepath: filePath,
        mtimeMs: fileStat.mtimeMs,
      });

      const [existing] = await db
        .select({ id: schema.source_documents.id })
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.user_id, userId),
            eq(schema.source_documents.origin_type, SourceDocumentOrigin.LOCAL_FILE),
            eq(schema.source_documents.origin_key, originKey),
          ),
        )
        .limit(1);

      if (existing) {
        skipped++;
        continue;
      }

      // Cópia para ArrayBuffer comum: o `Buffer` do Node é tipado sobre
      // `ArrayBufferLike`, que não satisfaz o `UploadBody` de @sbf/storage
      // (restrito a views sobre `ArrayBuffer`).
      const fileData = new Uint8Array(await readFile(filePath));
      const storagePath = `${userId}/${crypto.randomUUID()}/${entry}`;
      const mimeType = MIME_MAP[ext] ?? "application/octet-stream";

      try {
        await bucket.upload(storagePath, fileData, { contentType: mimeType });
      } catch (err) {
        errors.push(
          `Upload failed for ${entry}: ${err instanceof Error ? err.message : String(err)}`,
        );
        continue;
      }

      let documentId: string;
      try {
        const [doc] = await db
          .insert(schema.source_documents)
          .values({
            user_id: userId,
            origin_type: SourceDocumentOrigin.LOCAL_FILE,
            origin_key: originKey,
            filename: entry,
            mime_type: mimeType,
            file_size_bytes: fileStat.size,
            storage_path: storagePath,
            metadata: { local_path: filePath },
          })
          .returning({ id: schema.source_documents.id });
        if (!doc) throw new Error("insert não devolveu linha");
        documentId = doc.id;
      } catch {
        errors.push(`Failed to create source_document for ${entry}`);
        continue;
      }

      await enqueueJob({
        db,
        runId,
        userId,
        sourceDocumentId: documentId,
        status: IngestionJobStatus.DISCOVERED,
        requires: resolveRequires(entry, fileData),
        metadata: { filename: entry, mime_type: mimeType, triggered_by: "mcp" },
      });

      discovered++;
    } catch (err: unknown) {
      errors.push(`Error processing ${entry}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  await db
    .update(schema.ingestion_runs)
    .set({
      status: IngestionRunStatus.COMPLETED,
      metadata: {
        directory: dirPath,
        triggered_by: "mcp",
        discovered,
        skipped,
        error_count: errors.length,
      },
    })
    .where(eq(schema.ingestion_runs.id, runId));

  return { discovered, skipped, errors };
}
