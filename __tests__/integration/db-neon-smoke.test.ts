import { describe, it, expect } from "vitest";
import { getDb, institutions } from "@sbf/db";
import { sql } from "drizzle-orm";

// Este teste exige um Postgres real alcançável via DATABASE_URL — em
// desenvolvimento normalmente é o Neon; no CI (.github/workflows/ci.yml) é o
// Postgres efêmero do job, com schema.sql e as migrations já aplicados antes
// da suíte rodar. Sem DATABASE_URL (ex.: clone novo, ainda sem `.env.local`
// configurado), o skipIf evita que getDb() lance "DATABASE_URL é obrigatório
// para @sbf/db" e quebre o job.
describe.skipIf(!process.env.DATABASE_URL)("Neon smoke test", () => {
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
