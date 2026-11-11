/**
 * Acesso ao storage dos originais da ingestão.
 *
 * O balde é o `@sbf/storage`, sobre o Vercel Blob. Mesmo padrão de
 * `workers/ingestion/src/storage.ts` — o nome do bucket não muda (lá ele vira
 * prefixo de caminho), e `source_documents.storage_path` continua sendo a
 * chave.
 *
 * O cliente é um singleton preguiçoso pelo mesmo motivo de `getDb()`:
 * construí-lo lê `BLOB_READ_WRITE_TOKEN` do ambiente e valida a credencial, e
 * fazer isso a cada chamada de ferramenta seria desperdício puro.
 */
import { createStorageClientFromEnv, type StorageBucket } from "@sbf/storage";

/** Bucket dos arquivos originais — no Vercel Blob, o prefixo dos caminhos. */
export const INGESTION_BUCKET = "ingestion-originals";

let cached: StorageBucket | null = null;

export function getIngestionBucket(): StorageBucket {
  if (!cached) cached = createStorageClientFromEnv().from(INGESTION_BUCKET);
  return cached;
}

/** Só para teste: descarta o singleton. */
export function resetIngestionBucket(): void {
  cached = null;
}
