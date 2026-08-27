import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createLiability,
  deleteLiability,
  getLiability,
  getLiabilities,
  updateLiability,
} from "./liabilities";

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

describe.skipIf(!process.env.TEST_DATABASE_URL)("liabilities actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/liabilities/liability_installments
    }
  });

  it("isola dívidas por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createLiability({
      financial_product_id: seedA.productId,
      name: "Financiamento carro",
      type: "personal_loan",
      original_amount: 30000,
      outstanding_balance: 30000,
      status: "active",
    });

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createLiability({
      financial_product_id: seedB.productId,
      name: "Empréstimo",
      type: "personal_loan",
      original_amount: 5000,
      outstanding_balance: 5000,
      status: "active",
    });

    currentUserId = USER_A;
    const listA = await getLiabilities();
    expect(listA.map((l) => l.id)).toContain(a.id);
    expect(listA.map((l) => l.id)).not.toContain(b.id);
    expect(await getLiability(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedProductFor(USER_A);
    institutionIds.push(seed.institutionId);
    const created = await createLiability({
      financial_product_id: seed.productId,
      name: "Cartão parcelado",
      type: "personal_loan",
      original_amount: 1000,
      outstanding_balance: 1000,
      status: "active",
    });

    const updated = await updateLiability(created.id, { status: "paid_off" });
    expect(updated.status).toBe("paid_off");

    await deleteLiability(created.id);
    expect(await getLiability(created.id)).toBeNull();
  });

  it("lança erro ao atualizar dívida de outro usuário", async () => {
    currentUserId = USER_B;
    const seed = await seedProductFor(USER_B);
    institutionIds.push(seed.institutionId);
    const b = await createLiability({
      financial_product_id: seed.productId,
      name: "Dívida B",
      type: "personal_loan",
      original_amount: 2000,
      outstanding_balance: 2000,
      status: "active",
    });

    currentUserId = USER_A;
    await expect(updateLiability(b.id, { status: "paid_off" })).rejects.toThrow();
  });
});
