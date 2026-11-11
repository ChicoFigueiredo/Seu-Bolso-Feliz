import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { updateDocumentMetadata } from "./document-metadata";

let currentUserId = "";
// A identidade vem inteiramente do nosso próprio `requireUser()`, sem dependência de provedor externo.
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

describe.skipIf(!process.env.TEST_DATABASE_URL)("document-metadata actions", () => {
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
        filename: "doc.pdf",
        status: "new",
      })
      .returning();
    if (!doc) throw new Error("Falha ao semear source_document de teste");
    docIds.push(doc.id);
    return doc;
  }

  it("atualiza metadados só do próprio documento", async () => {
    currentUserId = USER_A;
    const doc = await seedDoc(USER_A);

    await updateDocumentMetadata(doc.id, { supplier_name_raw: "Fornecedor X" });

    const db = getDb();
    const [updated] = await db
      .select()
      .from(schema.source_documents)
      .where(eq(schema.source_documents.id, doc.id));
    if (!updated) throw new Error("Documento de teste desapareceu");
    expect(updated.supplier_name_raw).toBe("Fornecedor X");
  });

  it("lança erro ao atualizar documento de outro usuário", async () => {
    currentUserId = USER_B;
    const docB = await seedDoc(USER_B);

    currentUserId = USER_A;
    await expect(
      updateDocumentMetadata(docB.id, { supplier_name_raw: "Hackeado" }),
    ).rejects.toThrow();
  });

  it("não faz nada (sem erro) se nenhum campo for informado", async () => {
    currentUserId = USER_A;
    const doc = await seedDoc(USER_A);
    await expect(updateDocumentMetadata(doc.id, {})).resolves.toBeUndefined();
  });
});
