import { describe, expect, it } from "vitest";
import { validateAllocations } from "./validate-allocations";

const CENTRO_EDULINGO = "11111111-1111-1111-1111-111111111111";
const CENTRO_IMOB_MAX = "22222222-2222-2222-2222-222222222222";
const CENTRO_DAIANE = "33333333-3333-3333-3333-333333333333";

describe("validateAllocations", () => {
  it("aceita movimentação sem nenhum rateio: tudo fica não alocado", () => {
    const resultado = validateAllocations(150, []);
    expect(resultado.ok).toBe(true);
    expect(resultado.issues).toEqual([]);
    expect(resultado.allocated).toBe(0);
    expect(resultado.unallocated).toBe(150);
  });

  it("aceita rateio parcial e devolve o não alocado", () => {
    const resultado = validateAllocations(150, [
      { financialCenterId: CENTRO_EDULINGO, amount: 100 },
    ]);
    expect(resultado.ok).toBe(true);
    expect(resultado.allocated).toBe(100);
    expect(resultado.unallocated).toBe(50);
  });

  it("aceita rateio integral: não alocado zera", () => {
    const resultado = validateAllocations(100, [
      { financialCenterId: CENTRO_EDULINGO, amount: 33.34 },
      { financialCenterId: CENTRO_IMOB_MAX, amount: 33.33 },
      { financialCenterId: CENTRO_DAIANE, amount: 33.33 },
    ]);
    expect(resultado.ok).toBe(true);
    expect(resultado.allocated).toBe(100);
    expect(resultado.unallocated).toBe(0);
  });

  it("soma em centavos: 0.1 + 0.2 não estoura uma movimentação de 0.30", () => {
    const resultado = validateAllocations(0.3, [
      { financialCenterId: CENTRO_EDULINGO, amount: 0.1 },
      { financialCenterId: CENTRO_IMOB_MAX, amount: 0.2 },
    ]);
    expect(resultado.ok).toBe(true);
    expect(resultado.allocated).toBe(0.3);
    expect(resultado.unallocated).toBe(0);
  });

  it("recusa soma maior que a movimentação, ainda que por um centavo", () => {
    const resultado = validateAllocations(100, [
      { financialCenterId: CENTRO_EDULINGO, amount: 99.99 },
      { financialCenterId: CENTRO_IMOB_MAX, amount: 0.02 },
    ]);
    expect(resultado.ok).toBe(false);
    expect(resultado.issues.map((i) => i.code)).toContain("soma_excede_movimentacao");
    // O excedente ainda é informado, para a UI conseguir dizer quanto sobra.
    expect(resultado.unallocated).toBe(-0.01);
  });

  it("recusa rateio de valor zero ou negativo", () => {
    const zero = validateAllocations(100, [{ financialCenterId: CENTRO_EDULINGO, amount: 0 }]);
    expect(zero.ok).toBe(false);
    expect(zero.issues[0]?.code).toBe("valor_nao_positivo");
    expect(zero.issues[0]?.financialCenterId).toBe(CENTRO_EDULINGO);

    const negativo = validateAllocations(100, [
      { financialCenterId: CENTRO_EDULINGO, amount: -10 },
    ]);
    expect(negativo.ok).toBe(false);
    expect(negativo.issues[0]?.code).toBe("valor_nao_positivo");
  });

  it("recusa rateio que arredonda para zero centavo", () => {
    const resultado = validateAllocations(100, [
      { financialCenterId: CENTRO_EDULINGO, amount: 0.004 },
    ]);
    expect(resultado.ok).toBe(false);
    expect(resultado.issues[0]?.code).toBe("valor_nao_positivo");
  });

  it("recusa o mesmo centro repetido", () => {
    const resultado = validateAllocations(100, [
      { financialCenterId: CENTRO_EDULINGO, amount: 40 },
      { financialCenterId: CENTRO_EDULINGO, amount: 20 },
    ]);
    expect(resultado.ok).toBe(false);
    const repetido = resultado.issues.find((i) => i.code === "centro_repetido");
    expect(repetido?.financialCenterId).toBe(CENTRO_EDULINGO);
  });

  it("recusa centro sem identificador", () => {
    const resultado = validateAllocations(100, [{ financialCenterId: "  ", amount: 10 }]);
    expect(resultado.ok).toBe(false);
    expect(resultado.issues.map((i) => i.code)).toContain("centro_ausente");
  });

  it("recusa valores não finitos", () => {
    const resultado = validateAllocations(100, [
      { financialCenterId: CENTRO_EDULINGO, amount: Number.NaN },
    ]);
    expect(resultado.ok).toBe(false);
    expect(resultado.issues[0]?.code).toBe("valor_invalido");
  });

  it("recusa movimentação com valor não finito", () => {
    const resultado = validateAllocations(Number.NaN, []);
    expect(resultado.ok).toBe(false);
    expect(resultado.issues[0]?.code).toBe("movimentacao_invalida");
  });

  it("usa o módulo do valor: receita rateia igual a despesa", () => {
    // O sinal vem da movimentação, nunca do rateio.
    const resultado = validateAllocations(-500, [
      { financialCenterId: CENTRO_EDULINGO, amount: 300 },
    ]);
    expect(resultado.ok).toBe(true);
    expect(resultado.unallocated).toBe(200);
  });

  it("movimentação de valor zero não aceita rateio nenhum", () => {
    expect(validateAllocations(0, []).ok).toBe(true);
    const resultado = validateAllocations(0, [
      { financialCenterId: CENTRO_EDULINGO, amount: 0.01 },
    ]);
    expect(resultado.ok).toBe(false);
    expect(resultado.issues.map((i) => i.code)).toContain("soma_excede_movimentacao");
  });

  it("acumula todos os problemas de uma vez, não só o primeiro", () => {
    const resultado = validateAllocations(10, [
      { financialCenterId: CENTRO_EDULINGO, amount: 20 },
      { financialCenterId: CENTRO_EDULINGO, amount: -1 },
    ]);
    const codigos = resultado.issues.map((i) => i.code);
    expect(codigos).toContain("valor_nao_positivo");
    expect(codigos).toContain("centro_repetido");
    expect(codigos).toContain("soma_excede_movimentacao");
  });

  it("não perde centavo somando cem rateios de um centavo", () => {
    const rateios = Array.from({ length: 100 }, (_, i) => ({
      financialCenterId: `centro-${i}`,
      amount: 0.01,
    }));
    const resultado = validateAllocations(1, rateios);
    expect(resultado.ok).toBe(true);
    expect(resultado.allocated).toBe(1);
    expect(resultado.unallocated).toBe(0);
  });

  it("toda mensagem de problema está em português", () => {
    const resultado = validateAllocations(1, [{ financialCenterId: CENTRO_EDULINGO, amount: 5 }]);
    expect(resultado.issues[0]?.message).toMatch(/\S/);
    expect(resultado.issues[0]?.message).toContain("rateio");
  });
});
