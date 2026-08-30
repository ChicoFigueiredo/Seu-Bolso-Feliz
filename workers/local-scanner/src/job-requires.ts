/**
 * Capacidades (`requires`) que um job criado a partir da pasta local exige do
 * worker que for reivindicá-lo.
 *
 * A fila roteia por subconjunto (`requires <@ capabilities`, ver
 * `packages/db/src/queue.ts`): um job cujo `requires` não cabe nas capacidades
 * de nenhum worker fica na fila para sempre, em silêncio, sem erro nem
 * retentativa. Por isso a decisão mora aqui, isolada e testada.
 */
import { extensaoDe, formatoDe } from "@sbf/contracts";

/** Worker com acesso à pasta local vigiada por este scanner. */
export const CAPACIDADE_PASTA_LOCAL = "pasta-local";

/** Worker capaz de rodar OCR (`ocrmypdf` e OCR de imagem). */
export const CAPACIDADE_OCR = "ocr";

/** Worker com as senhas de PDF configuradas. */
export const CAPACIDADE_SENHA_PDF = "senha-pdf";

/**
 * Quantos bytes do fim do arquivo são inspecionados atrás do `/Encrypt`.
 *
 * O trailer (ou o dicionário do XRef stream, no PDF 1.5+) vive no fim do
 * arquivo. Varrer o arquivo inteiro acharia a sequência dentro de qualquer
 * stream comprimido que por acaso a contivesse — o falso positivo ficaria
 * comum em vez de raro.
 */
const BYTES_DE_CAUDA = 8192;

const ENCRYPT = new TextEncoder().encode("/Encrypt");

/**
 * Todo job da pasta local exige `pasta-local`; PDF e imagem exigem também
 * `ocr`, e PDF protegido, `senha-pdf`.
 *
 * O `ocr` está aqui pelo mesmo motivo que está no scanner de Gmail: o mesmo
 * PDF escaneado precisa de OCR venha ele de onde vier, e deixar as duas
 * fontes discordarem sobre o mesmo arquivo é uma armadilha de manutenção —
 * um worker sem OCR reivindicaria o job local e falharia nele, enquanto
 * recusaria corretamente o idêntico vindo do e-mail. Nos dois consumidores
 * configurados hoje (o CLI e o worker da VPS) a exigência não restringe
 * nada, porque ambos já anunciam `ocr`.
 */
export function resolveFileRequires(filename: string, conteudo: Uint8Array): string[] {
  const requires = [CAPACIDADE_PASTA_LOCAL];
  const ehPdf = extensaoDe(filename) === ".pdf";

  // `suporte: "ocr"` no registro de `@sbf/contracts` marca os formatos que só
  // rendem texto com OCR (hoje, as imagens).
  if (ehPdf || formatoDe(filename)?.suporte === "ocr") {
    requires.push(CAPACIDADE_OCR);
  }
  if (ehPdf && pdfProtegidoPorSenha(conteudo)) {
    requires.push(CAPACIDADE_SENHA_PDF);
  }
  return requires;
}

/**
 * Detecta PDF cifrado procurando a chave `/Encrypt` no fim do arquivo.
 *
 * Um PDF com senha precisa declarar `/Encrypt` no trailer — é o único jeito de
 * o leitor saber que há o que decifrar —, então a presença da chave é um sinal
 * direto, sem abrir o documento.
 *
 * Errar para o lado do falso positivo é barato: o job passa a exigir
 * `senha-pdf`, e o worker que lê a pasta local também oferece essa capacidade
 * por padrão. Um falso negativo (arquivo enorme cujo dicionário do XRef stream
 * caia fora da cauda inspecionada) apenas repete o comportamento de hoje — o
 * job segue sem exigência e o parser reclama adiante.
 */
export function pdfProtegidoPorSenha(conteudo: Uint8Array): boolean {
  const inicio = Math.max(0, conteudo.length - BYTES_DE_CAUDA);
  return contemSequencia(conteudo.subarray(inicio), ENCRYPT);
}

/**
 * Capacidades exigidas pelo job que este host não anuncia.
 *
 * Serve de aviso na hora do scan: como a pasta local costuma ser lida pelo
 * mesmo host que processa os jobs dela, uma exigência descoberta aqui e não
 * atendida em lugar nenhum é trabalho criado para ninguém reivindicar.
 */
export function capacidadesNaoAtendidas(
  requires: readonly string[],
  capabilities: readonly string[],
): string[] {
  const oferecidas = new Set(capabilities);
  return requires.filter((capacidade) => !oferecidas.has(capacidade));
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
