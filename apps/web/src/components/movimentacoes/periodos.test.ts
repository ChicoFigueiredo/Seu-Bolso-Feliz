import { describe, expect, it } from "vitest";
import { intervaloDoPeriodo } from "./periodos";

// 30/08/2026, meio-dia local — a hora não importa para o cálculo, mas fixá-la
// evita que um teste rodando à meia-noite caia noutro dia.
const HOJE = new Date(2026, 7, 30, 12, 0, 0);

describe("intervaloDoPeriodo", () => {
  it("'este período' segue o mês civil quando o ciclo começa no dia 1", () => {
    expect(intervaloDoPeriodo("este-periodo", HOJE, 1)).toEqual({
      inicio: "2026-08-01",
      fim: "2026-08-31",
      rotulo: "01/08/2026 a 31/08/2026",
    });
  });

  it("'este período' respeita um ciclo que não começa no dia 1", () => {
    expect(intervaloDoPeriodo("este-periodo", HOJE, 20)).toEqual({
      inicio: "2026-08-20",
      fim: "2026-09-19",
      rotulo: "20/08/2026 a 19/09/2026",
    });
  });

  it("'período anterior' é o ciclo imediatamente antes, não trinta dias atrás", () => {
    expect(intervaloDoPeriodo("periodo-anterior", HOJE, 20)).toEqual({
      inicio: "2026-07-20",
      fim: "2026-08-19",
      rotulo: "20/07/2026 a 19/08/2026",
    });
  });

  it("ciclo no dia 31 encurta no mês curto sem pular para março", () => {
    const emFevereiro = new Date(2026, 1, 15, 12, 0, 0);
    expect(intervaloDoPeriodo("este-periodo", emFevereiro, 31)).toEqual({
      inicio: "2026-01-31",
      fim: "2026-02-27",
      rotulo: "31/01/2026 a 27/02/2026",
    });
  });

  it("'90 dias' conta noventa dias inclusive e termina hoje", () => {
    expect(intervaloDoPeriodo("90-dias", HOJE, 1)).toEqual({
      inicio: "2026-06-02",
      fim: "2026-08-30",
      rotulo: "Últimos 90 dias",
    });
  });

  it("'este ano' cobre o ano civil inteiro", () => {
    expect(intervaloDoPeriodo("este-ano", HOJE, 20)).toEqual({
      inicio: "2026-01-01",
      fim: "2026-12-31",
      rotulo: "Ano de 2026",
    });
  });

  it("'tudo' não impõe limite de data", () => {
    expect(intervaloDoPeriodo("tudo", HOJE, 1)).toEqual({
      inicio: null,
      fim: null,
      rotulo: "Todo o histórico",
    });
  });

  it("a virada de ano não vaza para o ano errado", () => {
    const emJaneiro = new Date(2026, 0, 5, 12, 0, 0);
    expect(intervaloDoPeriodo("periodo-anterior", emJaneiro, 1)).toEqual({
      inicio: "2025-12-01",
      fim: "2025-12-31",
      rotulo: "01/12/2025 a 31/12/2025",
    });
  });
});
