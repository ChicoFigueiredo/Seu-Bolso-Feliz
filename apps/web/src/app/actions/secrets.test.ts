import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { deleteSecret, listSecrets } from "./secrets";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("secrets actions", () => {
  const secretIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of secretIds.splice(0)) {
      await db.delete(schema.user_secrets).where(eq(schema.user_secrets.id, id));
    }
  });

  async function seedSecret(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.user_secrets)
      .values({
        user_id: userId,
        secret_type: "pdf_password",
        encrypted_value: "fake-encrypted-value",
      })
      .returning();
    if (!row) throw new Error("Falha ao semear user_secret de teste");
    secretIds.push(row.id);
    return row;
  }

  it("isola segredos por usuário (sem devolver o valor em claro)", async () => {
    currentUserId = USER_A;
    const a = await seedSecret(USER_A);
    currentUserId = USER_B;
    const b = await seedSecret(USER_B);

    currentUserId = USER_A;
    const listA = await listSecrets();
    expect(listA.map((s) => s.id)).toContain(a.id);
    expect(listA.map((s) => s.id)).not.toContain(b.id);
    expect(listA[0]).not.toHaveProperty("encryptedValue");
    expect(listA[0]).not.toHaveProperty("encrypted_value");
  });

  it("lança erro ao excluir segredo de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await seedSecret(USER_B);

    currentUserId = USER_A;
    await expect(deleteSecret(b.id)).rejects.toThrow();
  });
});
