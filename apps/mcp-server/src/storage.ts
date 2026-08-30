/**
 * Acesso ao storage dos originais da ingestão.
 *
 * Substitui `supabase.storage.from("ingestion-originals")` pelo cliente
 * S3-compatível de `@sbf/storage`. Mesmo padrão de
 * `workers/ingestion/src/storage.ts` — o nome do bucket não muda, e
 * `source_documents.storage_path` continua sendo a chave.
 *
 * O cliente é um singleton preguiçoso pelo mesmo motivo de `getDb()`:
 * construí-lo lê variáveis de ambiente e monta um assinador SigV4, e fazer
 * isso a cada chamada de ferramenta seria desperdício puro.
 */
import { createStorageClientFromEnv, type StorageBucket } from "@sbf/storage";

/** Bucket dos arquivos originais, herdado do Supabase Storage. */
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
