/**
 * Tipos públicos do @sbf/storage. Ver `client.ts` para a implementação e
 * `docs/integrations/storage-seaweedfs.md` para o contexto de infraestrutura
 * (SeaweedFS atrás de nginx, path-style obrigatório).
 */

export interface StorageConfig {
  /** URL base do endpoint S3-compatível, ex.: "https://s3.exemplo.com" ou "http://127.0.0.1:9000". */
  endpoint: string;
  accessKeyId: string;
  secretAccessKey: string;
  /** @default "us-east-1" */
  region?: string;
  /**
   * SeaweedFS exige endereçamento path-style (`endpoint/bucket/chave`), não
   * virtual-host (`bucket.endpoint/chave`). Hoje só path-style está
   * implementado — `false` lança erro explícito em `buildObjectUrl` em vez
   * de montar uma URL virtual-host incorreta em silêncio.
   * @default true
   */
  forcePathStyle?: boolean;
  /**
   * Nº de novas tentativas em respostas 5xx/429, com backoff exponencial
   * (implementado pelo próprio aws4fetch). O default da lib é 10 — na prática
   * até ~51s de espera acumulada antes de desistir, o que é longo demais para
   * uma Server Action ou uma rota web esperando resposta. Este pacote usa um
   * default bem mais conservador; workers em background que preferem mais
   * resiliência podem informar um valor maior.
   * @default 2
   */
  retries?: number;
}

/**
 * Formatos de corpo aceitos por `upload`. É um subconjunto de `BodyInit`
 * (fetch) que cobre tudo que o código hoje já passa para o Supabase Storage:
 * `File`/`Blob` no browser, `Buffer`/`Uint8Array`/`ArrayBuffer` nos workers,
 * e `string` para o CSV de `user-files`.
 *
 * `ArrayBufferView<ArrayBuffer>` (em vez do `ArrayBufferView` genérico) é de
 * propósito: a partir do TS 5.7, `lib.dom` restringe `BodyInit` a views sobre
 * `ArrayBuffer` normal, excluindo `SharedArrayBuffer` — `Buffer`/`Uint8Array`
 * comuns (readFile, File/Blob) sempre se qualificam.
 */
export type UploadBody = Blob | ArrayBuffer | ArrayBufferView<ArrayBuffer> | string;

export interface UploadOptions {
  contentType?: string;
  /**
   * Quando `false`, falha com `StorageError` (code `"already_exists"`) se já
   * existir um objeto no mesmo caminho, em vez de sobrescrever.
   *
   * Implementado como HEAD-antes-do-PUT — **não é atômico**: uma corrida rara
   * entre dois uploads concorrentes para o mesmo caminho ainda pode resultar
   * em sobrescrita. No único call site atual que usa `upsert: false`
   * (upload manual em `ingestion.ts`), a idempotência real já é garantida
   * antes, via `content_hash`/`origin_key` no Postgres — isto é só uma
   * segunda barreira, não a garantia primária.
   *
   * @default true
   */
  upsert?: boolean;
}

export interface StorageObjectInfo {
  key: string;
  size: number;
  /** Data em formato ISO 8601, como devolvida pelo `<LastModified>` do S3. */
  lastModified: string;
}

export interface ListObjectsOptions {
  maxKeys?: number;
  continuationToken?: string;
}

export interface ListObjectsResult {
  objects: StorageObjectInfo[];
  isTruncated: boolean;
  nextContinuationToken?: string;
}

/** Config já resolvida (com defaults aplicados) — uso interno do pacote. */
export interface ResolvedStorageConfig {
  endpoint: string;
  region: string;
  forcePathStyle: boolean;
}
