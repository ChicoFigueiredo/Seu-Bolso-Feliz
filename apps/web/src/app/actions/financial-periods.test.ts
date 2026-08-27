import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createFinancialPeriod,
  deleteFinancialPeriod,
  getCurrentFinancialPeriod,
  getFinancialPeriods,
  updateFinancialPeriod,
} from "./financial-periods";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-periods actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db.delete(schema.financial_periods).where(eq(schema.financial_periods.id, id));
    }
  });

  it("isola períodos por usuário", async () => {
    currentUserId = USER_A;
    const a = await createFinancialPeriod({
      start_date: "2026-01-01",
      end_date: "2026-01-31",
      is_current: false,
    });
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createFinancialPeriod({
      start_date: "2026-01-01",
      end_date: "2026-01-31",
      is_current: false,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getFinancialPeriods();
    expect(listA.map((p) => p.id)).toContain(a.id);
    expect(listA.map((p) => p.id)).not.toContain(b.id);
  });

  it("cria, marca como atual, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createFinancialPeriod({
      start_date: "2026-02-01",
      end_date: "2026-02-28",
      is_current: true,
    });
    createdIds.push(created.id);

    const current = await getCurrentFinancialPeriod();
    expect(current?.id).toBe(created.id);

    const updated = await updateFinancialPeriod(created.id, { is_current: false });
    expect(updated.is_current).toBe(false);

    await deleteFinancialPeriod(created.id);
    createdIds.length = 0;
  });

  it("lança erro ao atualizar id de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createFinancialPeriod({
      start_date: "2026-03-01",
      end_date: "2026-03-31",
      is_current: false,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(updateFinancialPeriod(b.id, { is_current: true })).rejects.toThrow();
  });
});
