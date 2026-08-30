/**
 * Hash e fingerprint para deduplicação de documentos.
 * Estratégia conforme planejamento 005 seção 2.3.
 */

/**
 * SHA-256 dos bytes brutos do arquivo.
 * Detecta arquivo exatamente igual.
 *
 * `ArrayBufferView<ArrayBuffer>` (em vez do `ArrayBufferView` genérico) é de
 * propósito, mesmo motivo de `UploadBody` em `@sbf/storage`: a partir do TS 5.7
 * o `BufferSource` que a Web Crypto aceita exclui views sobre
 * `SharedArrayBuffer` — `crypto.subtle.digest` de fato recusa memória
 * compartilhada. `Uint8Array`/`Buffer` vindos de `readFile`, `Buffer.from`,
 * `TextEncoder.encode` ou `new Uint8Array(...)` sempre se qualificam.
 */
export async function computeContentHash(
  data: ArrayBuffer | ArrayBufferView<ArrayBuffer>,
): Promise<string> {
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * SHA-256 do texto extraído normalizado.
 * Detecta documento semanticamente equivalente (mesmo conteúdo em PDFs diferentes).
 */
export async function computeCanonicalFingerprint(extractedText: string): Promise<string> {
  const normalized = extractedText.toLowerCase().replace(/\s+/g, " ").trim();

  const encoder = new TextEncoder();
  const data = encoder.encode(normalized);
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
