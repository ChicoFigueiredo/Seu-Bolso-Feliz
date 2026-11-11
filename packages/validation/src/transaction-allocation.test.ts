import { describe, expect, it } from "vitest";
import { allocateTransactionSchema, allocationInputSchema } from "./transaction-allocation";

const CENTRO = "11111111-1111-1111-1111-111111111111";
const MOVIMENTACAO = "22222222-2222-2222-2222-222222222222";

describe("allocationInputSchema", () => {
  it("aceita um rateio com centro e valor", () => {
    expect(allocationInputSchema.parse({ financialCenterId: CENTRO, amount: 12.34 })).toEqual({
      financialCenterId: CENTRO,
      amount: 12.34,
    });
  });

  it("recusa valor zero ou negativo — o sinal vem da movimentação", () => {
    expect(allocationInputSchema.safeParse({ financialCenterId: CENTRO, amount: 0 }).success).toBe(
      false,
    );
    expect(allocationInputSchema.safeParse({ financialCenterId: CENTRO, amount: -5 }).success).toBe(
      false,
    );
  });

  it("recusa valor não finito", () => {
    expect(
      allocationInputSchema.safeParse({ financialCenterId: CENTRO, amount: Number.NaN }).success,
    ).toBe(false);
    expect(
      allocationInputSchema.safeParse({
        financialCenterId: CENTRO,
        amount: Number.POSITIVE_INFINITY,
      }).success,
    ).toBe(false);
  });

  it("recusa centro que não é uuid", () => {
    expect(
      allocationInputSchema.safeParse({ financialCenterId: "edulingo", amount: 1 }).success,
    ).toBe(false);
  });

  it("não aceita percentual: rateio guarda valor", () => {
    const parsed = allocationInputSchema.parse({
      financialCenterId: CENTRO,
      amount: 10,
      percentage: 50,
    });
    expect(parsed).not.toHaveProperty("percentage");
  });
});

describe("allocateTransactionSchema", () => {
  it("aceita conjunto vazio — significa apagar todos os rateios", () => {
    const parsed = allocateTransactionSchema.parse({
      transactionId: MOVIMENTACAO,
      allocations: [],
    });
    expect(parsed.allocations).toEqual([]);
  });

  it("recusa conjunto maior que o teto defensivo", () => {
    const allocations = Array.from({ length: 101 }, () => ({
      financialCenterId: CENTRO,
      amount: 1,
    }));
    expect(
      allocateTransactionSchema.safeParse({ transactionId: MOVIMENTACAO, allocations }).success,
    ).toBe(false);
  });

  it("recusa movimentação que não é uuid", () => {
    expect(
      allocateTransactionSchema.safeParse({ transactionId: "x", allocations: [] }).success,
    ).toBe(false);
  });
});
