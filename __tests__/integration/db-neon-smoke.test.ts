import { describe, it, expect } from "vitest";
import { getDb, institutions } from "@sbf/db";
import { sql } from "drizzle-orm";

// Este teste exige um Neon real alcançável via DATABASE_URL. O CI
// (.github/workflows/ci.yml) provisiona só o Supabase local e roda
// `bun run test:integration` SEM DATABASE_URL — sem o skipIf, getDb() lança
// "DATABASE_URL é obrigatório para @sbf/db" e quebra o job. Apontar o CI para
// o Neon de produção não é opção: isso exigiria um branch Neon dedicado a CI,
// decisão de Fase 2 da ADR-009.
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
