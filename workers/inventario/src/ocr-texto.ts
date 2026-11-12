/**
 * Lógica pura do OCR de imagens (sem bun:sqlite nem processos): limpeza da
 * saída do tesseract e decisão de "tem texto de verdade". Fica separada de
 * e3-ocr.ts para o vitest (Node) poder testar.
 */

/** Imagens que o e3-ocr tenta ler. */
export const EXTENSOES_OCR: readonly string[] = ["jpg", "jpeg", "png", "webp", "heic"];

/** Menos letras que isso é ruído de foto, não texto. */
export const MIN_LETRAS_OCR = 15;

export const LIMITE_TEXTO_OCR = 20 * 1024;

/** Tira caracteres de controle, espaços de ponta e linhas vazias repetidas. */
export function limparOcr(bruto: string): string {
  return bruto
    .replace(/\r/g, "")
    .replace(/\p{Cc}/gu, (c) => (c === "\n" || c === "\t" ? c : ""))
    .split("\n")
    .map((l) => l.replace(/[ \t]+/g, " ").trim())
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function contarLetras(texto: string): number {
  return (texto.match(/\p{L}/gu) ?? []).length;
}

/** O OCR achou texto suficiente para a triagem reavaliar a imagem? */
export function textoSignificativo(texto: string): boolean {
  return contarLetras(texto) >= MIN_LETRAS_OCR;
}

/** Texto a gravar na tabela `texto`: limpo e cortado em 20 KB; vazio se for ruído. */
export function textoParaGravar(bruto: string): string {
  const limpo = limparOcr(bruto);
  return textoSignificativo(limpo) ? limpo.slice(0, LIMITE_TEXTO_OCR) : "";
}
