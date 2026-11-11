import { describe, it, expect } from "vitest";
import { planejarAtualizacaoDeSaldos } from "./saldos";
import type { NormalizedAccount } from "@sbf/financial-connectors";

function conta(sobrescreve: Partial<NormalizedAccount>): NormalizedAccount {
  return {
    externalAccountId: "acc-1",
    externalItemId: "item-1",
    name: "CAIXA",
    type: "BANK",
    subtype: "CHECKING_ACCOUNT",
    currency: "BRL",
    balanceCents: 433,
    raw: {},
    ...sobrescreve,
  };
}

describe("planejarAtualizacaoDeSaldos", () => {
  it("converte centavos para reais no formato da coluna", () => {
    const plano = planejarAtualizacaoDeSaldos(
      [conta({ balanceCents: 433 })],
      [{ externalAccountId: "acc-1", financialProductId: "prod-1", status: "mapped" }],
    );
    expect(plano).toEqual([{ financialProductId: "prod-1", saldo: 4.33 }]);
  });

  it("saldo negativo (cheque especial) passa como veio", () => {
    const plano = planejarAtualizacaoDeSaldos(
      [conta({ balanceCents: -433_562 })],
      [{ externalAccountId: "acc-1", financialProductId: "prod-1", status: "mapped" }],
    );
    expect(plano[0]?.saldo).toBe(-4335.62);
  });

  it("cartão de crédito fica de fora — fatura não é saldo em conta", () => {
    const plano = planejarAtualizacaoDeSaldos(
      [conta({ type: "CREDIT", subtype: "CREDIT_CARD", balanceCents: 892_590 })],
      [{ externalAccountId: "acc-1", financialProductId: "prod-1", status: "mapped" }],
    );
    expect(plano).toEqual([]);
  });

  it("ignorados e sem produto vinculado ficam de fora", () => {
    const plano = planejarAtualizacaoDeSaldos(
      [conta({}), conta({ externalAccountId: "acc-2" })],
      [
        { externalAccountId: "acc-1", financialProductId: "prod-1", status: "ignored" },
        { externalAccountId: "acc-2", financialProductId: null, status: "pending_mapping" },
      ],
    );
    expect(plano).toEqual([]);
  });

  it("conta que sumiu da Pluggy não gera atualização", () => {
    const plano = planejarAtualizacaoDeSaldos(
      [],
      [{ externalAccountId: "acc-1", financialProductId: "prod-1", status: "mapped" }],
    );
    expect(plano).toEqual([]);
  });

  it("sem balance na resposta, não zera o que está no banco", () => {
    const plano = planejarAtualizacaoDeSaldos(
      [conta({ balanceCents: undefined })],
      [{ externalAccountId: "acc-1", financialProductId: "prod-1", status: "mapped" }],
    );
    expect(plano).toEqual([]);
  });
});
