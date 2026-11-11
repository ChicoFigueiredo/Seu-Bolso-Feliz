import { describe, expect, it } from "vitest";
import { splitEvenly } from "./split-evenly";
import { sumCents, toCents } from "./cents";

const A = "centro-a";
const B = "centro-b";
const C = "centro-c";

describe("splitEvenly", () => {
  it("divide em partes iguais quando o valor é divisível", () => {
    expect(splitEvenly(100, [A, B])).toEqual([
      { financialCenterId: A, amount: 50 },
      { financialCenterId: B, amount: 50 },
    ]);
  });

  it("entrega o resto ao primeiro centro", () => {
    expect(splitEvenly(100, [A, B, C])).toEqual([
      { financialCenterId: A, amount: 33.34 },
      { financialCenterId: B, amount: 33.33 },
      { financialCenterId: C, amount: 33.33 },
    ]);
  });

  it("entrega um resto de vários centavos ao primeiro centro", () => {
    // 10000 centavos / 7 = 1428, resto 4 -> primeiro fica com 1432.
    const partes = splitEvenly(100, [A, B, C, "d", "e", "f", "g"]);
    expect(partes[0]?.amount).toBe(14.32);
    expect(partes.slice(1).map((p) => p.amount)).toEqual([
      14.28, 14.28, 14.28, 14.28, 14.28, 14.28,
    ]);
  });

  it("nunca perde nem cria centavo, para qualquer valor e quantidade de centros", () => {
    const centros = [A, B, C, "d", "e", "f", "g", "h", "i", "j", "k", "l", "m"];
    for (let centavos = 1; centavos <= 400; centavos += 1) {
      const valor = centavos / 100;
      for (let n = 1; n <= Math.min(centros.length, centavos); n += 1) {
        const partes = splitEvenly(valor, centros.slice(0, n));
        expect(partes).toHaveLength(n);
        expect(sumCents(partes.map((p) => p.amount))).toBe(toCents(valor));
      }
    }
  });

  it("distribui a diferença em no máximo um centavo entre as partes", () => {
    const partes = splitEvenly(10, [A, B, C]);
    const valores = partes.map((p) => toCents(p.amount));
    expect(Math.max(...valores) - Math.min(...valores)).toBeLessThanOrEqual(1);
  });

  it("preserva a ordem dos centros recebidos", () => {
    expect(splitEvenly(9, [C, A, B]).map((p) => p.financialCenterId)).toEqual([C, A, B]);
  });

  it("usa o módulo do valor: receita divide igual a despesa", () => {
    expect(splitEvenly(-100, [A, B])).toEqual([
      { financialCenterId: A, amount: 50 },
      { financialCenterId: B, amount: 50 },
    ]);
  });

  it("aceita um único centro: recebe o valor inteiro", () => {
    expect(splitEvenly(37.77, [A])).toEqual([{ financialCenterId: A, amount: 37.77 }]);
  });

  it("recusa lista de centros vazia", () => {
    expect(() => splitEvenly(100, [])).toThrow(RangeError);
  });

  it("recusa centro repetido", () => {
    expect(() => splitEvenly(100, [A, A])).toThrow(RangeError);
  });

  it("recusa valor que não dá um centavo por centro", () => {
    // Um rateio de zero é proibido pelo domínio; dividir R$ 0,02 entre 3
    // centros é impossível sem produzir zero.
    expect(() => splitEvenly(0.02, [A, B, C])).toThrow(RangeError);
    expect(() => splitEvenly(0.02, [A, B])).not.toThrow();
  });

  it("recusa valor zero ou não finito", () => {
    expect(() => splitEvenly(0, [A])).toThrow(RangeError);
    expect(() => splitEvenly(Number.NaN, [A])).toThrow(RangeError);
    expect(() => splitEvenly(Number.POSITIVE_INFINITY, [A])).toThrow(RangeError);
  });
});
