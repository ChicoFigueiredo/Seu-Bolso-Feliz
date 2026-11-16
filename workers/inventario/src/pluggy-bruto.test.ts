import { describe, expect, it } from "vitest";
import { enriquecer, registroDaFatura } from "./pluggy-bruto";

describe("enriquecer", () => {
  it("Pix enviado: a contraparte é quem recebeu", () => {
    const e = enriquecer({
      id: "t1",
      type: "DEBIT",
      amount: -150,
      description: "PIX ENVIADO",
      paymentData: {
        payer: { name: "Titular da conta" },
        receiver: {
          name: "  Maria  da Silva ",
          documentNumber: { type: "CPF", value: "***.123.456-**" },
        },
        paymentMethod: "PIX",
      },
    });
    expect(e).toMatchObject({
      contraparte: "Maria da Silva",
      contraparte_doc: "***.123.456-**",
      meio: "PIX",
    });
  });

  it("Pix recebido: a contraparte é quem pagou", () => {
    const e = enriquecer({
      id: "t2",
      type: "CREDIT",
      amount: 80,
      paymentData: { payer: { name: "Empresa X Ltda" }, receiver: { name: "Titular" } },
    });
    expect(e.contraparte).toBe("Empresa X Ltda");
  });

  it("compra parcelada no cartão traz parcela, total e data original", () => {
    const e = enriquecer({
      id: "t3",
      type: "DEBIT",
      amount: -38.99,
      creditCardMetadata: {
        installmentNumber: 3,
        totalInstallments: 10,
        totalAmount: 389.9,
        purchaseDate: "2026-07-05T00:00:00.000Z",
        billId: "b9",
      },
      merchant: { name: "LOJA", cnpj: "12345678000199" },
    });
    expect(e).toMatchObject({
      parcela_numero: 3,
      parcela_total: 10,
      valor_total_compra_centavos: 38990,
      data_compra: "2026-07-05",
      fatura_id: "b9",
      merchant_cnpj: "12345678000199",
    });
  });

  it("compra à vista não vira parcela 1/1", () => {
    const e = enriquecer({
      id: "t4",
      type: "DEBIT",
      creditCardMetadata: {
        installmentNumber: 1,
        totalInstallments: 1,
        purchaseDate: "2026-10-01",
      },
    });
    expect(e.parcela_numero).toBeNull();
    expect(e.data_compra).toBe("2026-10-01");
  });
});

describe("registroDaFatura", () => {
  it("fatura vira registro de saída na data de vencimento", () => {
    expect(
      registroDaFatura({
        id: "b1",
        dueDate: "2026-11-10T00:00:00Z",
        totalAmount: 1715,
        minimumPaymentAmount: 257.25,
      }),
    ).toEqual({
      external_ref: "pluggy:bill_b1",
      data: "2026-11-10",
      valor_centavos: -171500,
      descricao: "Fatura do cartão (mínimo 257.25)",
    });
  });

  it("sem vencimento ou valor não vira registro", () => {
    expect(registroDaFatura({ id: "b2", totalAmount: 10 })).toBeNull();
  });
});
