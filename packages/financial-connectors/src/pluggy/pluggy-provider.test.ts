import { describe, it, expect } from "vitest";
import { normalizeAccount, normalizeTransaction } from "./pluggy-provider";
import type { PluggyAccount, PluggyTransaction } from "./types";

function makeTransaction(overrides: Partial<PluggyTransaction> = {}): PluggyTransaction {
  return {
    id: "tx-1",
    accountId: "acc-1",
    date: "2026-08-20T00:00:00.000Z",
    description: "Compra em loja",
    amount: 150.5,
    currencyCode: "BRL",
    category: "Alimentação",
    type: "DEBIT",
    status: "POSTED",
    ...overrides,
  };
}

describe("normalizeTransaction", () => {
  it("mapeia DEBIT para outflow com amountCents positivo", () => {
    const result = normalizeTransaction(makeTransaction({ type: "DEBIT", amount: 150.5 }));
    expect(result.direction).toBe("outflow");
    expect(result.amountCents).toBe(15050);
  });

  it("mapeia CREDIT para inflow com amountCents positivo", () => {
    const result = normalizeTransaction(makeTransaction({ type: "CREDIT", amount: 2000 }));
    expect(result.direction).toBe("inflow");
    expect(result.amountCents).toBe(200000);
  });

  it("usa magnitude absoluta mesmo quando o provedor manda amount negativo (cartão de crédito)", () => {
    const result = normalizeTransaction(makeTransaction({ type: "CREDIT", amount: -300 }));
    expect(result.direction).toBe("inflow");
    expect(result.amountCents).toBe(30000);
  });

  it("trunca a data para YYYY-MM-DD", () => {
    const result = normalizeTransaction(makeTransaction({ date: "2026-08-20T14:33:00.000Z" }));
    expect(result.date).toBe("2026-08-20");
  });

  it("mapeia status PENDING/POSTED", () => {
    expect(normalizeTransaction(makeTransaction({ status: "PENDING" })).status).toBe("pending");
    expect(normalizeTransaction(makeTransaction({ status: "POSTED" })).status).toBe("posted");
  });

  it("prefere merchant.businessName, cai para merchant.name", () => {
    const withBusinessName = normalizeTransaction(
      makeTransaction({ merchant: { businessName: "Loja LTDA", name: "Loja" } }),
    );
    expect(withBusinessName.merchantName).toBe("Loja LTDA");

    const withoutBusinessName = normalizeTransaction(
      makeTransaction({ merchant: { name: "Loja" } }),
    );
    expect(withoutBusinessName.merchantName).toBe("Loja");
  });

  it("preserva o payload cru em raw", () => {
    const tx = makeTransaction();
    const result = normalizeTransaction(tx);
    expect(result.raw).toMatchObject({ id: "tx-1" });
  });
});

describe("normalizeAccount", () => {
  it("converte balance para centavos", () => {
    const account: PluggyAccount = {
      id: "acc-1",
      itemId: "item-1",
      type: "BANK",
      name: "Conta corrente",
      balance: 1234.56,
      currencyCode: "BRL",
    };
    expect(normalizeAccount(account).balanceCents).toBe(123456);
  });
});
