/**
 * Testes de integração: orquestrador de parsing (`parseDocument`).
 *
 * O que está sob teste é a **persistência decidida pelo orquestrador**: qual
 * metadata de extração ele grava em `parsed_document_versions` e qual valor de
 * cada campo ele grava em `extraction_results` quando três fontes
 * determinísticas discordam entre si.
 *
 * Por isso os parsers continuam mockados (`extractText`, `parseBoleto`,
 * `applySupplierTemplate`, `extractWithBoletoUtils`, `isCemig`): eles são a
 * ENTRADA do orquestrador, e fixá-los é o que torna a decisão observável. Cada
 * um tem seus próprios testes de unidade.
 *
 * O banco, ao contrário, **não** é mockado. A versão anterior deste arquivo
 * montava um dublê do query builder anterior e afirmava sobre o payload
 * capturado por ele — ou seja, provava que o dublê tinha guardado o objeto que
 * acabara de receber, e não que o Postgres aceitaria aquela linha. Depois da
 * ADR-009 (`@sbf/db`/Drizzle) o dublê parou até de encaixar na assinatura, e
 * refazê-lo em Drizzle repetiria o mesmo vício. Agora as asserções releem as
 * duas tabelas do banco real, o que passa a cobrir de graça o que o dublê
 * escondia: os CHECKs (`parser_type`), as FKs, os tipos das colunas
 * (`numeric(12,2)`, `date`), o `user_id` de cada linha — sem RLS, o filtro por
 * usuário é do código — e a serialização do jsonb.
 *
 * `writeLog` segue mockado porque logging não é o assunto deste arquivo e
 * `ingestion_logs.run_id`/`job_id` exigiriam semear uma run e um job inteiros
 * só para satisfazer FK; o conteúdo dos logs do pipeline já é verificado em
 * `ingestion-pipeline.test.ts`.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { beforeAll, afterAll, beforeEach, describe, expect, it, vi } from "vitest";

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

import { sql } from "drizzle-orm";
import type { Database } from "@sbf/db";
import { ParserType } from "@sbf/ingestion-types";
import { parseDocument } from "../../workers/ingestion/src/parsers/parse-orchestrator";
import { extractText } from "../../workers/ingestion/src/parsers/text-extractor";
import { parseBoleto } from "../../workers/ingestion/src/parsers/boleto-parser";
import { applySupplierTemplate } from "../../workers/ingestion/src/parsers/supplier-templates";
import { extractWithBoletoUtils } from "../../workers/ingestion/src/parsers/boleto-utils-extractor";
import { getTestDb, limparPorUsuario, linha, semBancoDeTeste } from "../helpers/db";

// UUID fixo e exclusivo deste arquivo — os arquivos de integração dividem o
// mesmo Postgres (ver `__tests__/helpers/db.ts`).
const USER_ID = "80000000-0000-4000-8000-000000000001";

/** Filhas antes das pais. */
const TABELAS = ["extraction_results", "parsed_document_versions", "source_documents"];

let db: Database;

/**
 * Cria o `source_documents` que a FK de `parsed_document_versions` exige.
 * O conteúdo do arquivo é irrelevante: `extractText` está mockado, e o
 * orquestrador só usa o id.
 */
async function semearDocumento(originKey: string): Promise<string> {
  const row = await linha<{ id: string }>(
    db,
    sql`insert into public.source_documents
          (user_id, origin_type, origin_key, filename, mime_type, status)
        values (${USER_ID}::uuid, 'manual_upload', ${originKey}, ${originKey},
                'application/pdf', 'parsing')
        returning id`,
  );
  return row!.id;
}

/** Chama o orquestrador do jeito que o processador chama em produção. */
async function rodarParse(sourceDocumentId: string) {
  return parseDocument(
    {
      db,
      ctx: { userId: USER_ID },
      userId: USER_ID,
      sourceDocumentId,
      mimeType: "application/pdf",
    },
    new TextEncoder().encode("fake-pdf").buffer,
  );
}

