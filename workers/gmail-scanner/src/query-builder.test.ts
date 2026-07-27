import { describe, it, expect } from "vitest";
import { buildGmailQuery, toGmailDate, addOneDay } from "./query-builder";

describe("toGmailDate", () => {
  it("converte ISO para o formato do Gmail", () => {
    expect(toGmailDate("2026-04-15")).toBe("2026/04/15");
  });

  it.each([null, undefined, "", "15/04/2026", "2026-4-5", "amanhã"])(
    "devolve null para entrada inválida (%p)",
    (input) => {
      expect(toGmailDate(input as string | null)).toBeNull();
    },
  );
});

describe("addOneDay", () => {
  it("avança um dia", () => {
    expect(addOneDay("2026-04-15")).toBe("2026-04-16");
  });

  it("atravessa a virada de mês", () => {
    expect(addOneDay("2026-04-30")).toBe("2026-05-01");
  });

  it("atravessa a virada de ano", () => {
    expect(addOneDay("2026-12-31")).toBe("2027-01-01");
  });

  it("lida com ano bissexto", () => {
    expect(addOneDay("2028-02-28")).toBe("2028-02-29");
  });
});

describe("buildGmailQuery", () => {
  it("devolve string vazia sem nenhum filtro", () => {
    expect(buildGmailQuery({})).toBe("");
  });

  it("preserva a query do usuário", () => {
    expect(buildGmailQuery({ query: "from:nubank" })).toBe("from:nubank");
  });

  it("adiciona after: a partir de fromDate", () => {
    expect(buildGmailQuery({ fromDate: "2026-01-01" })).toBe("after:2026/01/01");
  });

  it("torna toDate inclusivo somando um dia", () => {
    // before: é exclusivo no Gmail. Sem o +1, o dia 30 seria perdido.
    expect(buildGmailQuery({ toDate: "2026-04-30" })).toBe("before:2026/05/01");
  });

  it("compõe query e período", () => {
    expect(
      buildGmailQuery({
        query: "from:nubank has:attachment",
        fromDate: "2026-01-01",
        toDate: "2026-06-30",
      }),
    ).toBe("from:nubank has:attachment after:2026/01/01 before:2026/07/01");
  });

  it("ignora datas inválidas em vez de gerar filtro errado", () => {
    expect(buildGmailQuery({ query: "from:x", fromDate: "ontem" })).toBe("from:x");
  });

  it("apara espaços da query do usuário", () => {
    expect(buildGmailQuery({ query: "  from:x  " })).toBe("from:x");
  });
});
