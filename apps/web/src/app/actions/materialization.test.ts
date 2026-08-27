import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { postApprovedDraftRecord } from "./materialization";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("materialization actions", () => {
  const draftIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0)) {
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    }
  });

  async function seedDraft(userId: string, status: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.draft_records)
      .values({
        user_id: userId,
        draft_type: "transaction",
        status,
        draft_data: { amount: 100 },
        materialization_key: `test::${crypto.randomUUID()}`,
      })
      .returning();
    if (!row) throw new Error("Falha ao criar draft_record de teste");
    draftIds.push(row.id);
    return row;
  }

  it("recusa lançar draft de outro usuário (draft não encontrado)", async () => {
    currentUserId = USER_B;
    const draftB = await seedDraft(USER_B, "approved");

    currentUserId = USER_A;
    const result = await postApprovedDraftRecord(draftB.id);
    expect(result.success).toBe(false);
    expect(result.message).toContain("não encontrado");
  });

  it("recusa lançar draft ainda pending_review", async () => {
    currentUserId = USER_A;
    const draft = await seedDraft(USER_A, "pending_review");

    const result = await postApprovedDraftRecord(draft.id);
    expect(result.success).toBe(false);
    expect(result.message).toContain("Aprove primeiro");
  });

  it("recusa quando não autenticado", async () => {
    currentUserId = "";
    const result = await postApprovedDraftRecord("00000000-0000-0000-0000-000000000000");
    expect(result.success).toBe(false);
    expect(result.message).toBe("Não autenticado");
  });
});
