import { describe, expect, it } from "vitest";
import { percentageOf } from "./percentage";

const A = "centro-a";

describe("percentageOf", () => {
  it("deriva o percentual de um rateio", () => {
    expect(percentageOf({ financialCenterId: A, amount: 50 }, 200)).toBe(25);
    expect(percentageOf({ financialCenterId: A, amount: 200 }, 200)).toBe(100);
  });

  it("aceita o valor do rateio direto, sem o objeto", () => {
    expect(percentageOf(50, 200)).toBe(25);
  });

  it("arredonda para duas casas — é número de exibição, não de cálculo", () => {
    expect(percentageOf(33.34, 100)).toBe(33.34);
    expect(percentageOf(33.33, 100)).toBe(33.33);
    expect(percentageOf(1, 3)).toBe(33.33);
    expect(percentageOf(2, 3)).toBe(66.67);
  });

  it("usa o módulo do valor da movimentação: receita também tem percentual", () => {
    expect(percentageOf(50, -200)).toBe(25);
  });

  it("devolve zero quando a movimentação vale zero, em vez de dividir por zero", () => {
    expect(percentageOf(10, 0)).toBe(0);
  });

  it("devolve zero para rateio de valor zero", () => {
    expect(percentageOf(0, 200)).toBe(0);
  });

  it("recusa valores não finitos", () => {
    expect(() => percentageOf(Number.NaN, 100)).toThrow(RangeError);
    expect(() => percentageOf(10, Number.POSITIVE_INFINITY)).toThrow(RangeError);
  });

  it("pode passar de 100 quando o rateio excede a movimentação — quem barra é a validação", () => {
    expect(percentageOf(150, 100)).toBe(150);
  });
});
