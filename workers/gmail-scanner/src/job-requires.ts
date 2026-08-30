/**
 * Capacidades (`requires`) que um job criado pelo scanner de Gmail exige do
 * worker que for reivindicá-lo.
 *
 * A fila roteia por subconjunto (`requires <@ capabilities`, ver
 * `packages/db/src/queue.ts`): `requires` vazio é elegível para qualquer
 * worker, e cada capacidade a mais restringe quem pode pegar o job. Exigir de
 * menos manda o arquivo para um worker que não consegue lê-lo; exigir demais
 * deixa o job parado na fila. Por isso a decisão mora aqui, isolada e testada,
 * em vez de espalhada pelo fluxo de upload.
 */
import { extensaoDe, formatoDe } from "@sbf/contracts";

/** Worker capaz de rodar OCR (`ocrmypdf` e OCR de imagem). */
export const CAPACIDADE_OCR = "ocr";

/** O mínimo que o roteamento precisa saber sobre um anexo. */
export interface AnexoParaRoteamento {
  filename: string;
  mimeType: string;
}

/**
 * PDF e imagem entram como `["ocr"]`; todo o resto, como `[]`.
 *
 * Um PDF que chega por e-mail tanto é o boleto com camada de texto quanto o
 * comprovante escaneado, e o scanner não abre o arquivo para descobrir qual
 * dos dois é. Exigir OCR de todo PDF erra para o lado seguro: o job vai para
 * um worker que dá conta dos dois casos, em vez de parar num que só lê texto.
 *
 * CSV, XLSX, OFX e XML têm parser próprio e não dependem de OCR — pedir a
 * capacidade neles só reduziria à toa o conjunto de workers elegíveis.
 */
export function resolveAttachmentRequires(anexo: AnexoParaRoteamento): string[] {
  return precisaDeOcr(anexo) ? [CAPACIDADE_OCR] : [];
}

/**
 * Corpo de e-mail é texto puro extraído pelo próprio scanner — chega ao
 * storage já como `.txt`, sem nada para reconhecer opticamente.
 */
export function resolveEmailBodyRequires(): string[] {
  return [];
}

function precisaDeOcr({ filename, mimeType }: AnexoParaRoteamento): boolean {
  // O MIME vem primeiro por ser o que o Gmail declara; a extensão é o
  // desempate para o caso corriqueiro do anexo servido como
  // `application/octet-stream`.
  const mime = (mimeType ?? "").toLowerCase().split(";")[0]?.trim() ?? "";
  if (mime === "application/pdf" || mime.startsWith("image/")) return true;

  if (extensaoDe(filename) === ".pdf") return true;
  // `suporte: "ocr"` no registro de `@sbf/contracts` marca os formatos que só
  // rendem texto com OCR (hoje, as imagens).
  return formatoDe(filename)?.suporte === "ocr";
}
