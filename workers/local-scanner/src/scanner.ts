/**
 * @sbf/worker-local-scanner — Core scanner logic
 * Escaneia diretório por arquivos novos, faz upload e cria jobs.
 */
import { readdir, stat, readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import type { SupabaseClient } from "@supabase/supabase-js";
import {
  IngestionRunStatus,
  IngestionJobStatus,
  SourceDocumentOrigin,
} from "@sbf/ingestion-types";
import { buildOriginKey } from "@sbf/operations";

/** Extensões aceitas para ingestão */
const ACCEPTED_EXTENSIONS = new Set([
  ".pdf", ".png", ".jpg", ".jpeg", ".webp",
  ".csv", ".xls", ".xlsx", ".xml", ".ofx",
]);

/** Mapa de extensão → MIME type */
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

/**
 * ID do usuário local — em ambiente de desenvolvimento, usa variável de ambiente.
 * Em produção, isso será resolvido via auth token do request.
 */
function getUserId(): string {
  const uid = process.env.LOCAL_USER_ID;
  if (!uid) throw new Error("LOCAL_USER_ID is required for local scanner");
  return uid;
}

/**
 * Escaneia um diretório e retorna quantos documentos novos foram descobertos.
 */
export async function scanDirectory(
  supabase: SupabaseClient,
  dirPath: string,
): Promise<number> {
  const userId = getUserId();

  let entries: string[];
  try {
    entries = await readdir(dirPath);
  } catch {
    // Diretório não existe ou inacessível — normal no primeiro run
    return 0;
  }

  // Criar run para esta sessão de scan
  const { data: run, error: runError } = await supabase
    .from("ingestion_runs")
    .insert({
      user_id: userId,
      source_type: SourceDocumentOrigin.LOCAL_FILE,
      status: IngestionRunStatus.RUNNING,
      metadata: { directory: dirPath },
    })
    .select("id")
    .single();

  if (runError || !run) {
    console.error("[SCANNER] Failed to create run:", runError?.message);
    return 0;
  }

  let discovered = 0;

  for (const entry of entries) {
    const ext = extname(entry).toLowerCase();
    if (!ACCEPTED_EXTENSIONS.has(ext)) continue;

    const filePath = join(dirPath, entry);
    const fileStat = await stat(filePath);
    if (!fileStat.isFile()) continue;

    const originKey = buildOriginKey({
      type: "local_file",
      filepath: filePath,
      mtimeMs: fileStat.mtimeMs,
    });

    // Verificar se já existe um source_document com mesma origin_key
    const { data: existing } = await supabase
      .from("source_documents")
      .select("id")
      .eq("user_id", userId)
      .eq("origin_type", SourceDocumentOrigin.LOCAL_FILE)
      .eq("origin_key", originKey)
      .limit(1);

    if (existing && existing.length > 0) continue; // Já processado

    // Upload para Storage
    const fileBuffer = await readFile(filePath);
    const storagePath = `${userId}/${crypto.randomUUID()}/${entry}`;
    const mimeType = MIME_MAP[ext] ?? "application/octet-stream";

    const { error: uploadError } = await supabase.storage
      .from("ingestion-originals")
      .upload(storagePath, fileBuffer, { contentType: mimeType });

    if (uploadError) {
      console.error(`[SCANNER] Upload failed for ${entry}:`, uploadError.message);
      continue;
    }

    // Criar source_document
    const { data: doc, error: docError } = await supabase
      .from("source_documents")
      .insert({
        user_id: userId,
        origin_type: SourceDocumentOrigin.LOCAL_FILE,
        origin_key: originKey,
        local_filepath: filePath,
        local_mtime: fileStat.mtime.toISOString(),
        filename: entry,
        mime_type: mimeType,
        file_size_bytes: fileStat.size,
        storage_path: storagePath,
        status: "active",
      })
      .select("id")
      .single();

    if (docError || !doc) {
      console.error(`[SCANNER] Failed to create source_document for ${entry}:`, docError?.message);
      continue;
    }

    // Criar ingestion_job
    const { error: jobError } = await supabase
      .from("ingestion_jobs")
      .insert({
        run_id: run.id,
        user_id: userId,
        source_document_id: doc.id,
        status: IngestionJobStatus.DISCOVERED,
        metadata: { filename: entry, origin: "local_scanner" },
      });

    if (jobError) {
      console.error(`[SCANNER] Failed to create job for ${entry}:`, jobError.message);
      continue;
    }

    discovered++;
  }

  // Finalizar run
  await supabase
    .from("ingestion_runs")
    .update({
      status: discovered > 0 ? IngestionRunStatus.COMPLETED : IngestionRunStatus.COMPLETED,
      completed_at: new Date().toISOString(),
      stats: { discovered, directory: dirPath },
    })
    .eq("id", run.id);

  return discovered;
}
