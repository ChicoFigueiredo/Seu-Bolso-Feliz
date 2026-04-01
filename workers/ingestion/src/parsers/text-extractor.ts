/**
 * Extração de texto de documentos.
 * Suporta PDF (com e sem senha), CSV e texto plano.
 */
// Import from lib/ to avoid pdf-parse@1.x debug code that runs on top-level import
import pdfParse from "pdf-parse/lib/pdf-parse.js";

export interface TextExtractResult {
  text: string;
  pages: number;
  mimeType: string;
  wasProtected: boolean;
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
    };
  }

  // Imagens não extraem texto no MVP (futuro: OCR / OpenAI Vision)
  if (mimeType.startsWith("image/")) {
    return {
      text: "",
      pages: 1,
      mimeType,
      wasProtected: false,
    };
  }

  // Fallback: tentar como texto
  return {
    text: buf.toString("utf-8"),
    pages: 1,
    mimeType,
    wasProtected: false,
  };
}

async function extractPdfText(buf: Buffer, password?: string): Promise<TextExtractResult> {
  const options: Record<string, unknown> = {};
  if (password) {
    options.password = password;
  }

  try {
    const result = await pdfParse(buf, options);
    return {
      text: result.text,
      pages: result.numpages,
      mimeType: "application/pdf",
      wasProtected: !!password,
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
