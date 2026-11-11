/**
 * Tipos públicos do @sbf/storage. Ver `client.ts` para a implementação e
 * `docs/integrations/storage-vercel-blob.md` para o contexto de
 * infraestrutura (loja do Vercel Blob, blobs privados, URL assinada).
 */

export interface StorageConfig {
  /**
   * Token de leitura/escrita da loja do Vercel Blob — o valor de
   * `BLOB_READ_WRITE_TOKEN`. É a única credencial: o token carrega o id da
   * loja, então não existe endpoint, região nem par de chaves para
   * configurar.
   *
   * É passado explicitamente em toda chamada ao SDK, em vez de deixar o
   * `@vercel/blob` cair sozinho no `process.env`. Assim um ambiente sem a
   * variável falha ao construir o cliente, com o nome da variável na
   * mensagem, e não no meio de um upload.
   */
  token: string;
}

/**
 * Formatos de corpo aceitos por `upload`. É um subconjunto de `BodyInit`
 * (fetch) que cobre tudo que os call sites já passam: `File`/`Blob` na web,
 * `Buffer`/`Uint8Array`/`ArrayBuffer` nos workers, e `string` para o CSV de
 * `user-files`.
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
   * Diferente da implementação anterior (HEAD-antes-do-PUT, sujeita a
   * corrida), quem recusa aqui é o próprio Vercel Blob: vira
   * `allowOverwrite: false` na chamada de `put`, checado do lado do
   * servidor, na mesma operação da escrita.
   *
   * @default true
   */
  upsert?: boolean;
}

export interface StorageObjectInfo {
  key: string;
  size: number;
  /** Data em formato ISO 8601. */
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