describe.skipIf(semBancoDeTeste)("orquestrador de parsing", () => {
  beforeAll(() => {
    db = getTestDb();
  });

  beforeEach(async () => {
    vi.clearAllMocks();
    await limparPorUsuario(db, TABELAS, USER_ID);
  });

  afterAll(async () => {
    await limparPorUsuario(db, TABELAS, USER_ID);
  });

  it("persiste metadata de OCR local quando a extração usa o fallback OCR", async () => {
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

    const sourceDocumentId = await semearDocumento("orq:ocr");
    const resultado = await rodarParse(sourceDocumentId);

    // Filtrar por `user_id` faz parte da asserção: sem RLS, achar a linha aqui
    // é o que prova que o orquestrador gravou o dono certo.
    const versao = await linha<{
      id: string;
      version_number: number;
      parser_type: string;
      raw_text: string;
      metadata: {
        pages: number;
        was_protected: boolean;
        text_length: number;
        extraction_method: string;
        ocr_applied: boolean;
      };
    }>(
      db,
      sql`select id, version_number, parser_type, raw_text, metadata
            from public.parsed_document_versions
           where user_id = ${USER_ID}::uuid
             and source_document_id = ${sourceDocumentId}::uuid`,
    );

    expect(versao).toBeTruthy();
    // O id devolvido é o da linha que existe no banco, e não um id inventado.
    expect(versao!.id).toBe(resultado.parsedVersionId);
    expect(versao!.version_number).toBe(1);
    expect(versao!.parser_type).toBe(ParserType.LOCAL_REGEX);
    expect(versao!.raw_text).toContain("VALOR DO DOCUMENTO");

    expect(versao!.metadata.extraction_method).toBe("pdf_native_plus_ocrmypdf");
    expect(versao!.metadata.ocr_applied).toBe(true);
    expect(versao!.metadata.was_protected).toBe(false);
    expect(versao!.metadata.pages).toBe(1);
  });

  it("aplica a precedência do consenso: boleto_utils > template > parser", async () => {
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

    const sourceDocumentId = await semearDocumento("orq:consenso");
    const resultado = await rodarParse(sourceDocumentId);

    // O join amarra a extração ao documento semeado: sem ele, uma linha órfã de
    // outra execução passaria por resultado deste teste.
    const extracao = await linha<{
      id: string;
      parsed_version_id: string;
      supplier_name_raw: string;
      total_amount: string;
      due_date: string;
      competence_date: string;
      document_number: string;
      metadata: {
        source_per_field: Record<string, string>;
        deterministic_extras: { supplier_cnpj: string; barcode_digitable_line: string };
      };
    }>(
      db,
      sql`select er.id, er.parsed_version_id, er.supplier_name_raw,
                 er.total_amount::text as total_amount,
                 er.due_date::text as due_date,
                 er.competence_date::text as competence_date,
                 er.document_number, er.metadata
            from public.extraction_results er
            join public.parsed_document_versions pv on pv.id = er.parsed_version_id
           where er.user_id = ${USER_ID}::uuid
             and pv.source_document_id = ${sourceDocumentId}::uuid`,
    );

    expect(extracao).toBeTruthy();
    expect(extracao!.id).toBe(resultado.extractionResultId);
    expect(extracao!.parsed_version_id).toBe(resultado.parsedVersionId);

    // ── Topo da precedência: boleto_utils vence template e parser ──
    expect(extracao!.supplier_name_raw).toBe("BOLETO BANCARIO");
    expect(Number(extracao!.total_amount)).toBe(95);
    expect(extracao!.due_date).toBe("2026-05-20");
    expect(extracao!.metadata.deterministic_extras.barcode_digitable_line).toBe(
      "23794150099001980167035000211405700000000000000",
    );

    // ── Degrau do meio: onde boleto_utils não opinou, o template vence o parser ──
    // A versão anterior só checava o degrau de cima, apesar do nome do teste.
    expect(extracao!.document_number).toBe("DOC-TPL");
    expect(extracao!.competence_date).toBe("2026-05-01");
    expect(extracao!.metadata.deterministic_extras.supplier_cnpj).toBe("22222222000122");

    const sourcePerField = extracao!.metadata.source_per_field;
    expect(sourcePerField.total_amount).toBe("boleto_utils");
    expect(sourcePerField.due_date).toBe("boleto_utils");
    expect(sourcePerField.supplier_name_raw).toBe("boleto_utils");
    expect(sourcePerField.barcode_digitable_line).toBe("boleto_utils");
    expect(sourcePerField.document_number).toBe("supplier_template");
    expect(sourcePerField.supplier_cnpj).toBe("supplier_template");
    expect(sourcePerField.competence_date).toBe("supplier_template");
  });
});
