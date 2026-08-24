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
});
