import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  deleteSourceDocument,
  getDocumentStorageUrl,
  getSourceDocument,
  getSourceDocuments,
} from "./ingestion";

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
// `ingestion.ts` remove arquivo pelo @sbf/storage agora, não pelo cliente antigo.
vi.mock("@sbf/storage", () => ({
  createStorageClientFromEnv: () => ({
    from: () => ({
      remove: async () => {},
      upload: async () => {},
      getSignedUrl: async () => "https://exemplo.invalido/arquivo",
    }),
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

  async function seedDoc(userId: string, storagePath?: string) {
    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: userId,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "boleto.pdf",
        status: "new",
        ...(storagePath ? { storage_path: storagePath } : {}),
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

  /**
   * `getDocumentStorageUrl` recebe o `storage_path` cru do cliente. Sem
   * checagem de dono, ela assina o boleto de quem for.
   *
   * As duas asserções andam juntas de propósito: a segunda (o dono recebe a
   * URL) é o que impede a primeira de passar por acidente — se a checagem de
   * dono for removida, USER_A passa a receber a mesma URL e o teste quebra.
   */
  it("não assina URL do documento de outro usuário", async () => {
    const caminhoDeB = `${USER_B}/manual/${crypto.randomUUID()}.pdf`;
    currentUserId = USER_B;
    await seedDoc(USER_B, caminhoDeB);

    currentUserId = USER_A;
    expect(await getDocumentStorageUrl(caminhoDeB)).toBeNull();

    currentUserId = USER_B;
    expect(await getDocumentStorageUrl(caminhoDeB)).toBe("https://exemplo.invalido/arquivo");
  });

  it("devolve null para caminho que não existe, sem revelar a diferença", async () => {
    currentUserId = USER_A;
    expect(await getDocumentStorageUrl(`${USER_A}/manual/inexistente.pdf`)).toBeNull();
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
