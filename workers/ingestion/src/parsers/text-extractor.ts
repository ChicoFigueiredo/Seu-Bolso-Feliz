/**
 * Extração de texto de documentos.
 * Suporta PDF (com e sem senha), CSV e texto plano.
 */
// Import from lib/ to avoid pdf-parse@1.x debug code that runs on top-level import
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import pdfParse from "pdf-parse/lib/pdf-parse.js";

export interface TextExtractResult {
  text: string;
  pages: number;
  mimeType: string;
  wasProtected: boolean;
  extractionMethod: "pdf_native" | "pdf_native_plus_ocrmypdf" | "text_plain" | "image_placeholder";
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

  if (mimeType === "text/csv" || mimeType === "application/xml") {
    return {
      text: buf.toString("utf-8"),
      pages: 1,
      mimeType,
      wasProtected: false,
      extractionMethod: "text_plain",
      ocrApplied: false,
    };
  }

  // Imagens não extraem texto no MVP (futuro: OCR / OpenAI Vision)
  if (mimeType.startsWith("image/")) {
    return {
      text: "",
      pages: 1,
      mimeType,
      wasProtected: false,
      extractionMethod: "image_placeholder",
      ocrApplied: false,
    };
  }

  // Fallback: tentar como texto
  return {
    text: buf.toString("utf-8"),
    pages: 1,
    mimeType,
    wasProtected: false,
    extractionMethod: "text_plain",
    ocrApplied: false,
  };
}

async function extractPdfText(buf: Buffer, password?: string): Promise<TextExtractResult> {
  const options: Record<string, unknown> = {};
  if (password) {
    options.password = password;
  }

  try {
    let result = await pdfParse(buf, options);
    let extractionMethod: TextExtractResult["extractionMethod"] = "pdf_native";
    let ocrApplied = false;

    if (shouldAttemptOcrFallback("application/pdf", result.text)) {
      const ocrBuffer = await runOcrMyPdf(buf);
      if (ocrBuffer) {
        result = await pdfParse(ocrBuffer, options);
        extractionMethod = "pdf_native_plus_ocrmypdf";
        ocrApplied = true;
      }
    }

    return {
      text: result.text,
      pages: result.numpages,
      mimeType: "application/pdf",
      wasProtected: !!password,
      extractionMethod,
      ocrApplied,
    };
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);

    // Detectar PDF protegido que precisa de senha
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
