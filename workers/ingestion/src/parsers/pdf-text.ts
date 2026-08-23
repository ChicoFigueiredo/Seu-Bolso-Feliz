/**
 * Extração de texto de PDF sobre `pdfjs-dist`.
 *
 * POR QUE ESTE MÓDULO EXISTE, e não é gosto por biblioteca nova: o
 * `pdf-parse@1.1.1` embute uma build do `pdf.js` v1.10.100 (2018) que falha de
 * forma NÃO DETERMINÍSTICA sob carga. A prova é direta: no mesmo processo, com
 * os mesmos bytes — 1243 bytes, `Buffer.compare` = 0 contra o original — duas
 * tentativas seguidas devolveram erros DIFERENTES:
 *
 *   tentativa 1 → "Command token too long: 128"
 *   tentativa 2 → "bad XRef entry"
 *
 * Entrada igual, resultado diferente: o defeito é estado interno do parser, não
 * o documento. O E2E `document-to-transaction` falhava de forma reprodutível na
 * primeira execução após `supabase db reset` — o momento em que a máquina está
 * mais ocupada, com os containers reiniciando. Um parser sensível a pressão de
 * event loop não é um problema só de teste: sob um worker carregado, ele
 * reprova documentos de verdade.
 *
 * O `pdfjs-dist` já estava no repositório (o `react-pdf` da web depende dele),
 * então isto troca uma build de 2018 por uma mantida, sem dependência nova de
 * fato.
 *
 * SOBRE A RECONSTRUÇÃO DE LINHAS: os parsers determinísticos (boleto, CEMIG,
 * templates de fornecedor) casam expressões regulares contra LINHAS — por
 * exemplo `Vencimento: 15/04/2026`. O `getTextContent` do pdf.js devolve
 * fragmentos soltos com coordenadas, não linhas. Concatenar tudo com espaço
 * destruiria a estrutura de que os parsers dependem. Por isso os fragmentos são
 * agrupados pela coordenada Y e ordenados por X dentro de cada linha.
 */
import { getDocument, PasswordResponses, type TextItem } from "pdfjs-dist/legacy/build/pdf.mjs";

export interface PdfTextResult {
  text: string;
  pages: number;
}

/** Erro de senha, normalizado para o chamador não depender do pdf.js. */
export class PdfPasswordError extends Error {
  constructor(
    message: string,
    /** `true` quando o PDF é protegido e nenhuma senha foi oferecida. */
    readonly needsPassword: boolean,
  ) {
    super(message);
    this.name = "PdfPasswordError";
  }
}

/**
 * Tolerância vertical, em unidades de usuário do PDF, para considerar dois
 * fragmentos como parte da mesma linha.
 *
 * Não é zero de propósito: sobrescritos, acentos posicionados por conta própria
 * e fontes com baselines levemente distintas produzem Y diferentes por frações
 * de ponto dentro de uma mesma linha visual. Um valor de 2 unidades (≈0,7 mm)
 * agrupa esses casos sem colar linhas adjacentes, que num documento financeiro
 * ficam a 10 unidades ou mais.
 */
const MESMA_LINHA_TOLERANCIA = 2;

interface Fragmento {
  x: number;
  y: number;
  texto: string;
}

/** Agrupa fragmentos em linhas por proximidade vertical e ordena por X. */
export function reconstruirLinhas(fragmentos: Fragmento[]): string {
  if (fragmentos.length === 0) return "";

  const ordenados = [...fragmentos].sort((a, b) => b.y - a.y || a.x - b.x);

  const linhas: Fragmento[][] = [];
  let atual: Fragmento[] = [ordenados[0]!];

  for (const frag of ordenados.slice(1)) {
    const referencia = atual[0]!;
    if (Math.abs(referencia.y - frag.y) <= MESMA_LINHA_TOLERANCIA) {
      atual.push(frag);
    } else {
      linhas.push(atual);
      atual = [frag];
    }
  }
  linhas.push(atual);

  return linhas
    .map((linha) =>
      linha
        .sort((a, b) => a.x - b.x)
        .map((f) => f.texto)
        .join("")
        .trim(),
    )
    .join("\n");
}

/**
 * Extrai o texto de um PDF, página a página.
 *
 * `password` vazia ou ausente segue o caminho normal; um PDF protegido levanta
 * `PdfPasswordError` para que o orquestrador tente as senhas cadastradas.
 */
export async function extrairTextoPdf(data: Buffer, password?: string): Promise<PdfTextResult> {
  // O pdf.js assume posse do buffer que recebe e o desanexa durante o parse.
  // Passar o buffer do chamador o deixaria inutilizável para um retry ou para
  // o fallback de OCR — que é justamente o caminho de recuperação.
  const bytes = new Uint8Array(data);

  const loadingTask = getDocument({
    data: bytes,
    password,
    // Node não tem canvas nem DOM: sem isto o pdf.js tenta usar fontes do
    // sistema e emite avisos a cada página.
    useSystemFonts: false,
    // `eval` é desnecessário para extrair texto e é superfície de ataque num
    // pipeline que ingere documentos de terceiros.
    isEvalSupported: false,
  });

  let doc;
  try {
    doc = await loadingTask.promise;
  } catch (err: unknown) {
    const e = err as { name?: string; code?: number; message?: string };
    if (e?.name === "PasswordException") {
      throw new PdfPasswordError(
        e.message ?? "PDF protegido por senha",
        e.code === PasswordResponses.NEED_PASSWORD,
      );
    }
    throw err;
  }

  try {
    const paginas: string[] = [];

    for (let n = 1; n <= doc.numPages; n++) {
      const page = await doc.getPage(n);
      const content = await page.getTextContent();

      const fragmentos: Fragmento[] = content.items
        .filter((item): item is TextItem => "str" in item)
        .map((item) => ({
          x: item.transform[4] as number,
          y: item.transform[5] as number,
          texto: item.str,
        }));

      paginas.push(reconstruirLinhas(fragmentos));
      page.cleanup();
    }

    return { text: paginas.join("\n\n"), pages: doc.numPages };
  } finally {
    // Sem isto, cada documento deixa para trás o worker e os buffers da página.
    // Num worker que processa lotes, isso é o suficiente para crescer sem
    // limite até o processo ser morto pelo OOM killer.
    await doc.destroy();
  }
}
