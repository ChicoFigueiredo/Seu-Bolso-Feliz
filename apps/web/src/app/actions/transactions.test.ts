import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createTransaction,
  deleteTransaction,
  getTransaction,
  getTransactions,
  updateTransaction,
} from "./transactions";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedProductFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id);
  return { institutionId: institution.id, productId: product.id };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("transactions actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/transactions
    }
  });

  it("isola transações por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createTransaction({
      financial_product_id: seedA.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 100,
    });

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createTransaction({
      financial_product_id: seedB.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 200,
    });

    currentUserId = USER_A;
    const listA = await getTransactions();
    expect(listA.map((t) => t.id)).toContain(a.id);
    expect(listA.map((t) => t.id)).not.toContain(b.id);
    expect(await getTransaction(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedProductFor(USER_A);
    institutionIds.push(seed.institutionId);
    const created = await createTransaction({
      financial_product_id: seed.productId,
      type: "expense",
      event_date: "2026-01-05",
      amount: 50,
    });

    const updated = await updateTransaction(created.id, { amount: 75 });
    expect(updated.amount).toBe(75);

    await deleteTransaction(created.id);
    expect(await getTransaction(created.id)).toBeNull();
  });

  it("lança erro ao excluir transação de outro usuário", async () => {
    currentUserId = USER_B;
    const seed = await seedProductFor(USER_B);
    institutionIds.push(seed.institutionId);
    const b = await createTransaction({
      financial_product_id: seed.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 300,
    });

    currentUserId = USER_A;
    await expect(deleteTransaction(b.id)).rejects.toThrow();
  });
});
