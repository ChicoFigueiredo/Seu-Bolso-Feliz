import "./test-db-setup"; // redireciona getDb() para TEST_DATABASE_URL — ver Task 1, Step 10
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createInstitution,
  deleteInstitution,
  getInstitution,
  getInstitutions,
  updateInstitution,
} from "./institutions";

// Mocka a identidade — os testes chamam as funções "como se fossem" o userId A.
let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("institutions actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db.delete(schema.institutions).where(eq(schema.institutions.id, id));
    }
  });

  it("isola instituições por usuário", async () => {
    currentUserId = USER_A;
    const a = await createInstitution({ name: "Banco A", type: "bank" });
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createInstitution({ name: "Banco B", type: "bank" });
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getInstitutions();
    expect(listA.map((i) => i.id)).toContain(a.id);
    expect(listA.map((i) => i.id)).not.toContain(b.id);

    const found = await getInstitution(b.id);
    expect(found).toBeNull();
  });

  it("cria, busca, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createInstitution({ name: "Nubank", type: "bank" });
    createdIds.push(created.id);

    const fetched = await getInstitution(created.id);
    expect(fetched?.name).toBe("Nubank");

    const updated = await updateInstitution(created.id, { name: "Nubank Atualizado" });
    expect(updated.name).toBe("Nubank Atualizado");

    await deleteInstitution(created.id);
    expect(await getInstitution(created.id)).toBeNull();
    createdIds.length = 0; // já deletado, não tentar de novo no afterEach
  });

  it("lança erro ao atualizar id inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateInstitution("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("lança erro ao excluir id de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createInstitution({ name: "Banco B2", type: "bank" });
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(deleteInstitution(b.id)).rejects.toThrow();
  });
});
