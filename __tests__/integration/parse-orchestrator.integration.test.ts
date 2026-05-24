import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../workers/ingestion/src/parsers/text-extractor", () => ({
  extractText: vi.fn(),
  PdfPasswordRequiredError: class PdfPasswordRequiredError extends Error {},
}));

vi.mock("../../workers/ingestion/src/parsers/cemig-parser", () => ({
  isCemig: vi.fn(() => false),
  parseCemig: vi.fn(),
}));

vi.mock("../../workers/ingestion/src/parsers/boleto-parser", () => ({
  parseBoleto: vi.fn(),
}));

vi.mock("../../workers/ingestion/src/parsers/supplier-templates", () => ({
  applySupplierTemplate: vi.fn(),
}));

vi.mock("../../workers/ingestion/src/parsers/boleto-utils-extractor", () => ({
  extractWithBoletoUtils: vi.fn(),
}));

vi.mock("../../workers/ingestion/src/parsers/pattern-matcher", () => ({
  findMatchingPattern: vi.fn(async () => null),
}));

vi.mock("../../workers/ingestion/src/logger", () => ({
  writeLog: vi.fn(async () => undefined),
}));

import { parseDocument } from "../../workers/ingestion/src/parsers/parse-orchestrator";
import { extractText } from "../../workers/ingestion/src/parsers/text-extractor";
import { parseBoleto } from "../../workers/ingestion/src/parsers/boleto-parser";
import { applySupplierTemplate } from "../../workers/ingestion/src/parsers/supplier-templates";
import { extractWithBoletoUtils } from "../../workers/ingestion/src/parsers/boleto-utils-extractor";

type InsertPayload = Record<string, unknown>;

function createSupabaseMock() {
  const captures: Record<string, InsertPayload[]> = {
    parsed_document_versions: [],
    extraction_results: [],
  };

  const tableState = new Map<string, { action: "select" | "insert"; payload?: InsertPayload }>();

  const api = {
    from(table: string) {
      tableState.set(table, { action: "select" });
      const chain = {
        select() {
          tableState.set(table, { action: "select" });
          return chain;
        },
        insert(payload: InsertPayload) {
          captures[table]?.push(payload);
          tableState.set(table, { action: "insert", payload });
          return chain;
        },
        eq() {
          return chain;
        },
        order() {
          return chain;
        },
        limit() {
          return chain;
        },
        single() {
          const state = tableState.get(table);
          if (state?.action === "select") {
            if (table === "parsed_document_versions") {
              return Promise.resolve({ data: { version_number: 0 }, error: null });
            }
            return Promise.resolve({ data: null, error: null });
          }

          if (table === "parsed_document_versions") {
            return Promise.resolve({ data: { id: "pv-1" }, error: null });
          }
          if (table === "extraction_results") {
            return Promise.resolve({ data: { id: "er-1" }, error: null });
          }

          return Promise.resolve({ data: { id: "id-1" }, error: null });
        },
      };
      return chain;
    },
  };

  return { api, captures };
}

describe("Integração: parse-orchestrator com OCR + consenso", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("persiste metadata de OCR local quando extração usa fallback OCR", async () => {
    vi.mocked(extractText).mockResolvedValue({
      text: "VENCIMENTO: 20/05/2026\nVALOR DO DOCUMENTO: R$ 100,00",
      pages: 1,
      mimeType: "application/pdf",
      wasProtected: false,
      extractionMethod: "pdf_native_plus_ocrmypdf",
      ocrApplied: true,
    });

    vi.mocked(parseBoleto).mockReturnValue({
      supplierNameRaw: "Fornecedor OCR",
      supplierCnpj: null,
      competenceDate: null,
      dueDate: "2026-05-20",
      totalAmount: 100,
      documentNumber: null,
      barcodeDigitableLine: null,
      confidence: 0.7,
    });

    vi.mocked(applySupplierTemplate).mockReturnValue(null);
    vi.mocked(extractWithBoletoUtils).mockResolvedValue(null);

    const { api, captures } = createSupabaseMock();

    await parseDocument(
      {
        supabase: api as never,
        ctx: { userId: "u1", runId: "r1", jobId: "j1" },
        userId: "u1",
        sourceDocumentId: "doc-1",
        mimeType: "application/pdf",
      },
      new TextEncoder().encode("fake-pdf").buffer,
    );

    const parsedPayload = captures.parsed_document_versions[0];
    expect(parsedPayload).toBeTruthy();
    const metadata = parsedPayload?.metadata as Record<string, unknown>;
    expect(metadata.extraction_method).toBe("pdf_native_plus_ocrmypdf");
    expect(metadata.ocr_applied).toBe(true);
  });

  it("aplica precedência do consenso: boleto_utils > template > parser", async () => {
    vi.mocked(extractText).mockResolvedValue({
      text: "boleto teste",
      pages: 1,
      mimeType: "application/pdf",
      wasProtected: false,
      extractionMethod: "pdf_native",
      ocrApplied: false,
    });

    vi.mocked(parseBoleto).mockReturnValue({
      supplierNameRaw: "Nome Parser",
      supplierCnpj: "11111111000111",
      competenceDate: "2026-05-01",
      dueDate: "2026-05-10",
      totalAmount: 120,
      documentNumber: "DOC-PARSER",
      barcodeDigitableLine: "111",
      confidence: 0.8,
    });

    vi.mocked(applySupplierTemplate).mockReturnValue({
      templateId: "tpl-vivo-telecom-v1",
      supplierNameRaw: "Nome Template",
      supplierCnpj: "22222222000122",
      totalAmount: 100,
      dueDate: "2026-05-15",
      competenceDate: "2026-05-01",
      documentNumber: "DOC-TPL",
      confidence: 0.88,
      matchedRules: ["supplier_hint", "total_amount"],
    });

    vi.mocked(extractWithBoletoUtils).mockResolvedValue({
      supplierNameRaw: "BOLETO BANCARIO",
      dueDate: "2026-05-20",
      totalAmount: 95,
      barcodeDigitableLine: "23794150099001980167035000211405700000000000000",
      boletoType: "BANCO",
      confidence: 0.99,
      reason: "validated_by_boleto_utils",
    });

    const { api, captures } = createSupabaseMock();

    await parseDocument(
      {
        supabase: api as never,
        ctx: { userId: "u1", runId: "r1", jobId: "j1" },
        userId: "u1",
        sourceDocumentId: "doc-2",
        mimeType: "application/pdf",
      },
      new TextEncoder().encode("fake-pdf").buffer,
    );

    const extractionPayload = captures.extraction_results[0];
    expect(extractionPayload).toBeTruthy();
    expect(extractionPayload?.supplier_name_raw).toBe("BOLETO BANCARIO");
    expect(extractionPayload?.total_amount).toBe(95);
    expect(extractionPayload?.due_date).toBe("2026-05-20");

    const metadata = extractionPayload?.metadata as Record<string, unknown>;
    const sourcePerField = metadata.source_per_field as Record<string, string>;
    expect(sourcePerField.total_amount).toBe("boleto_utils");
    expect(sourcePerField.due_date).toBe("boleto_utils");
    expect(sourcePerField.supplier_name_raw).toBe("boleto_utils");
  });
});
