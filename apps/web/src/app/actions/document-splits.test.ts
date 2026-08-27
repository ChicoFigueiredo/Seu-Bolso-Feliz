import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createDocumentSplit,
  deleteDocumentSplit,
  getDocumentSplitsTotal,
  listDocumentSplits,
} from "./document-splits";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedSourceDocument(userId: string, amount?: number) {
  const db = getDb();
  const [row] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: `test-${crypto.randomUUID()}`,
      filename: "documento-teste.pdf",
      metadata: amount !== undefined ? { amount } : null,
    })
    .returning();
  if (!row) throw new Error("Falha ao semear source_document de teste");
  return row;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("document-splits actions", () => {
  const sourceDocumentIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sourceDocumentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  it("isola rateios por usuário mesmo quando o id do documento é conhecido", async () => {
    currentUserId = USER_A;
    const docA = await seedSourceDocument(USER_A, 100);
    sourceDocumentIds.push(docA.id);
    await createDocumentSplit({ source_document_id: docA.id, amount: 40 });

    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B, 100);
    sourceDocumentIds.push(docB.id);
    await createDocumentSplit({ source_document_id: docB.id, amount: 40 });

    currentUserId = USER_A;
    expect(await listDocumentSplits(docB.id)).toEqual([]);
  });

  it("cria, lista, soma e exclui rateios (caminho feliz)", async () => {
    currentUserId = USER_A;
    const doc = await seedSourceDocument(USER_A, 100);
    sourceDocumentIds.push(doc.id);

    const split1 = await createDocumentSplit({
      source_document_id: doc.id,
      amount: 40,
      description: "Parte 1",
    });
    await createDocumentSplit({ source_document_id: doc.id, amount: 30, description: "Parte 2" });

    expect((await listDocumentSplits(doc.id)).length).toBe(2);
    expect(await getDocumentSplitsTotal(doc.id)).toBe(70);

    await deleteDocumentSplit(split1.id);
    expect(await getDocumentSplitsTotal(doc.id)).toBe(30);
  });

  it("lança erro quando a soma dos rateios excede o valor do documento", async () => {
    currentUserId = USER_A;
    const doc = await seedSourceDocument(USER_A, 50);
    sourceDocumentIds.push(doc.id);

    await expect(createDocumentSplit({ source_document_id: doc.id, amount: 60 })).rejects.toThrow(
      /excede o valor do documento/,
    );
  });

  // IDOR fechado (mesma classe do achado CRITICAL em document-transactions.ts,
  // commit b5995eb): createDocumentSplit não validava que source_document_id
  // pertence ao usuário autenticado antes de gravar. Um usuário A conseguia
  // criar um rateio próprio (user_id correto) apontando para o documento de B.
  it("lança erro ao criar rateio apontando para documento de outro usuário", async () => {
    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B, 100);
    sourceDocumentIds.push(docB.id);

    currentUserId = USER_A;
    await expect(createDocumentSplit({ source_document_id: docB.id, amount: 10 })).rejects.toThrow(
      /Documento não encontrado/,
    );
  });

  it("não exclui (nem lança erro) rateio de outro usuário — comportamento preservado", async () => {
    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B, 100);
    sourceDocumentIds.push(docB.id);
    const splitB = await createDocumentSplit({ source_document_id: docB.id, amount: 20 });

    currentUserId = USER_A;
    await deleteDocumentSplit(splitB.id); // não deve lançar

    const db = getDb();
    const [stillThere] = await db
      .select()
      .from(schema.document_splits)
      .where(eq(schema.document_splits.id, splitB.id));
    expect(stillThere).toBeDefined();
  });
});
