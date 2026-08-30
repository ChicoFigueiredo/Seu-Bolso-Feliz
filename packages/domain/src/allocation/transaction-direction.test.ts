import { describe, expect, it } from "vitest";
import {
  allocationDirection,
  canAllocateTransactionType,
  INFLOW_TRANSACTION_TYPES,
  NON_ALLOCATABLE_TRANSACTION_TYPES,
} from "./transaction-direction";

describe("allocationDirection", () => {
  it("conta receita e estorno como entrada do centro", () => {
    expect(allocationDirection("income")).toBe("inflow");
    expect(allocationDirection("refund")).toBe("inflow");
  });

  it("conta despesa, tarifa, juros e pagamento de dívida como saída do centro", () => {
    expect(allocationDirection("expense")).toBe("outflow");
    expect(allocationDirection("fee")).toBe("outflow");
    expect(allocationDirection("interest_charge")).toBe("outflow");
    expect(allocationDirection("liability_payment")).toBe("outflow");
  });

  it("trata tipo desconhecido como saída, o lado conservador", () => {
    expect(allocationDirection("adjustment")).toBe("outflow");
    expect(allocationDirection("tipo_que_nao_existe")).toBe("outflow");
  });

  it("expõe a lista de tipos de entrada para o SQL usar a mesma regra", () => {
    expect([...INFLOW_TRANSACTION_TYPES]).toEqual(["income", "refund"]);
  });
});

describe("canAllocateTransactionType", () => {
  it("bloqueia pagamento de fatura — as compras dentro dela já foram rateadas", () => {
    expect(canAllocateTransactionType("statement_payment")).toBe(false);
    expect([...NON_ALLOCATABLE_TRANSACTION_TYPES]).toEqual(["statement_payment"]);
  });

  it("libera os demais tipos", () => {
    for (const tipo of [
      "income",
      "expense",
      "refund",
      "adjustment",
      "interest_charge",
      "fee",
      "liability_payment",
    ]) {
      expect(canAllocateTransactionType(tipo)).toBe(true);
    }
  });
});
