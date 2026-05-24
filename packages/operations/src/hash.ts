/**
 * Hash e fingerprint para deduplicação de documentos.
 * Estratégia conforme planejamento 005 seção 2.3.
 */

/**
 * SHA-256 dos bytes brutos do arquivo.
 * Detecta arquivo exatamente igual.
 */
export async function computeContentHash(data: ArrayBuffer | Uint8Array): Promise<string> {
  const buffer = data instanceof Uint8Array ? data : new Uint8Array(data);
  const hashBuffer = await crypto.subtle.digest("SHA-256", buffer);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * SHA-256 do texto extraído normalizado.
 * Detecta documento semanticamente equivalente (mesmo conteúdo em PDFs diferentes).
 */
export async function computeCanonicalFingerprint(extractedText: string): Promise<string> {
  const normalized = extractedText
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim();

  const encoder = new TextEncoder();
  const data = encoder.encode(normalized);
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
