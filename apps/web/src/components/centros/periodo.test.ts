import { describe, expect, it } from "vitest";
import { rotuloCurtoDoPeriodo } from "./periodo";

describe("rotuloCurtoDoPeriodo", () => {
  it("chama o mês pelo nome quando o período é o mês civil inteiro", () => {
    expect(rotuloCurtoDoPeriodo("2026-08-01", "2026-08-31", 2026)).toBe("agosto");
    expect(rotuloCurtoDoPeriodo("2026-02-01", "2026-02-28", 2026)).toBe("fevereiro");
  });

  it("acerta fevereiro de ano bissexto", () => {
    expect(rotuloCurtoDoPeriodo("2028-02-01", "2028-02-29", 2028)).toBe("fevereiro");
    // 28 de fevereiro num ano bissexto NÃO fecha o mês: é ciclo próprio.
    expect(rotuloCurtoDoPeriodo("2028-02-01", "2028-02-28", 2028)).toBe("01/02 a 28/02");
  });

  it("mostra o ano quando não é o ano de referência", () => {
    expect(rotuloCurtoDoPeriodo("2025-12-01", "2025-12-31", 2026)).toBe("dezembro de 2025");
  });

  it("mostra os dias quando o ciclo não começa no dia 1º", () => {
    expect(rotuloCurtoDoPeriodo("2026-03-20", "2026-04-19", 2026)).toBe("20/03 a 19/04");
  });

  it("acrescenta o ano quando o ciclo atravessa o ano ou é de outro ano", () => {
    expect(rotuloCurtoDoPeriodo("2025-12-20", "2026-01-19", 2026)).toBe("20/12 a 19/01/2026");
  });

  it("devolve a entrada crua quando a data não está no formato esperado", () => {
    expect(rotuloCurtoDoPeriodo("ontem", "hoje", 2026)).toBe("ontem a hoje");
  });
});
