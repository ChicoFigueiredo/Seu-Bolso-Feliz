import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanupInstitution,
  seedCard,
  seedFinancialProduct,
  seedInstitution,
} from "./__tests__/fixtures";
import {
  createStatementCycle,
  deleteStatementCycle,
  getStatementCycle,
  getStatementCycles,
  updateStatementCycle,
} from "./statement-cycles";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

// `fixtures.ts` (Task 1) devolve `Row | undefined` (noUncheckedIndexedAccess
// no `const [row] = ...returning()` sem checagem) — não é escopo deste task
// tocar em infra compartilhada, então a checagem fica aqui, do lado de quem
// consome.
function required<T>(value: T | undefined, message: string): T {
  if (value === undefined) throw new Error(message);
  return value;
}

async function seedCardFor(userId: string) {
  const institution = required(await seedInstitution(userId), "seedInstitution falhou");
  const product = required(
    await seedFinancialProduct(userId, institution.id, "credit_card"),
    "seedFinancialProduct falhou",
  );
  const card = required(await seedCard(userId, product.id), "seedCard falhou");
  return { institutionId: institution.id, card };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("statement-cycles actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/cards/statement_cycles
    }
  });

  it("isola ciclos de fatura por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedCardFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createStatementCycle({
      card_id: seedA.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    currentUserId = USER_B;
    const seedB = await seedCardFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createStatementCycle({
      card_id: seedB.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    currentUserId = USER_A;
    const listA = await getStatementCycles();
    expect(listA.map((c) => c.id)).toContain(a.id);
    expect(listA.map((c) => c.id)).not.toContain(b.id);
    expect(await getStatementCycle(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedCardFor(USER_A);
    institutionIds.push(seed.institutionId);

    const created = await createStatementCycle({
      card_id: seed.card.id,
      reference_month: "2026-02-01",
      cycle_start_date: "2026-01-01",
      cycle_end_date: "2026-01-31",
      due_date: "2026-02-10",
    });

    const updated = await updateStatementCycle(created.id, { due_date: "2026-02-12" });
    expect(updated.due_date).toBe("2026-02-12");

    await deleteStatementCycle(created.id);
    expect(await getStatementCycle(created.id)).toBeNull();
  });

  it("lança erro ao atualizar ciclo inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateStatementCycle("00000000-0000-0000-0000-000000000000", { due_date: "2026-01-01" }),
    ).rejects.toThrow();
  });
});
