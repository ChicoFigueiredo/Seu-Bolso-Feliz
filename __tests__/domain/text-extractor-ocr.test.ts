import { describe, expect, it } from "vitest";
import {
  OCR_MIN_TEXT_LENGTH,
  shouldAttemptOcrFallback,
} from "../../workers/ingestion/src/parsers/text-extractor";

describe("text-extractor OCR fallback heuristics", () => {
  it("não tenta OCR para MIME diferente de PDF", () => {
    process.env.INGESTION_ENABLE_OCRMYPDF = "true";
    const should = shouldAttemptOcrFallback("text/csv", "");
    expect(should).toBe(false);
  });

  it("não tenta OCR quando feature flag está desativada", () => {
    process.env.INGESTION_ENABLE_OCRMYPDF = "false";
    const should = shouldAttemptOcrFallback("application/pdf", "texto curto");
    expect(should).toBe(false);
  });

  it("tenta OCR para PDF com texto insuficiente quando flag ativa", () => {
    process.env.INGESTION_ENABLE_OCRMYPDF = "true";
    const lowText = "x".repeat(OCR_MIN_TEXT_LENGTH - 1);
    const should = shouldAttemptOcrFallback("application/pdf", lowText);
    expect(should).toBe(true);
  });

  it("não tenta OCR para PDF com texto já suficiente", () => {
    process.env.INGESTION_ENABLE_OCRMYPDF = "true";
    const goodText = "x".repeat(OCR_MIN_TEXT_LENGTH + 10);
    const should = shouldAttemptOcrFallback("application/pdf", goodText);
    expect(should).toBe(false);
  });
});
