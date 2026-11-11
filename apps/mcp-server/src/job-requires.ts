/**
 * Capacidades (`requires`) que um job de ingestão criado pelo MCP server
 * exige do worker que for reivindicá-lo.
 *
 * A fila roteia por subconjunto (`requires <@ capabilities`, ver
 * `packages/db/src/queue.ts`): um job cujo `requires` não cabe nas
 * capacidades de nenhum worker fica na fila para sempre, em silêncio, sem
 * erro nem retentativa. Versão reduzida da mesma ideia em
 * `workers/local-scanner/src/job-requires.ts` — duplicada de propósito (cada
 * serviço decide sozinho o que exige) — sem a checagem de "capacidades não
 * atendidas", que aqui não tem call site (o MCP não anuncia capacidades
 * próprias, só cria jobs para os workers reais reivindicarem).
 */
import { extensaoDe, formatoDe } from "@sbf/contracts";

/** Worker capaz de rodar OCR (`ocrmypdf` e OCR de imagem). */
export const CAPACIDADE_OCR = "ocr";

/** Worker com as senhas de PDF configuradas. */
export const CAPACIDADE_SENHA_PDF = "senha-pdf";

/**
 * Quantos bytes do fim do arquivo são inspecionados atrás do `/Encrypt`.
 * O trailer (ou o dicionário do XRef stream, no PDF 1.5+) vive no fim do
 * arquivo — variar o arquivo inteiro acharia a sequência dentro de qualquer
 * stream comprimido que por acaso a contivesse.
 */
const BYTES_DE_CAUDA = 8192;
const ENCRYPT = new TextEncoder().encode("/Encrypt");

/**
 * Capacidades exigidas por um arquivo, a partir do nome (extensão) e,
 * opcionalmente, do conteúdo (para detectar PDF protegido por senha).
 *
 * PDF sempre exige `ocr` — pode ser um PDF escaneado sem camada de texto — e
 * qualquer formato marcado `suporte: "ocr"` no registro de `@sbf/contracts`
 * (hoje, as imagens). A detecção de senha só roda quando `conteudo` é
 * informado: `scan_local_folder` já tem os bytes em mãos (lidos para o
 * upload); `reprocess_document`/`ingest_document` só conhecem o nome do
 * arquivo, então essa exigência fica de fora nesses dois — o parser
 * descobre a senha faltando do jeito de sempre, só sem o aviso adiantado.
 */
export function resolveRequires(filename: string, conteudo?: Uint8Array): string[] {
  const requires: string[] = [];
  const ehPdf = extensaoDe(filename) === ".pdf";

  if (ehPdf || formatoDe(filename)?.suporte === "ocr") {
    requires.push(CAPACIDADE_OCR);
  }
  if (ehPdf && conteudo && pdfProtegidoPorSenha(conteudo)) {
    requires.push(CAPACIDADE_SENHA_PDF);
  }
  return requires;
}

/**
 * Detecta PDF cifrado procurando a chave `/Encrypt` no fim do arquivo. Errar
 * para o lado do falso positivo é barato (o job só passa a exigir
 * `senha-pdf` de um worker que já oferece essa capacidade por padrão).
 */
function pdfProtegidoPorSenha(conteudo: Uint8Array): boolean {
  const inicio = Math.max(0, conteudo.length - BYTES_DE_CAUDA);
  return contemSequencia(conteudo.subarray(inicio), ENCRYPT);
}

function contemSequencia(alvo: Uint8Array, agulha: Uint8Array): boolean {
  if (agulha.length === 0 || alvo.length < agulha.length) return false;
  const ultimo = alvo.length - agulha.length;
  for (let i = 0; i <= ultimo; i++) {
    let bateu = true;
    for (let j = 0; j < agulha.length; j++) {
      if (alvo[i + j] !== agulha[j]) {
        bateu = false;
        break;
      }
    }
    if (bateu) return true;
  }
  return false;
}
