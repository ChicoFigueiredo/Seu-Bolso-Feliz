export { StorageClient, StorageBucket } from "./client";
export { StorageError } from "./errors";
export { loadStorageConfigFromEnv } from "./env";
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
 * Atalho para o caso comum: cria o `StorageClient` já configurado a partir
 * de `S3_ENDPOINT` / `S3_ACCESS_KEY_ID` / `S3_SECRET_ACCESS_KEY` / `S3_REGION`
 * / `S3_FORCE_PATH_STYLE` em `process.env` (ou no objeto passado em `env`).
 * Lança se alguma variável obrigatória estiver ausente — ver `env.ts`.
 */
export function createStorageClientFromEnv(
  env?: Record<string, string | undefined>,
): StorageClient {
  return new StorageClient(loadStorageConfigFromEnv(env));
}
