/**
 * Extração de texto de documentos.
 * Suporta PDF (com e sem senha), CSV e texto plano.
 */
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { extrairTextoPdf, PdfPasswordError } from "./pdf-text";
import {
  csvParaTexto,
  decodificarTexto,
  ofxParaTexto,
  parseCsv,
  parseOfx,
  xlsxParaTexto,
} from "./tabular";

export interface TextExtractResult {
  text: string;
  pages: number;
  mimeType: string;
  wasProtected: boolean;
  extractionMethod:
    | "pdf_native"
    | "pdf_native_plus_ocrmypdf"
    | "text_plain"
    | "image_placeholder"
    | "image_ocr"
    | "spreadsheet"
    | "ofx"
    | "csv";
  ocrApplied: boolean;
}

export const OCR_MIN_TEXT_LENGTH = 80;

export function shouldAttemptOcrFallback(mimeType: string, text: string): boolean {
  if (mimeType !== "application/pdf") return false;
  if (process.env.INGESTION_ENABLE_OCRMYPDF !== "true") return false;
  return text.trim().length < OCR_MIN_TEXT_LENGTH;
}

/**
 * Extrai texto de um buffer de documento.
 * Para PDFs protegidos, tenta com a senha fornecida.
 */
export async function extractText(
  data: ArrayBuffer | Buffer,
  mimeType: string,
  password?: string,
): Promise<TextExtractResult> {
  const buf = Buffer.isBuffer(data) ? data : Buffer.from(data);

  if (mimeType === "application/pdf") {
    return extractPdfText(buf, password);
  }

  if (ehPlanilha(mimeType)) {
    return {
      text: await xlsxParaTexto(buf),
      pages: 1,
      mimeType,
      wasProtected: false,
      extractionMethod: "spreadsheet",
      ocrApplied: false,
    };
  }

  const texto = decodificarTexto(buf);

  // OFX é reconhecido pelo conteúdo, não pelo MIME: bancos servem o arquivo
  // como `application/octet-stream` com frequência, e o scanner registra
  // exatamente isso. Confiar só no MIME deixaria todo extrato OFX cair no
  // caminho de texto puro.
  if (ehOfx(mimeType, texto)) {
    return {
      text: ofxParaTexto(parseOfx(texto)),
      pages: 1,
      mimeType,
      wasProtected: false,
      extractionMethod: "ofx",
      ocrApplied: false,
    };
  }

  if (mimeType === "text/csv") {
    const tabela = parseCsv(texto);

    // Só converte o que é tabela de verdade. `delimitador: null` significa
    // "texto corrido salvo com extensão .csv" — comum em conta de luz
    // exportada —, e forçá-lo a virar tabela quebraria `R$ 245,50` em dois
    // campos. Sem linha de dados também não há o que converter.
    if (tabela.delimitador !== null && tabela.linhas.length > 0) {
      return {
        text: csvParaTexto(tabela),
        pages: 1,
        mimeType,
        wasProtected: false,
        extractionMethod: "csv",
        ocrApplied: false,
      };
    }
  }

  // Imagens não extraem texto sem OCR — ver `INGESTION_ENABLE_IMAGE_OCR`.
  if (mimeType.startsWith("image/")) {
    return extrairTextoDeImagem(buf, mimeType);
  }

  // Fallback: texto puro, agora com detecção de codificação. Antes era
  // `buf.toString("utf-8")` cru, o que transformava todo acento de arquivo
  // latin-1 em U+FFFD e fazia o alias do fornecedor deixar de casar.
  return {
    text: texto,
    pages: 1,
    mimeType,
    wasProtected: false,
    extractionMethod: "text_plain",
    ocrApplied: false,
  };
}

const MIMES_DE_PLANILHA = new Set([
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.ms-excel",
]);

function ehPlanilha(mimeType: string): boolean {
  return MIMES_DE_PLANILHA.has(mimeType);
}

/**
 * OFX 1.x abre com `OFXHEADER:`; o 2.x é XML com a raiz `<OFX>`. Os dois cabem
 * nos primeiros bytes, então a checagem é barata e não depende do MIME.
 */
function ehOfx(mimeType: string, texto: string): boolean {
  if (mimeType === "application/x-ofx") return true;
  const inicio = texto.slice(0, 512).toUpperCase();
  return inicio.includes("OFXHEADER") || /<OFX[\s>]/.test(inicio);
}

