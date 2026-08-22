/**
 * @sbf/worker-local-scanner — Core scanner logic
 * Escaneia diretório por arquivos novos, faz upload e cria jobs.
 */
import { readdir, stat, readFile, rename, mkdir } from "node:fs/promises";
import { join, extname } from "node:path";
import type { SupabaseClient } from "@supabase/supabase-js";
import { IngestionRunStatus, IngestionJobStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { buildOriginKey } from "@sbf/operations";
import { EXTENSOES_ACEITAS, MIME_POR_EXTENSAO } from "@sbf/contracts";

export interface ScannerOptions {
  recursive?: boolean;
  extensions?: Set<string>;
  dryRun?: boolean;
  verbose?: boolean;
  moveProcessedTo?: string;
}

/**
 * Extensões e MIMEs vêm do registro único (`@sbf/contracts`).
 *
 * Antes eram duas listas locais que divergiram do resto: aceitavam `.xls`, que
 * o pipeline não abre, e rotulavam `.ofx` como `application/octet-stream`, o
 * que fazia o extrato bancário chegar ao extrator sem nenhuma pista do que era.
 */
const ACCEPTED_EXTENSIONS = EXTENSOES_ACEITAS;
const MIME_MAP = MIME_POR_EXTENSAO;

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
 * Coleta recursivamente todos os arquivos aceitos de um diretório.
 */
async function collectFiles(
  dirPath: string,
  recursive: boolean,
  acceptedExts: Set<string>,
): Promise<string[]> {
  const files: string[] = [];
  let entries: string[];

  try {
    entries = await readdir(dirPath);
  } catch {
    return files;
  }

  for (const entry of entries) {
    const fullPath = join(dirPath, entry);
    let fileStat;
    try {
      fileStat = await stat(fullPath);
    } catch {
      continue;
    }

    if (fileStat.isDirectory()) {
      if (recursive) {
        files.push(...(await collectFiles(fullPath, true, acceptedExts)));
      }
      continue;
    }

    const ext = extname(entry).toLowerCase();
    if (acceptedExts.has(ext)) {
      files.push(fullPath);
    }
  }

  return files;
}

/**
 * Escaneia um diretório e retorna quantos documentos novos foram descobertos.
 * `options` é opcional para compatibilidade retroativa.
 */
export async function scanDirectory(
  supabase: SupabaseClient,
  dirPath: string,
  options: ScannerOptions = {},
): Promise<number> {
  const userId = getUserId();
  const {
    recursive = false,
    extensions = ACCEPTED_EXTENSIONS,
    dryRun = false,
    verbose = false,
    moveProcessedTo,
  } = options;

  const filePaths = await collectFiles(dirPath, recursive, extensions);

  if (filePaths.length === 0) {
    return 0;
  }

  // Criar run (skip em dry-run)
  let runId = "dry-run";
  if (!dryRun) {
    const { data: run, error: runError } = await supabase
      .from("ingestion_runs")
      .insert({
        user_id: userId,
        source_type: SourceDocumentOrigin.LOCAL_FILE,
        status: IngestionRunStatus.RUNNING,
        metadata: { directory: dirPath, recursive },
      })
      .select("id")
      .single();

    if (runError || !run) {
      console.error("[SCANNER] Failed to create run:", runError?.message);
      return 0;
    }
    runId = run.id;
  }

  let discovered = 0;

  for (const filePath of filePaths) {
    const entry = filePath.split("/").pop()!;
    const ext = extname(entry).toLowerCase();
    const mimeType = MIME_MAP[ext] ?? "application/octet-stream";

    let fileStat;
    try {
      fileStat = await stat(filePath);
    } catch {
      continue;
    }

    if (dryRun) {
      console.log(`  [DRY RUN] ${filePath} (${mimeType})`);
      discovered++;
      continue;
    }

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

    if (existing && existing.length > 0) {
      if (verbose) console.log(`[SCANNER] Skip (já processado): ${filePath}`);
      continue;
    }

    // Upload para Storage
    const fileBuffer = await readFile(filePath);
    const storagePath = `${userId}/${crypto.randomUUID()}/${entry}`;

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
    const { error: jobError } = await supabase.from("ingestion_jobs").insert({
      run_id: runId,
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
    if (verbose) console.log(`[SCANNER] ✅ ${filePath}`);

    // Mover arquivo processado para outra pasta
    if (moveProcessedTo) {
      try {
        await mkdir(moveProcessedTo, { recursive: true });
        const dest = join(moveProcessedTo, entry);
        await rename(filePath, dest);
        if (verbose) console.log(`[SCANNER]   → movido para ${dest}`);
      } catch (err) {
        console.warn(
          `[SCANNER] Falha ao mover ${entry}:`,
          err instanceof Error ? err.message : err,
        );
      }
    }
  }

  // Finalizar run
  if (!dryRun && runId !== "dry-run") {
    await supabase
      .from("ingestion_runs")
      .update({
        status: IngestionRunStatus.COMPLETED,
        completed_at: new Date().toISOString(),
        stats: { discovered, directory: dirPath },
      })
      .eq("id", runId);
  }

  return discovered;
}
