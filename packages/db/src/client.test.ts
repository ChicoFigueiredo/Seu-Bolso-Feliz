import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

/**
 * `vi.resetModules()` obriga cada teste a reimportar `./client` do zero, e
 * esse grafo puxa drizzle-orm e @neondatabase/serverless — transformar tudo
 * isso a frio custa segundos, não milissegundos.
 *
 * Com o padrão de 5s o teste terminava em ~5,0s: passava sozinho e falhava na
 * suíte cheia, conforme o escalonamento entre workers, sem nada de errado no
 * que ele afirma. Um teste que depende da máquina estar de bom humor não
 * prova nada — ou some do radar, ou treina a equipe a reexecutar até passar.
 */
const TEMPO_LIMITE_IMPORT_A_FRIO = 30_000;

describe("getDb", () => {
  const originalUrl = process.env.DATABASE_URL;

  beforeEach(() => {
    vi.resetModules();
    process.env.DATABASE_URL = "postgresql://user:pass@example.com/db";
  });

  afterEach(() => {
    process.env.DATABASE_URL = originalUrl;
  });

  it(
    "lança erro claro se DATABASE_URL não está definida",
    async () => {
      delete process.env.DATABASE_URL;
      const { getDb } = await import("./client");
      expect(() => getDb()).toThrow(/DATABASE_URL/);
    },
    TEMPO_LIMITE_IMPORT_A_FRIO,
  );

  it(
    "retorna a mesma instância em chamadas repetidas (lazy singleton)",
    async () => {
      const { getDb } = await import("./client");
      const first = getDb();
      const second = getDb();
      expect(first).toBe(second);
    },
    TEMPO_LIMITE_IMPORT_A_FRIO,
  );
});
