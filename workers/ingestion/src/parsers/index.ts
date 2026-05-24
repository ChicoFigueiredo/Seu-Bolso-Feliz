/**
 * @sbf/worker-ingestion/parsers — Módulo de parsing de documentos
 */
export { extractText, PdfPasswordRequiredError, TextExtractionError } from "./text-extractor";
export type { TextExtractResult } from "./text-extractor";
export { findPdfPassword } from "./secret-lookup";
export type { SecretMatch } from "./secret-lookup";
export { isCemig, parseCemig } from "./cemig-parser";
export type { CemigExtractionResult } from "./cemig-parser";
export { parseBoleto } from "./boleto-parser";
export type { BoletoExtractionResult } from "./boleto-parser";
export { parseDocument } from "./parse-orchestrator";
export type { ParseResult } from "./parse-orchestrator";
