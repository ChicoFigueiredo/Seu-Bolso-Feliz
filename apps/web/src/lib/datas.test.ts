import { describe, it, expect, vi, afterEach } from "vitest";
import { hojeIso, paraIso, somaDias } from "./datas";

afterEach(() => vi.useRealTimers());

describe("paraIso", () => {
  // A janela que motivou o módulo: 30/08 às 22h37 em São Paulo já é 31/08 em UTC.
  it("22h37 de São Paulo ainda é o mesmo dia, não o seguinte", () => {
    expect(paraIso(new Date("2026-08-31T01:37:00Z"))).toBe("2026-08-30");
  });

  it("meia-noite e um minuto em São Paulo já é o dia novo", () => {
    expect(paraIso(new Date("2026-08-31T03:01:00Z"))).toBe("2026-08-31");
  });

  it("23h59 em UTC ainda é o dia anterior em São Paulo", () => {
    expect(paraIso(new Date("2026-08-30T23:59:00Z"))).toBe("2026-08-30");
  });

  it("não muda nada no meio do dia, quando UTC e São Paulo concordam", () => {
    expect(paraIso(new Date("2026-08-30T15:00:00Z"))).toBe("2026-08-30");
  });

  it("aceita número e string, não só Date", () => {
    const t = Date.parse("2026-08-31T01:37:00Z");
    expect(paraIso(t)).toBe("2026-08-30");
    expect(paraIso("2026-08-31T01:37:00Z")).toBe("2026-08-30");
  });
});

describe("hojeIso", () => {
  it("dá o dia de São Paulo mesmo com o relógio do processo em UTC", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-31T02:10:00Z")); // 23h10 em São Paulo
    expect(hojeIso()).toBe("2026-08-30");
  });

  it("vira o dia na meia-noite de São Paulo, não na de UTC", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-31T03:00:00Z"));
    expect(hojeIso()).toBe("2026-08-31");
  });
});

describe("somaDias", () => {
  it("soma e subtrai", () => {
    expect(somaDias("2026-08-30", 1)).toBe("2026-08-31");
    expect(somaDias("2026-08-30", -1)).toBe("2026-08-29");
    expect(somaDias("2026-08-30", 7)).toBe("2026-09-06");
  });

  it("atravessa a virada do ano", () => {
    expect(somaDias("2026-12-31", 1)).toBe("2027-01-01");
  });

  it("respeita ano bissexto", () => {
    expect(somaDias("2028-02-28", 1)).toBe("2028-02-29");
  });

  it("somar zero devolve a mesma data", () => {
    expect(somaDias("2026-08-30", 0)).toBe("2026-08-30");
  });
});
