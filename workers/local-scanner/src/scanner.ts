/**
 * @sbf/worker-local-scanner — Core scanner logic
 * Escaneia diretório por arquivos novos, envia ao storage e cria jobs.
 */
import { readdir, stat, readFile, rename, mkdir } from "node:fs/promises";
import { join, extname } from "node:path";
import { and, eq } from "drizzle-orm";
import {
  enqueueJob,
  ingestion_runs,
  resolveOwnerId,
  source_documents,
  type Database,
} from "@sbf/db";
import { createStorageClientFromEnv, type StorageClient } from "@sbf/storage";
import { IngestionRunStatus, IngestionJobStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { buildOriginKey } from "@sbf/operations";
import { EXTENSOES_ACEITAS, MIME_POR_EXTENSAO } from "@sbf/contracts";
import { capacidadesNaoAtendidas, resolveFileRequires } from "./job-requires";
import { mensagemDeErro, uploadOriginal } from "./storage-upload";

export interface ScannerOptions {
  recursive?: boolean;
  extensions?: ReadonlySet<string>;
  dryRun?: boolean;
  verbose?: boolean;
  moveProcessedTo?: string;
  /**
   * Capacidades que este host anuncia (ver `resolveCapabilities` em
   * `@sbf/db`). Quando informado, o scanner avisa ao criar um job que exige
   * algo que ninguém aqui atende. Ausente, a checagem é pulada.
   */
  capabilities?: readonly string[];
  /**
   * Cliente de storage. Por padrão, o configurado por ambiente — o parâmetro
   * existe para os testes poderem injetar um dublê.
   */
  storage?: StorageClient;
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
 * Quem é o dono dos arquivos varridos. Sai do banco, não de um UUID copiado à
 * mão para o `.env` — ver `resolveOwnerId` em @sbf/db para o porquê.
 */
function getUserId(): Promise<string> {
  return resolveOwnerId();
}

/**
 * Coleta recursivamente todos os arquivos aceitos de um diretório.
 */
async function collectFiles(
  dirPath: string,
  recursive: boolean,
  // `ReadonlySet` porque o padrão é o `EXTENSOES_ACEITAS` de `@sbf/contracts`,
  // que é imutável de propósito — e ninguém aqui precisa escrever no conjunto.
  acceptedExts: ReadonlySet<string>,
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
  db: Database,
  dirPath: string,
  options: ScannerOptions = {},
): Promise<number> {
  const userId = await getUserId();
  const {
    recursive = false,
    extensions = ACCEPTED_EXTENSIONS,
    dryRun = false,
    verbose = false,
    moveProcessedTo,
    capabilities,
  } = options;

  const filePaths = await collectFiles(dirPath, recursive, extensions);

  if (filePaths.length === 0) {
    return 0;
  }

  // Em dry-run nada sobe, então nem o cliente é criado: exigir credencial de
  // S3 seria pedir configuração para não usar.
  const storage = dryRun ? undefined : (options.storage ?? createStorageClientFromEnv());

  // Criar run (skip em dry-run)
  let runId = "dry-run";
  if (!dryRun) {
    try {
      const [run] = await db
        .insert(ingestion_runs)
        .values({
          user_id: userId,
          source_type: SourceDocumentOrigin.LOCAL_FILE,
          status: IngestionRunStatus.RUNNING,
          metadata: { directory: dirPath, recursive },
        })
        .returning({ id: ingestion_runs.id });

      if (!run) throw new Error("insert de ingestion_run não devolveu linha");
      runId = run.id;
    } catch (err) {
      console.error("[SCANNER] Failed to create run:", mensagemDeErro(err));
      return 0;
    }
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

    if (dryRun || !storage) {
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
    let jaProcessado: boolean;
    try {
      const [existing] = await db
        .select({ id: source_documents.id })
        .from(source_documents)
        .where(
          and(
            eq(source_documents.user_id, userId),
            eq(source_documents.origin_type, SourceDocumentOrigin.LOCAL_FILE),
            eq(source_documents.origin_key, originKey),
          ),
        )
        .limit(1);
      jaProcessado = existing !== undefined;
    } catch (err) {
      // A consulta antiga descartava o erro e seguia como se nada existisse —
      // o que, na prática, mandava o arquivo para um upload que ia falhar
      // igual. Aqui a falha é dita e o arquivo fica para o próximo scan.
      console.error(`[SCANNER] Dedup check failed for ${entry}:`, mensagemDeErro(err));
      continue;
    }

    if (jaProcessado) {
      if (verbose) console.log(`[SCANNER] Skip (já processado): ${filePath}`);
      continue;
    }

    // `new Uint8Array(...)` copia para um ArrayBuffer comum: o `Buffer` do
    // Node é tipado sobre `ArrayBufferLike`, que não satisfaz o `UploadBody`
    // do `@sbf/storage` (restrito a views sobre `ArrayBuffer`).
    const fileBuffer = new Uint8Array(await readFile(filePath));
    const storagePath = `${userId}/${crypto.randomUUID()}/${entry}`;

    const upload = await uploadOriginal(storage, storagePath, fileBuffer, mimeType);

    if (!upload.ok) {
      console.error(`[SCANNER] Upload failed for ${entry}:`, upload.erro);
      continue;
    }

    // Criar source_document
    let documentId: string;
    try {
      const [doc] = await db
        .insert(source_documents)
        .values({
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
        .returning({ id: source_documents.id });

      if (!doc) throw new Error("insert de source_document não devolveu linha");
      documentId = doc.id;
    } catch (err) {
      console.error(
        `[SCANNER] Failed to create source_document for ${entry}:`,
        mensagemDeErro(err),
      );
      continue;
    }

    // Criar ingestion_job
    const requires = resolveFileRequires(entry, fileBuffer);
    if (capabilities) {
      const semAtendimento = capacidadesNaoAtendidas(requires, capabilities);
      if (semAtendimento.length > 0) {
        console.warn(
          `[SCANNER] ⚠️  ${entry} exige [${semAtendimento.join(", ")}] — nenhum worker deste host anuncia essa capacidade; o job só sai da fila se outro host oferecer.`,
        );
      }
    }

    try {
      await enqueueJob({
        db,
        runId,
        userId,
        sourceDocumentId: documentId,
        status: IngestionJobStatus.DISCOVERED,
        requires,
        metadata: { filename: entry, origin: "local_scanner" },
      });
    } catch (err) {
      console.error(`[SCANNER] Failed to create job for ${entry}:`, mensagemDeErro(err));
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
    try {
      await db
        .update(ingestion_runs)
        .set({
          status: IngestionRunStatus.COMPLETED,
          completed_at: new Date().toISOString(),
          stats: { discovered, directory: dirPath },
        })
        .where(eq(ingestion_runs.id, runId));
    } catch (err) {
      // O scan já aconteceu; só o registro do desfecho se perde. Derrubar o
      // processo aqui transformaria um scan bem-sucedido em erro.
      console.error("[SCANNER] Failed to finalize run:", mensagemDeErro(err));
    }
  }

  return discovered;
}
