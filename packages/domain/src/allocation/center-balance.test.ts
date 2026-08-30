import { describe, expect, it } from "vitest";
import { centerBalance } from "./center-balance";

describe("centerBalance", () => {
  it("trata o centro como conta corrente virtual: entradas menos saídas", () => {
    expect(centerBalance(1000, 350)).toEqual({ inflow: 1000, outflow: 350, balance: 650 });
  });

  it("aceita saldo negativo — o centro pode ter consumido mais do que recebeu", () => {
    expect(centerBalance(100, 250).balance).toBe(-150);
  });

  it("aceita listas de valores em vez de totais prontos", () => {
    expect(centerBalance([100.1, 200.2], [50.05])).toEqual({
      inflow: 300.3,
      outflow: 50.05,
      balance: 250.25,
    });
  });

  it("soma em centavos: 0.1 + 0.2 não vira 0.30000000000000004", () => {
    expect(centerBalance([0.1, 0.2], []).inflow).toBe(0.3);
    expect(centerBalance([0.1, 0.2], [0.3]).balance).toBe(0);
  });

  it("centro sem movimento tem tudo zerado", () => {
    expect(centerBalance([], [])).toEqual({ inflow: 0, outflow: 0, balance: 0 });
    expect(centerBalance(0, 0)).toEqual({ inflow: 0, outflow: 0, balance: 0 });
  });

  it("não devolve zero negativo", () => {
    expect(Object.is(centerBalance(10, 10).balance, 0)).toBe(true);
  });

  it("recusa valores negativos: a direção vem do argumento, não do sinal", () => {
    expect(() => centerBalance(-10, 0)).toThrow(RangeError);
    expect(() => centerBalance(0, -10)).toThrow(RangeError);
    expect(() => centerBalance([10, -1], [])).toThrow(RangeError);
  });

  it("recusa valores não finitos", () => {
    expect(() => centerBalance(Number.NaN, 0)).toThrow(RangeError);
    expect(() => centerBalance(0, Number.POSITIVE_INFINITY)).toThrow(RangeError);
  });
});
