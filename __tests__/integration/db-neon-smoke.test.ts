import { describe, it, expect } from "vitest";
import { getDb, institutions } from "@sbf/db";
import { sql } from "drizzle-orm";

describe("Neon smoke test", () => {
  it("conecta no Neon real e executa uma query simples", async () => {
    const db = getDb();
    const result = await db.execute(sql`select 1 as ok`);
    expect(result.rows[0]).toEqual({ ok: 1 });
  });

  it("a tabela institutions existe e é consultável", async () => {
    const db = getDb();
    const rows = await db.select().from(institutions).limit(1);
    expect(Array.isArray(rows)).toBe(true);
  });
});
