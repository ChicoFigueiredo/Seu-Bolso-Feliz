import "./test-db-setup"; // redireciona getDb() para TEST_DATABASE_URL — ver Task 1, Step 10
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createRecurringTemplate,
  deleteRecurringTemplate,
  getRecurringTemplate,
  getRecurringTemplates,
  updateRecurringTemplate,
} from "./recurring";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("recurring actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db
        .delete(schema.recurring_template_tags)
        .where(eq(schema.recurring_template_tags.recurring_template_id, id));
      await db.delete(schema.recurring_templates).where(eq(schema.recurring_templates.id, id));
    }
  });

  it("isola templates recorrentes por usuário", async () => {
    currentUserId = USER_A;
    const a = await createRecurringTemplate({
      name: "Aluguel",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 1500,
      is_active: true,
    });
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createRecurringTemplate({
      name: "Aluguel B",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 1200,
      is_active: true,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getRecurringTemplates();
    expect(listA.map((t) => t.id)).toContain(a.id);
    expect(listA.map((t) => t.id)).not.toContain(b.id);
    expect(await getRecurringTemplate(b.id)).toBeNull();
  });

  it("cria com tags, atualiza e desativa (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createRecurringTemplate({
      name: "Internet",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 100,
      is_active: true,
    });
    createdIds.push(created.id);

    const updated = await updateRecurringTemplate(created.id, { amount: 120 });
    expect(updated.amount).toBe(120);

    await deleteRecurringTemplate(created.id);
    const afterDelete = await getRecurringTemplate(created.id);
    expect(afterDelete?.is_active).toBe(false);
  });

  it("lança erro ao atualizar template de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createRecurringTemplate({
      name: "Luz",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 200,
      is_active: true,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(updateRecurringTemplate(b.id, { amount: 999 })).rejects.toThrow();
  });
});
