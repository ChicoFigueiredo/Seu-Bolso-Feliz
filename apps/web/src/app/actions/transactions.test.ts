import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
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

async function seedSourceDocument(userId: string) {
  const db = getDb();
  const [doc] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: `test::${crypto.randomUUID()}`,
      filename: "teste.pdf",
      status: "new",
    })
    .returning();
  if (!doc) throw new Error("Falha ao semear source_document de teste");
  return doc;
}

async function seedCategory(userId: string) {
  const db = getDb();
  const [category] = await db
    .insert(schema.categories)
    .values({ user_id: userId, name: `Categoria de teste ${crypto.randomUUID()}` })
    .returning();
  if (!category) throw new Error("Falha ao semear category de teste");
  return category;
}

async function seedTag(userId: string) {
  const db = getDb();
  const [tag] = await db
    .insert(schema.tags)
    .values({ user_id: userId, name: `Tag de teste ${crypto.randomUUID()}` })
    .returning();
  if (!tag) throw new Error("Falha ao semear tag de teste");
  return tag;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("transactions actions", () => {
  const institutionIds: string[] = [];
  const sourceDocumentIds: string[] = [];
  const categoryIds: string[] = [];
  const tagRowIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sourceDocumentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
    for (const id of categoryIds.splice(0)) {
      await db.delete(schema.categories).where(eq(schema.categories.id, id));
    }
    for (const id of tagRowIds.splice(0)) {
      await db.delete(schema.tags).where(eq(schema.tags.id, id));
    }
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

  // IDOR fechado (achado CRITICAL da varredura de segurança): createTransaction/
  // updateTransaction não validavam posse de NENHUMA FK do input.
  // source_document_id é o caso mais grave — contorna inteiramente a
  // validação de posse já aplicada em linkTransactionToDocument
  // (document-transactions.ts, commit b5995eb), já que basta gravar a FK
  // direto na transação em vez de passar pelo vínculo.
  it("lança erro ao criar transação com source_document_id de outro usuário", async () => {
    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B);
    sourceDocumentIds.push(docB.id);

    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);

    await expect(
      createTransaction({
        financial_product_id: seedA.productId,
        type: "expense",
        event_date: "2026-01-01",
        amount: 100,
        source_document_id: docB.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar transação apontando source_document_id para documento de outro usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const tx = await createTransaction({
      financial_product_id: seedA.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 100,
    });

    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B);
    sourceDocumentIds.push(docB.id);

    currentUserId = USER_A;
    await expect(updateTransaction(tx.id, { source_document_id: docB.id })).rejects.toThrow();
  });

  it("lança erro ao criar transação com financial_product_id de outro usuário", async () => {
    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);

    currentUserId = USER_A;
    await expect(
      createTransaction({
        financial_product_id: seedB.productId,
        type: "expense",
        event_date: "2026-01-01",
        amount: 100,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar transação com category_id de outro usuário", async () => {
    currentUserId = USER_B;
    const categoryB = await seedCategory(USER_B);
    categoryIds.push(categoryB.id);

    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);

    await expect(
      createTransaction({
        financial_product_id: seedA.productId,
        type: "expense",
        event_date: "2026-01-01",
        amount: 100,
        category_id: categoryB.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar transação com tag de outro usuário", async () => {
    currentUserId = USER_B;
    const tagB = await seedTag(USER_B);
    tagRowIds.push(tagB.id);

    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);

    await expect(
      createTransaction(
        {
          financial_product_id: seedA.productId,
          type: "expense",
          event_date: "2026-01-01",
          amount: 100,
        },
        [tagB.id],
      ),
    ).rejects.toThrow();
  });
});
