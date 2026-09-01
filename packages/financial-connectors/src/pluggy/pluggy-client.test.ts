import { describe, it, expect } from "vitest";
import { extrairCursor } from "./pluggy-client";

/**
 * A query string exata que a `/v2/transactions` devolveu em produção. Guardada
 * literal de propósito: foi mandá-la de volta como se fosse o cursor que
 * derrubou o backfill com 400 — e, pior, deixou o valor errado gravado no
 * checkpoint, repetindo o mesmo erro em toda execução seguinte.
 */
const NEXT_REAL =
  "?accountId=2d462c2e-87ee-44fc-931e-6bcd9151a471&dateFrom=2025-09-01&dateTo=2026-09-01" +
  "&after=MjAyNi0wNS0xMFQxNDoyOTowMC4wMDBafDRmODZhNzViLTNhNDQtNGE2OS1hNTdkLWVmNDAzMjYxNjk2Yw%3D%3D";

const TOKEN_REAL =
  "MjAyNi0wNS0xMFQxNDoyOTowMC4wMDBafDRmODZhNzViLTNhNDQtNGE2OS1hNTdkLWVmNDAzMjYxNjk2Yw==";

describe("extrairCursor", () => {
  it("extrai o after da query string que a v2 devolve em next", () => {
    expect(extrairCursor(NEXT_REAL)).toBe(TOKEN_REAL);
  });

  it("decodifica o padding base64 que vem percent-encoded", () => {
    expect(extrairCursor(NEXT_REAL)?.endsWith("==")).toBe(true);
  });

  it("descarta accountId e as datas — o filtro vem de quem chama", () => {
    // Um cursor gravado para uma conta e reaplicado em outra não pode
    // carregar o accountId antigo junto.
    expect(extrairCursor(NEXT_REAL)).not.toContain("accountId");
    expect(extrairCursor(NEXT_REAL)).not.toContain("dateFrom");
  });

  it("cura um cursor já gravado no formato errado (regressão do 400)", () => {
    // Idempotência: normalizar o que já saiu normalizado não muda nada.
    expect(extrairCursor(extrairCursor(NEXT_REAL))).toBe(TOKEN_REAL);
  });

  it("devolve o token intacto quando já veio limpo", () => {
    expect(extrairCursor(TOKEN_REAL)).toBe(TOKEN_REAL);
  });

  it("aceita a query string sem o ? inicial", () => {
    expect(extrairCursor(`after=${encodeURIComponent(TOKEN_REAL)}`)).toBe(TOKEN_REAL);
  });

  it("trata o fim da paginação como ausência de cursor", () => {
    expect(extrairCursor(null)).toBeUndefined();
    expect(extrairCursor(undefined)).toBeUndefined();
    expect(extrairCursor("")).toBeUndefined();
  });

  it("não inventa cursor quando a query string não tem after", () => {
    expect(extrairCursor("?accountId=abc&dateFrom=2025-09-01")).toBeUndefined();
  });
});
