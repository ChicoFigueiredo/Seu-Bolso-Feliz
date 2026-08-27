import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedInstitution } from "./__tests__/fixtures";
import {
  createFinancialProduct,
  deleteFinancialProduct,
  getFinancialProduct,
  getFinancialProducts,
  updateFinancialProduct,
} from "./financial-products";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-products actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products
    }
  });

  it("isola produtos financeiros por usuário", async () => {
    currentUserId = USER_A;
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const a = await createFinancialProduct({
      institution_id: instA.id,
      name: "Conta A",
      type: "checking_account",
    });

    currentUserId = USER_B;
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);
    const b = await createFinancialProduct({
      institution_id: instB.id,
      name: "Conta B",
      type: "checking_account",
    });

    currentUserId = USER_A;
    const listA = await getFinancialProducts();
    expect(listA.map((p) => p.id)).toContain(a.id);
    expect(listA.map((p) => p.id)).not.toContain(b.id);
    expect(await getFinancialProduct(b.id)).toBeNull();
  });

  it("cria, busca, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const inst = await seedInstitution(USER_A);
    institutionIds.push(inst.id);

    const created = await createFinancialProduct({
      institution_id: inst.id,
      name: "Cartão Nubank",
      type: "credit_card",
      credit_limit: 5000,
    });

    const fetched = await getFinancialProduct(created.id);
    expect(fetched?.name).toBe("Cartão Nubank");
    expect(fetched?.credit_limit).toBe(5000); // number, não string — Task 1 (mode: "number")

    const updated = await updateFinancialProduct(created.id, { name: "Cartão Nubank Platinum" });
    expect(updated.name).toBe("Cartão Nubank Platinum");

    await deleteFinancialProduct(created.id);
    expect(await getFinancialProduct(created.id)).toBeNull();
  });

  it("lança erro ao atualizar produto inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateFinancialProduct("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("não exclui (nem lança erro) produto de outro usuário — comportamento preservado", async () => {
    currentUserId = USER_B;
    const inst = await seedInstitution(USER_B);
    institutionIds.push(inst.id);
    const product = await createFinancialProduct({
      institution_id: inst.id,
      name: "Conta B2",
      type: "checking_account",
    });

    currentUserId = USER_A;
    await deleteFinancialProduct(product.id); // não deve lançar, e não deve remover a linha de B

    const db = getDb();
    const [row] = await db
      .select()
      .from(schema.financial_products)
      .where(eq(schema.financial_products.id, product.id));
    expect(row).toBeDefined();
  });

  // IDOR: mesma classe do achado em cards.ts (requireOwnedFinancialProduct) e
  // document-transactions.ts (commit b5995eb) — createFinancialProduct não
  // validava que institution_id pertence ao usuário autenticado antes de
  // gravar. Um usuário A conseguia criar um produto próprio (user_id
  // correto) apontando institution_id para a instituição de B.
  it("lança erro ao criar produto financeiro apontando para instituição de outro usuário", async () => {
    currentUserId = USER_B;
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);

    currentUserId = USER_A;
    await expect(
      createFinancialProduct({
        institution_id: instB.id,
        name: "Invasor",
        type: "checking_account",
      }),
    ).rejects.toThrow();
  });

  // Mesma classe de IDOR, caminho de updateFinancialProduct: reapontar um
  // produto já próprio para a instituição de outro usuário também não
  // validava posse.
  it("lança erro ao atualizar produto financeiro apontando para instituição de outro usuário", async () => {
    currentUserId = USER_A;
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const product = await createFinancialProduct({
      institution_id: instA.id,
      name: "Conta A",
      type: "checking_account",
    });

    currentUserId = USER_B;
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);

    currentUserId = USER_A;
    await expect(
      updateFinancialProduct(product.id, { institution_id: instB.id }),
    ).rejects.toThrow();
  });
});
