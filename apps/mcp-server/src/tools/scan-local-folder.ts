/**
 * @sbf/mcp-server — Tool: scan_local_folder
 * Escaneia diretório local por documentos e cria jobs de ingestão.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { readdir, stat, readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import {
  IngestionRunStatus,
  IngestionJobStatus,
  SourceDocumentOrigin,
} from "@sbf/ingestion-types";
import { buildOriginKey } from "@sbf/operations";

const ACCEPTED_EXTENSIONS = new Set([
  ".pdf", ".png", ".jpg", ".jpeg", ".webp",
  ".csv", ".xls", ".xlsx", ".xml", ".ofx",
]);

const MIME_MAP: Record<string, string> = {
  ".pdf": "application/pdf",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".webp": "image/webp",
  ".csv": "text/csv",
  ".xls": "application/vnd.ms-excel",
  ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  ".xml": "application/xml",
  ".ofx": "application/octet-stream",
};

export async function scanLocalFolder(
  supabase: SupabaseClient,
  userId: string,
  dirPath: string,
): Promise<{ discovered: number; skipped: number; errors: string[] }> {
  let entries: string[];
  try {
    entries = await readdir(dirPath);
  } catch {
    return { discovered: 0, skipped: 0, errors: [`Directory not found: ${dirPath}`] };
  }

  const { data: run, error: runError } = await supabase
    .from("ingestion_runs")
    .insert({
      user_id: userId,
      source_type: SourceDocumentOrigin.LOCAL_FILE,
      status: IngestionRunStatus.RUNNING,
      metadata: { directory: dirPath, triggered_by: "mcp" },
    })
    .select("id")
    .single();

  if (runError || !run) {
    return { discovered: 0, skipped: 0, errors: [`Failed to create run: ${runError?.message}`] };
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

      const originKey = buildOriginKey({ type: "local_file", filepath: filePath, mtimeMs: fileStat.mtimeMs });
      const { data: existing } = await supabase
        .from("source_documents")
        .select("id")
        .eq("user_id", userId)
        .eq("origin_type", SourceDocumentOrigin.LOCAL_FILE)
        .eq("origin_key", originKey)
        .maybeSingle();

      if (existing) { skipped++; continue; }

      const fileData = await readFile(filePath);
      const storagePath = `${userId}/${Date.now()}-${entry}`;
      const mimeType = MIME_MAP[ext] ?? "application/octet-stream";

      const { error: uploadError } = await supabase.storage
        .from("ingestion-originals")
        .upload(storagePath, fileData, { contentType: mimeType });
      if (uploadError) { errors.push(`Upload failed for ${entry}: ${uploadError.message}`); continue; }

      const { data: doc } = await supabase
        .from("source_documents")
        .insert({
          user_id: userId,
          origin_type: SourceDocumentOrigin.LOCAL_FILE,
          origin_key: originKey,
          filename: entry,
          mime_type: mimeType,
          file_size: fileStat.size,
          storage_path: storagePath,
          metadata: { local_path: filePath },
        })
        .select("id")
        .single();

      if (!doc) { errors.push(`Failed to create source_document for ${entry}`); continue; }

      await supabase.from("ingestion_jobs").insert({
        run_id: run.id,
        user_id: userId,
        source_document_id: doc.id,
        status: IngestionJobStatus.DISCOVERED,
        metadata: { filename: entry, mime_type: mimeType, triggered_by: "mcp" },
      });

      discovered++;
    } catch (err: unknown) {
      errors.push(`Error processing ${entry}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  await supabase
    .from("ingestion_runs")
    .update({
      status: IngestionRunStatus.COMPLETED,
      metadata: { directory: dirPath, triggered_by: "mcp", discovered, skipped, error_count: errors.length },
    })
    .eq("id", run.id);

  return { discovered, skipped, errors };
}
