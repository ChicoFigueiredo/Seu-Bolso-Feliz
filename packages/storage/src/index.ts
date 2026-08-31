export { StorageClient, StorageBucket } from "./client";
export { StorageError, toStorageError } from "./errors";
export { loadStorageConfigFromEnv, TOKEN_ENV_VAR } from "./env";
export { vercelBlobApi } from "./blob-api";
export type { BlobApi } from "./blob-api";
export type {
  StorageConfig,
  UploadBody,
  UploadOptions,
  StorageObjectInfo,
  ListObjectsOptions,
  ListObjectsResult,
} from "./types";

import { StorageClient } from "./client";
import { loadStorageConfigFromEnv } from "./env";

/**
 * Atalho para o caso comum: cria o `StorageClient` já configurado a partir de
 * `BLOB_READ_WRITE_TOKEN` em `process.env` (ou no objeto passado em `env`).
 * Lança se a variável estiver ausente — ver `env.ts`.
 */
export function createStorageClientFromEnv(
  env?: Record<string, string | undefined>,
): StorageClient {
  return new StorageClient(loadStorageConfigFromEnv(env));
}
