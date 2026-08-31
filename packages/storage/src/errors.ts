/**
 * Erro tipado para qualquer falha de storage. Espelha o formato de
 * `PluggyApiError` em `@sbf/financial-connectors` (message + status + detalhe
 * cru) para manter o mesmo hábito de tratamento de erro de API externa no
 * monorepo.
 */
import {
  BlobAccessError,
  BlobError,
  BlobNotFoundError,
  BlobServiceNotAvailable,
  BlobServiceRateLimited,
  BlobStoreNotFoundError,
  BlobStoreSuspendedError,
} from "@vercel/blob";

export class StorageError extends Error {
  constructor(
    message: string,
    /** Status HTTP equivalente da falha, quando ela tem um. */
    readonly status?: number,
    /** Código curto e estável (ex.: "not_found", "already_exists"). */
    readonly code?: string,
    /** Caminho do objeto envolvido, quando aplicável. */
    readonly key?: string,
  ) {
    super(message);
    this.name = "StorageError";
  }
}

/**
 * O Vercel Blob recusa o `put` com `allowOverwrite: false` sobre um caminho
 * ocupado com um erro genérico (`bad_request`), sem classe própria — só a
 * mensagem do servidor distingue esse caso de qualquer outro pedido inválido.
 * Daí o casamento por texto: é a única informação disponível, e o caso importa
 * porque `upsert: false` é justamente quem depende dele.
 */
const JA_EXISTE = /already exists/i;

/**
 * Traduz um erro do `@vercel/blob` (ou qualquer outra coisa lançada) para o
 * `StorageError` que os call sites deste monorepo já sabem tratar.
 *
 * O status é o HTTP equivalente da condição, não o da resposta original: o
 * SDK não expõe o status cru, e o que os call sites checam é a condição
 * ("sumiu", "já existe", "sem permissão").
 */
export function toStorageError(err: unknown, key?: string): StorageError {
  if (err instanceof StorageError) return err;

  if (err instanceof BlobNotFoundError) {
    return new StorageError(err.message, 404, "not_found", key);
  }
  if (err instanceof BlobStoreNotFoundError) {
    return new StorageError(err.message, 404, "store_not_found", key);
  }
  if (err instanceof BlobAccessError) {
    return new StorageError(err.message, 403, "access_denied", key);
  }
  if (err instanceof BlobStoreSuspendedError) {
    return new StorageError(err.message, 403, "store_suspended", key);
  }
  if (err instanceof BlobServiceRateLimited) {
    return new StorageError(err.message, 429, "rate_limited", key);
  }
  if (err instanceof BlobServiceNotAvailable) {
    return new StorageError(err.message, 503, "service_unavailable", key);
  }
  if (err instanceof BlobError) {
    if (JA_EXISTE.test(err.message)) {
      return new StorageError(err.message, 409, "already_exists", key);
    }
    return new StorageError(err.message, undefined, "blob_error", key);
  }

  if (err instanceof Error) return new StorageError(err.message, undefined, undefined, key);
  return new StorageError(String(err), undefined, undefined, key);
}
