import { describe, expect, it } from "vitest";
import { fromCents, sumCents, toCents } from "./cents";

describe("toCents", () => {
  it("converte reais em centavos inteiros", () => {
    expect(toCents(0)).toBe(0);
    expect(toCents(1)).toBe(100);
    expect(toCents(19.99)).toBe(1999);
    expect(toCents(1234.56)).toBe(123456);
  });

  it("não perde centavo quando o ponto flutuante representa o produto por baixo", () => {
    // Em IEEE-754: 0.29 * 100 === 28.999999999999996, 1.15 * 100 === 114.99999999999999,
    // 8.2 * 100 === 819.9999999999999, 4.35 * 100 === 434.99999999999994.
    expect(toCents(0.29)).toBe(29);
    expect(toCents(1.15)).toBe(115);
    expect(toCents(8.2)).toBe(820);
    expect(toCents(4.35)).toBe(435);
  });

  it("arredonda meio centavo para longe do zero", () => {
    expect(toCents(1.005)).toBe(101);
    expect(toCents(0.005)).toBe(1);
    expect(toCents(-1.005)).toBe(-101);
    // 8.165 * 100 === 816.4999999999999 — a versão ingênua devolveria 816.
    expect(toCents(8.165)).toBe(817);
  });

  it("descarta fração abaixo de meio centavo", () => {
    expect(toCents(0.004)).toBe(0);
    expect(toCents(10.0049)).toBe(1000);
  });

  it("preserva o sinal e normaliza o zero negativo", () => {
    expect(toCents(-19.99)).toBe(-1999);
    expect(Object.is(toCents(-0), 0)).toBe(true);
    expect(Object.is(toCents(-0.001), 0)).toBe(true);
  });

  it("suporta o limite de numeric(14,2)", () => {
    expect(toCents(999999999999.99)).toBe(99999999999999);
  });

  it("rejeita valores não finitos", () => {
    expect(() => toCents(Number.NaN)).toThrow(RangeError);
    expect(() => toCents(Number.POSITIVE_INFINITY)).toThrow(RangeError);
    expect(() => toCents(Number.NEGATIVE_INFINITY)).toThrow(RangeError);
  });
});

describe("fromCents", () => {
  it("volta de centavos para reais", () => {
    expect(fromCents(0)).toBe(0);
    expect(fromCents(1999)).toBe(19.99);
    expect(fromCents(-1999)).toBe(-19.99);
    expect(fromCents(123456)).toBe(1234.56);
  });

  it("faz round-trip com toCents sem deriva", () => {
    for (const valor of [0.01, 0.29, 1.15, 8.2, 19.99, 33.33, 1234.56, 999.99]) {
      expect(fromCents(toCents(valor))).toBe(valor);
    }
  });

  it("rejeita centavos fracionários", () => {
    expect(() => fromCents(10.5)).toThrow(RangeError);
  });
});

describe("sumCents", () => {
  it("soma em centavos, sem acumular erro de ponto flutuante", () => {
    // 0.1 + 0.2 === 0.30000000000000004 somando em reais.
    expect(sumCents([0.1, 0.2])).toBe(30);
    expect(sumCents([])).toBe(0);
    expect(sumCents([33.33, 33.33, 33.34])).toBe(10000);
  });

  it("soma cem parcelas de um centavo e chega exatamente a um real", () => {
    expect(sumCents(Array.from({ length: 100 }, () => 0.01))).toBe(100);
  });
});
