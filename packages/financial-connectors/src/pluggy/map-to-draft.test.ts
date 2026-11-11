import { describe, it, expect } from "vitest";
import { TransactionDraftV1Schema } from "@sbf/contracts";
import { pluggyTransactionToDraft } from "./map-to-draft";
import type { NormalizedTransaction } from "../financial-data-provider";

function makeNormalized(overrides: Partial<NormalizedTransaction> = {}): NormalizedTransaction {
  return {
    externalTransactionId: "tx-1",
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

describe("pluggyTransactionToDraft", () => {
  it("produz um draft válido contra TransactionDraftV1Schema", () => {
    const draft = pluggyTransactionToDraft(makeNormalized(), { financialProductId: null });
    expect(() => TransactionDraftV1Schema.parse(draft)).not.toThrow();
  });

  it("nunca referencia um documento — source_document_id null, parser_type pluggy_provider", () => {
    const draft = pluggyTransactionToDraft(makeNormalized(), { financialProductId: null });
    expect(draft.provenance.source_document_id).toBeNull();
    expect(draft.provenance.parser_type).toBe("pluggy_provider");
  });

  it("infere transaction_type a partir da direção (outflow → expense, inflow → income)", () => {
    const expense = pluggyTransactionToDraft(makeNormalized({ direction: "outflow" }), {
      financialProductId: null,
    });
    expect(expense.transaction_type).toBe("expense");

    const income = pluggyTransactionToDraft(makeNormalized({ direction: "inflow" }), {
      financialProductId: null,
    });
    expect(income.transaction_type).toBe("income");
  });

  it("repassa financialProductId quando a conta já foi mapeada", () => {
    const productId = "11111111-1111-1111-1111-111111111111";
    const draft = pluggyTransactionToDraft(makeNormalized(), { financialProductId: productId });
    expect(draft.financial_product_id).toBe(productId);
  });

  it("usa merchantName como supplier_name_raw", () => {
    const draft = pluggyTransactionToDraft(makeNormalized({ merchantName: "Loja LTDA" }), {
      financialProductId: null,
    });
    expect(draft.supplier_name_raw).toBe("Loja LTDA");
  });

  it("cai para descriptionRaw quando description está vazia", () => {
    const draft = pluggyTransactionToDraft(
      makeNormalized({ description: "", descriptionRaw: "PIX RECEBIDO" }),
      { financialProductId: null },
    );
    expect(draft.description).toBe("PIX RECEBIDO");
  });

  it("repassa supplierId resolvido pelo chamador", () => {
    const supplierId = "33333333-3333-3333-3333-333333333333";
    const draft = pluggyTransactionToDraft(makeNormalized(), {
      financialProductId: null,
      supplierId,
    });
    expect(draft.supplier_id).toBe(supplierId);
  });

  it("supplier_id nasce null quando o resolver não achou match (options omitido)", () => {
    const draft = pluggyTransactionToDraft(makeNormalized(), { financialProductId: null });
    expect(draft.supplier_id).toBeNull();
  });
});
