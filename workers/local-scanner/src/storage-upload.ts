/**
 * Envio do arquivo original para o bucket de ingestão.
 *
 * O cliente antigo devolvia `{ error }` e o `@sbf/storage` **lança**
 * `StorageError`. O scanner, porém, sempre tratou falha de upload como "loga e
 * segue para o próximo arquivo", nunca como aborto do scan — a exceção é
 * convertida de volta num resultado aqui, num lugar só, em vez de virar
 * try/catch espalhado pelo fluxo (e com risco de um `return` esquecido deixar
 * passar um documento sem arquivo no storage).
 */
import { StorageError, type UploadBody, type UploadOptions } from "@sbf/storage";

/** Bucket dos arquivos como chegaram, antes de qualquer processamento. */
export const BUCKET_ORIGINAIS = "ingestion-originals";

/**
 * Só a fatia do `StorageClient` que o upload usa.
 *
 * Declarar o mínimo permite passar um dublê nos testes sem falar com a loja
 * de blobs de verdade.
 */
export interface DestinoDeUpload {
  from(bucket: string): {
    upload(path: string, data: UploadBody, options?: UploadOptions): Promise<void>;
  };
}

export type ResultadoDeUpload = { ok: true } | { ok: false; erro: string };

export async function uploadOriginal(
  storage: DestinoDeUpload,
  path: string,
  data: UploadBody,
  contentType: string,
): Promise<ResultadoDeUpload> {
  try {
    await storage.from(BUCKET_ORIGINAIS).upload(path, data, { contentType });
    return { ok: true };
  } catch (err) {
    return { ok: false, erro: mensagemDeErro(err) };
  }
}

/**
 * Texto legível de qualquer erro.
 *
 * `StorageError` ganha o código e o status na mensagem: a mensagem sozinha
 * ("Access denied") não diz se o problema é credencial, caminho ou loja
 * suspensa — o `access_denied 403` junto resolve isso na primeira leitura do
 * log.
 */
export function mensagemDeErro(err: unknown): string {
  if (err instanceof StorageError) {
    const detalhe = [err.code, err.status].filter(Boolean).join(" ");
    return detalhe ? `${err.message} (${detalhe})` : err.message;
  }
  if (err instanceof Error) return err.message;
  return String(err);
}
