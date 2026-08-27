import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import { createCard, deleteCard, getCard, getCards, updateCard } from "./cards";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedProductFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id, "credit_card");
  return { institutionId: institution.id, productId: product.id };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("cards actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/cards/statement_cycles
    }
  });

  it("isola cartões por usuário e resolve o produto/instituição", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createCard({ financial_product_id: seedA.productId });

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createCard({ financial_product_id: seedB.productId });

    currentUserId = USER_A;
    const listA = await getCards();
    expect(listA.map((c) => c.id)).toContain(a.id);
    expect(listA.map((c) => c.id)).not.toContain(b.id);

    const found = await getCard(a.id);
    expect(found?.financial_products?.id).toBe(seedA.productId);
    expect(await getCard(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedProductFor(USER_A);
    institutionIds.push(seed.institutionId);
    const created = await createCard({ financial_product_id: seed.productId });

    const updated = await updateCard(created.id, { holder_name: "Chico" });
    expect(updated.holder_name).toBe("Chico");

    await deleteCard(created.id);
    expect(await getCard(created.id)).toBeNull();
  });

  it("lança erro ao atualizar cartão de outro usuário", async () => {
    currentUserId = USER_B;
    const seed = await seedProductFor(USER_B);
    institutionIds.push(seed.institutionId);
    const b = await createCard({ financial_product_id: seed.productId });

    currentUserId = USER_A;
    await expect(updateCard(b.id, { holder_name: "Hackeado" })).rejects.toThrow();
  });

  // IDOR fechado (mesma classe do achado CRITICAL da Task 7 em
  // document-transactions.ts, commit b5995eb): createCard não validava que
  // financial_product_id pertence ao usuário autenticado antes de gravar. Um
  // usuário A conseguia criar um cartão próprio (user_id correto) apontando
  // para o produto financeiro de B.
  it("lança erro ao criar cartão apontando para produto financeiro de outro usuário", async () => {
    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);

    currentUserId = USER_A;
    await expect(createCard({ financial_product_id: seedB.productId })).rejects.toThrow();
  });

  // Mesma classe de IDOR, caminho de updateCard: reapontar um cartão já
  // próprio para o produto financeiro de outro usuário também não validava
  // posse.
  it("lança erro ao atualizar cartão apontando para produto financeiro de outro usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const card = await createCard({ financial_product_id: seedA.productId });

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);

    currentUserId = USER_A;
    await expect(updateCard(card.id, { financial_product_id: seedB.productId })).rejects.toThrow();
  });

  // Defesa em profundidade (mesmo padrão do achado CRITICAL em
  // listLinkedTransactions/document-transactions.ts): mesmo que uma linha já
  // exista com FK inconsistente — aqui, um financial_product que pertence ao
  // usuário certo mas cuja institution_id aponta para uma instituição de
  // outro usuário (dado legado, ou gravado por um caminho que não passa por
  // este arquivo) — getCard/getCards não podem vazar o nome dessa
  // instituição de terceiro. O segundo leftJoin (institutions) filtra
  // user_id no próprio ON, então a linha do cartão continua aparecendo, só
  // que com institutions: null.
  it("não expõe a instituição de outro usuário quando o FK de institution_id está inconsistente", async () => {
    currentUserId = USER_B;
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);

    currentUserId = USER_A;
    // Dado inconsistente: produto pertence a A, mas a instituição referenciada
    // pertence a B. `seedFinancialProduct` não valida posse — simula o bug de
    // escrita anterior que a defesa em profundidade precisa cobrir.
    const productA = await seedFinancialProduct(USER_A, instB.id, "credit_card");
    const card = await createCard({ financial_product_id: productA.id });

    const found = await getCard(card.id);
    expect(found?.financial_products?.id).toBe(productA.id);
    expect(found?.financial_products?.institutions).toBeNull();

    const list = await getCards();
    const cardInList = list.find((c) => c.id === card.id);
    expect(cardInList?.financial_products?.institutions).toBeNull();
  });
});