async function extractPdfText(buf: Buffer, password?: string): Promise<TextExtractResult> {
  try {
    let result = await extrairTextoPdf(buf, password);
    let extractionMethod: TextExtractResult["extractionMethod"] = "pdf_native";
    let ocrApplied = false;

    if (shouldAttemptOcrFallback("application/pdf", result.text)) {
      const ocrBuffer = await runOcrMyPdf(buf);
      if (ocrBuffer) {
        result = await extrairTextoPdf(ocrBuffer, password);
        extractionMethod = "pdf_native_plus_ocrmypdf";
        ocrApplied = true;
      }
    }

    return {
      text: result.text,
      pages: result.pages,
      mimeType: "application/pdf",
      wasProtected: !!password,
      extractionMethod,
      ocrApplied,
    };
  } catch (err: unknown) {
    // O pdf.js distingue "precisa de senha" de "a senha está errada", e essa
    // diferença importa: sem senha, o orquestrador vai buscar as cadastradas;
    // com senha errada, ele passa para a próxima candidata.
    if (err instanceof PdfPasswordError && err.needsPassword && !password) {
      throw new PdfPasswordRequiredError();
    }

    const msg = err instanceof Error ? err.message : String(err);
    if (isPasswordError(msg) && !password) {
      throw new PdfPasswordRequiredError();
    }

    throw new TextExtractionError(`Falha na extração de texto do PDF: ${msg}`);
  }
}

async function runOcrMyPdf(inputBuffer: Buffer): Promise<Buffer | null> {
  const workdir = await mkdtemp(join(tmpdir(), "sbf-ocrmypdf-"));
  const inputPath = join(workdir, "input.pdf");
  const outputPath = join(workdir, "output.pdf");
  const cmd = process.env.OCRMYPDF_BIN ?? "ocrmypdf";

  try {
    await writeFile(inputPath, inputBuffer);
    const args = ["--skip-text", "--force-ocr", "--quiet", inputPath, outputPath];
    const exitCode = await runProcess(cmd, args);
    if (exitCode !== 0) return null;
    return await readFile(outputPath);
  } catch {
    return null;
  } finally {
    await rm(workdir, { recursive: true, force: true });
  }
}

function runProcess(command: string, args: string[]): Promise<number> {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      stdio: "ignore",
    });

    child.on("error", () => resolve(1));
    child.on("close", (code) => resolve(code ?? 1));
  });
}

/** Idioma do OCR. `por+eng` cobre boleto em português com termos em inglês. */
const OCR_IDIOMA_PADRAO = "por+eng";

export function ocrDeImagemHabilitado(): boolean {
  return process.env.INGESTION_ENABLE_IMAGE_OCR === "true";
}

/**
 * OCR de imagem via tesseract.
 *
 * Antes, toda imagem devolvia `extractionMethod: "image_placeholder"` com texto
 * VAZIO — e sem erro. O documento atravessava o pipeline inteiro, gerava um
 * `extraction_result` sem campo algum e parava em revisão sem explicação, como
 * se o parser tivesse tentado e não achado nada. Fotografar um boleto com o
 * celular é o caminho mais natural que existe para quem usa o sistema pelo
 * telefone, e era exatamente o que não funcionava.
 *
 * Fica DESLIGADO por padrão, como o OCR de PDF: depende de um binário externo
 * (`sudo apt install tesseract-ocr tesseract-ocr-por`) e ligá-lo sozinho faria
 * o worker falhar em máquina sem ele. Quando desligado — ou quando o binário
 * não existe — o comportamento antigo é preservado, mas agora com o motivo
 * registrado no resultado em vez de silêncio.
 */
async function extrairTextoDeImagem(buf: Buffer, mimeType: string): Promise<TextExtractResult> {
  const placeholder: TextExtractResult = {
    text: "",
    pages: 1,
    mimeType,
    wasProtected: false,
    extractionMethod: "image_placeholder",
    ocrApplied: false,
  };

  if (!ocrDeImagemHabilitado()) return placeholder;

  const workdir = await mkdtemp(join(tmpdir(), "sbf-ocr-imagem-"));
  const inputPath = join(workdir, "entrada");
  const outputBase = join(workdir, "saida");
  const cmd = process.env.TESSERACT_BIN ?? "tesseract";
  const idioma = process.env.INGESTION_OCR_LANG ?? OCR_IDIOMA_PADRAO;

  try {
    await writeFile(inputPath, buf);
    // O tesseract acrescenta `.txt` ao caminho de saída por conta própria.
    const exitCode = await runProcess(cmd, [inputPath, outputBase, "-l", idioma]);
    if (exitCode !== 0) return placeholder;

    const texto = await readFile(`${outputBase}.txt`, "utf-8");
    if (texto.trim().length === 0) return placeholder;

    return {
      text: texto,
      pages: 1,
      mimeType,
      wasProtected: false,
      extractionMethod: "image_ocr",
      ocrApplied: true,
    };
  } catch {
    return placeholder;
  } finally {
    await rm(workdir, { recursive: true, force: true });
  }
}

function isPasswordError(message: string): boolean {
  const patterns = [
    "password",
    "encrypted",
    "PasswordException",
    "Password required",
    "Incorrect Password",
  ];
  return patterns.some((p) => message.toLowerCase().includes(p.toLowerCase()));
}

export class PdfPasswordRequiredError extends Error {
  constructor() {
    super("PDF protegido por senha. Senha necessária para extração.");
    this.name = "PdfPasswordRequiredError";
  }
}

export class TextExtractionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TextExtractionError";
  }
}
