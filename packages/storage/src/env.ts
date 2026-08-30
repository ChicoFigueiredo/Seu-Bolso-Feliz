/**
 * Leitura de configuração a partir de variáveis de ambiente. Separado de
 * `client.ts` para poder testar as regras de validação/default sem construir
 * um AwsClient de verdade — mesmo espírito de `packages/db/src/queue.ts`
 * (parâmetro `env` com default `process.env`, testável passando um objeto
 * literal no lugar).
 */
import type { StorageConfig } from "./types";

function requireEnvVar(env: Record<string, string | undefined>, key: string): string {
  const value = env[key];
  if (!value) {
    throw new Error(`@sbf/storage: variável de ambiente obrigatória ausente: ${key}`);
  }
  return value;
}

/**
 * Lê `S3_ENDPOINT`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY` (obrigatórias),
 * `S3_REGION` (default "us-east-1") e `S3_FORCE_PATH_STYLE` (default `true`;
 * só um literal `"false"` desliga). Lança `Error` descrevendo a variável
 * ausente — falha rápida na inicialização em vez de um 403 confuso depois.
 */
export function loadStorageConfigFromEnv(
  env: Record<string, string | undefined> = process.env,
): StorageConfig {
  return {
    endpoint: requireEnvVar(env, "S3_ENDPOINT"),
    accessKeyId: requireEnvVar(env, "S3_ACCESS_KEY_ID"),
    secretAccessKey: requireEnvVar(env, "S3_SECRET_ACCESS_KEY"),
    region: env.S3_REGION || "us-east-1",
    forcePathStyle: env.S3_FORCE_PATH_STYLE !== "false",
  };
}
