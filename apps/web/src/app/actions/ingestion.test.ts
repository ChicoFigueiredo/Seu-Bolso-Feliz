import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { deleteSourceDocument, getSourceDocument, getSourceDocuments } from "./ingestion";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
    storage: { from: () => ({ remove: async () => ({ error: null }) }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("ingestion actions — source documents", () => {
  const docIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of docIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  async function seedDoc(userId: string) {
    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: userId,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "boleto.pdf",
        status: "new",
      })
      .returning();
    if (!doc) throw new Error("Falha ao criar source_document de teste");
    docIds.push(doc.id);
    return doc;
  }

  it("isola documentos por usuário", async () => {
    currentUserId = USER_A;
    const docA = await seedDoc(USER_A);
    currentUserId = USER_B;
    const docB = await seedDoc(USER_B);

    currentUserId = USER_A;
    const { data: listA } = await getSourceDocuments();
    expect(listA.map((d) => d.id)).toContain(docA.id);
    expect(listA.map((d) => d.id)).not.toContain(docB.id);
    expect(await getSourceDocument(docB.id)).toBeNull();
  });

  it("lança erro ao excluir documento de outro usuário", async () => {
    currentUserId = USER_B;
    const docB = await seedDoc(USER_B);

    currentUserId = USER_A;
    await expect(deleteSourceDocument(docB.id, "document_only")).rejects.toThrow();
  });

  it("exclui documento (modo document_only) removendo o registro", async () => {
    currentUserId = USER_A;
    const doc = await seedDoc(USER_A);
    await deleteSourceDocument(doc.id, "document_only");

    const found = await getSourceDocument(doc.id);
    expect(found?.status).toBe("deleted");
    docIds.length = 0;
  });
});
