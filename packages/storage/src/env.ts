/**
 * Leitura de configuração a partir de variáveis de ambiente. Separado de
 * `client.ts` para poder testar as regras de validação sem construir um
 * cliente de verdade — mesmo espírito de `packages/db/src/queue.ts`
 * (parâmetro `env` com default `process.env`, testável passando um objeto
 * literal no lugar).
 */
import type { StorageConfig } from "./types";

/** Nome da variável de ambiente que guarda o token do Vercel Blob. */
export const TOKEN_ENV_VAR = "BLOB_READ_WRITE_TOKEN";

function requireEnvVar(env: Record<string, string | undefined>, key: string): string {
  const value = env[key];
  if (!value) {
    throw new Error(`@sbf/storage: variável de ambiente obrigatória ausente: ${key}`);
  }
  return value;
}

/**
 * Lê `BLOB_READ_WRITE_TOKEN` — a única variável que o storage precisa, já que
 * o token do Vercel Blob carrega o id da loja. Lança `Error` nomeando a
 * variável ausente: falha rápida na inicialização, em vez de um 403 confuso
 * no meio de um upload.
 */
export function loadStorageConfigFromEnv(
  env: Record<string, string | undefined> = process.env,
): StorageConfig {
  return { token: requireEnvVar(env, TOKEN_ENV_VAR) };
}
