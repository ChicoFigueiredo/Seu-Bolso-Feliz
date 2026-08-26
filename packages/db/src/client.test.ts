import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

describe("getDb", () => {
  const originalUrl = process.env.DATABASE_URL;

  beforeEach(() => {
    vi.resetModules();
    process.env.DATABASE_URL = "postgresql://user:pass@example.com/db";
  });

  afterEach(() => {
    process.env.DATABASE_URL = originalUrl;
  });

  it("lança erro claro se DATABASE_URL não está definida", async () => {
    delete process.env.DATABASE_URL;
    const { getDb } = await import("./client");
    expect(() => getDb()).toThrow(/DATABASE_URL/);
  });

  it("retorna a mesma instância em chamadas repetidas (lazy singleton)", async () => {
    const { getDb } = await import("./client");
    const first = getDb();
    const second = getDb();
    expect(first).toBe(second);
  });
});
