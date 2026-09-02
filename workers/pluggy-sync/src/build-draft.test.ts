import { describe, it, expect } from "vitest";
import { buildDraftRecordInsert } from "./build-draft";
import type { NormalizedTransaction } from "@sbf/financial-connectors";

function makeNormalized(overrides: Partial<NormalizedTransaction> = {}): NormalizedTransaction {
  return {
    externalTransactionId: "txn_abc123",
    externalAccountId: "acc-1",
    direction: "outflow",
    amountCents: 15050,
    currency: "BRL",
    date: "2026-08-20",
    description: "Compra em loja",
    status: "posted",
    raw: {},
    ...overrides,
  };
}

describe("buildDraftRecordInsert", () => {
  it("leva a categoria resolvida para o draft, sem perder a sugestão crua", () => {
    // A sugestão continua no payload porque ela é a EVIDÊNCIA de por que esta
    // categoria foi escolhida — a revisão humana precisa dela para discordar.
    const row = buildDraftRecordInsert(makeNormalized({ category: "Groceries" }), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: "11111111-1111-4111-8111-111111111111",
      categoryId: "22222222-2222-4222-8222-222222222222",
    });
    const data = row.draft_data as Record<string, unknown>;
    expect(data.category_id).toBe("22222222-2222-4222-8222-222222222222");
    expect(data.category_suggestion).toBe("Groceries");
  });

  it("sem categoria resolvida, o draft nasce sem categoria (não inventa)", () => {
    const row = buildDraftRecordInsert(makeNormalized({ category: "Algo Desconhecido" }), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: "11111111-1111-4111-8111-111111111111",
    });
    expect((row.draft_data as Record<string, unknown>).category_id).toBeNull();
  });

  it("gera external_ref opaco e estável a partir do id da transação", () => {
    const row = buildDraftRecordInsert(makeNormalized({ externalTransactionId: "txn_abc123" }), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: null,
    });
    expect(row.external_ref).toBe("pluggy:txn_txn_abc123");
  });

  it("nunca referencia documento — source_document_id/extraction_result_id null", () => {
    const row = buildDraftRecordInsert(makeNormalized(), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: null,
    });
    expect(row.source_document_id).toBeNull();
    expect(row.extraction_result_id).toBeNull();
  });

  it("confidence_score fixo em 1.0 — fonte estruturada, não OCR", () => {
    const row = buildDraftRecordInsert(makeNormalized(), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: null,
    });
    expect(row.confidence_score).toBe(1.0);
  });

  it("repassa batch_id, user_id e financial_product_id", () => {
    const productId = "22222222-2222-2222-2222-222222222222";
    const row = buildDraftRecordInsert(makeNormalized(), {
      batchId: "batch-42",
      userId: "user-42",
      financialProductId: productId,
    });
    expect(row.batch_id).toBe("batch-42");
    expect(row.user_id).toBe("user-42");
    expect((row.draft_data as { financial_product_id: string }).financial_product_id).toBe(
      productId,
    );
  });

  it("status nasce sempre pending_review", () => {
    const row = buildDraftRecordInsert(makeNormalized(), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: null,
    });
    expect(row.status).toBe("pending_review");
  });

  it("repassa supplierId resolvido pro draft_data", () => {
    const supplierId = "44444444-4444-4444-4444-444444444444";
    const row = buildDraftRecordInsert(makeNormalized(), {
      batchId: "batch-1",
      userId: "user-1",
      financialProductId: null,
      supplierId,
    });
    expect((row.draft_data as { supplier_id: string | null }).supplier_id).toBe(supplierId);
  });
});
