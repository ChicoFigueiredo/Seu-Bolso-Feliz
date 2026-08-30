import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  linkTransactionToDocument,
  listLinkedTransactions,
  unlinkTransactionFromDocument,
} from "./document-transactions";

let currentUserId = "";
// A identidade vem do nosso próprio `requireUser()` desde a saída do Supabase.
// Este mock devolve `ownerId === id` porque estes testes exercitam o dono; a
// distinção ator/dono (o admin de emergência operando sobre a base do dono)
// tem cobertura própria em apps/web/src/app/api/**.
vi.mock("@/lib/auth", () => {
  const sessao = () =>
    currentUserId
      ? {
          id: currentUserId,
          ownerId: currentUserId,
          email: "teste@exemplo.com",
          role: "owner" as const,
          name: "Teste",
        }
      : null;
  return {
    requireUser: async () => {
      const s = sessao();
      if (!s) throw new Error("Não autenticado");
      return s;
    },
    getUser: async () => sessao(),
    getUserOrNull: async () => sessao(),
  };
});

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("document-transactions actions", () => {
  const linkIds: string[] = [];
  const docIds: string[] = [];
  const institutionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of linkIds.splice(0)) {
      await db.delete(schema.document_transactions).where(eq(schema.document_transactions.id, id));
    }
    for (const id of docIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/transactions/document_transactions
    }
  });

  async function seedDocAndTx(userId: string) {
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
    docIds.push(doc.id);

    const institution = await seedInstitution(userId);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(userId, institution.id);

    const [tx] = await db
      .insert(schema.transactions)
      .values({
        user_id: userId,
        financial_product_id: product.id,
        type: "expense",
        event_date: "2026-01-01",
        amount: 100,
      })
      .returning();
    if (!tx) throw new Error("Falha ao semear transaction de teste");
    return { doc, tx };
  }

  it("isola vínculos por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedDocAndTx(USER_A);
    const linkA = await linkTransactionToDocument(seedA.doc.id, seedA.tx.id, "payment");
    linkIds.push(linkA.id);

    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);
    const linkB = await linkTransactionToDocument(seedB.doc.id, seedB.tx.id, "payment");
    linkIds.push(linkB.id);

    currentUserId = USER_A;
    const listA = await listLinkedTransactions(seedA.doc.id);
    expect(listA.map((l) => l.id)).toContain(linkA.id);

    const listB = await listLinkedTransactions(seedB.doc.id);
    expect(listB.map((l) => l.id)).not.toContain(linkA.id);
  });

  it("lança erro ao desvincular id de outro usuário", async () => {
    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);
    const linkB = await linkTransactionToDocument(seedB.doc.id, seedB.tx.id, "payment");
    linkIds.push(linkB.id);

    currentUserId = USER_A;
    await expect(unlinkTransactionFromDocument(linkB.id)).rejects.toThrow();
  });

  // IDOR fechado (achado CRITICAL da revisão): linkTransactionToDocument não
  // validava posse das FKs (source_document_id/transaction_id) antes de
  // gravar o vínculo — só gravava user_id na própria linha de
  // document_transactions. Um usuário A conseguia vincular o próprio
  // documento a uma transação de B sem erro.
  it("lança erro ao vincular transação de outro usuário ao próprio documento", async () => {
    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);

    currentUserId = USER_A;
    const seedA = await seedDocAndTx(USER_A);

    await expect(linkTransactionToDocument(seedA.doc.id, seedB.tx.id, "payment")).rejects.toThrow();
  });

  it("lança erro ao vincular a própria transação a um documento de outro usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedDocAndTx(USER_A);

    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);

    currentUserId = USER_A;
    await expect(linkTransactionToDocument(seedB.doc.id, seedA.tx.id, "payment")).rejects.toThrow();
  });

  // Defesa em profundidade (achado CRITICAL da revisão): listLinkedTransactions
  // fazia innerJoin com transactions mas só filtrava
  // document_transactions.user_id, nunca transactions.user_id. Mesmo que uma
  // linha "errada" já exista (dado legado, ou inserida por um caminho que não
  // passa por linkTransactionToDocument), o objeto `transaction` de outro
  // usuário não pode vazar na listagem.
  it("não expõe a transação de outro usuário mesmo que o vínculo já exista", async () => {
    currentUserId = USER_A;
    const seedA = await seedDocAndTx(USER_A);
    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);

    const db = getDb();
    const [badLink] = await db
      .insert(schema.document_transactions)
      .values({
        source_document_id: seedA.doc.id,
        transaction_id: seedB.tx.id,
        user_id: USER_A,
        link_type: "payment",
        created_by: "user",
      })
      .returning();
    if (!badLink) throw new Error("Falha ao semear vínculo de teste");
    linkIds.push(badLink.id);

    currentUserId = USER_A;
    const list = await listLinkedTransactions(seedA.doc.id);
    expect(list.map((l) => l.id)).not.toContain(badLink.id);
  });
});
