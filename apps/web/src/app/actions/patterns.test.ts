import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createPattern,
  createPatternVersion,
  deactivatePattern,
  getPattern,
  listPatterns,
  reactivatePattern,
  updatePattern,
} from "./patterns";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedInstitution(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.institutions)
    .values({ user_id: userId, name: `Instituição de teste ${crypto.randomUUID()}`, type: "bank" })
    .returning();
  if (!row) throw new Error("Falha ao semear institution de teste");
  return row;
}

async function seedSupplier(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.suppliers)
    .values({ user_id: userId, name: `Fornecedor de teste ${crypto.randomUUID()}` })
    .returning();
  if (!row) throw new Error("Falha ao semear supplier de teste");
  return row;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("patterns actions", () => {
  const patternIds: string[] = [];
  const feedbackIds: string[] = [];
  const institutionIds: string[] = [];
  const supplierIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of feedbackIds.splice(0)) {
      await db.delete(schema.pattern_feedback).where(eq(schema.pattern_feedback.id, id));
    }
    for (const id of patternIds.splice(0)) {
      await db.delete(schema.document_patterns).where(eq(schema.document_patterns.id, id));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await db.delete(schema.institutions).where(eq(schema.institutions.id, id));
    }
  });

  it("isola padrões por usuário", async () => {
    currentUserId = USER_A;
    const a = await createPattern({ name: "Padrão A", document_type: "boleto" });
    patternIds.push(a.id);
    currentUserId = USER_B;
    const b = await createPattern({ name: "Padrão B", document_type: "boleto" });
    patternIds.push(b.id);

    currentUserId = USER_A;
    const { data } = await listPatterns();
    expect(data.map((p) => p.id)).toContain(a.id);
    expect(data.map((p) => p.id)).not.toContain(b.id);

    const found = await getPattern(b.id);
    expect(found.pattern).toBeNull();
    expect(found.feedback).toEqual([]);
  });

  it("cria, busca (com feedback), atualiza, cria nova versão, desativa e reativa (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createPattern({
      name: "Cemig",
      document_type: "utility_bill",
      confidence_threshold: 0.9,
    });
    patternIds.push(created.id);

    const db = getDb();
    const [feedback] = await db
      .insert(schema.pattern_feedback)
      .values({
        user_id: USER_A,
        pattern_id: created.id,
        feedback_type: "correct",
        corrections: {},
      })
      .returning();
    if (!feedback) throw new Error("Falha ao semear feedback de teste");
    feedbackIds.push(feedback.id);

    const fetched = await getPattern(created.id);
    expect(fetched.pattern?.name).toBe("Cemig");
    expect(fetched.feedback.map((f) => f.id)).toContain(feedback.id);

    const updated = await updatePattern(created.id, { name: "Cemig v2" });
    expect(updated.name).toBe("Cemig v2");

    const versioned = await createPatternVersion(created.id, {
      field_mappings: { amount: "valor" },
    });
    patternIds.push(versioned.id);
    expect(versioned.version).toBe(2);
    expect(versioned.is_active).toBe(true);
    expect((await getPattern(created.id)).pattern?.is_active).toBe(false);

    await deactivatePattern(versioned.id);
    expect((await getPattern(versioned.id)).pattern?.is_active).toBe(false);

    await reactivatePattern(versioned.id);
    expect((await getPattern(versioned.id)).pattern?.is_active).toBe(true);
  });

  it("lança erro ao atualizar padrão inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updatePattern("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("lança 'Access denied' ao criar nova versão de padrão de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createPattern({ name: "Padrão B2", document_type: "boleto" });
    patternIds.push(b.id);

    currentUserId = USER_A;
    await expect(createPatternVersion(b.id, {})).rejects.toThrow("Access denied");
  });

  // IDOR: createPattern/updatePattern aceitavam supplier_id/institution_id
  // vindos do cliente sem validar posse — o RLS antigo cobria isso de graça
  // em `suppliers`/`institutions`; sem RLS, a checagem precisa ser explícita.
  it("lança erro ao criar padrão com supplier_id de outro usuário", async () => {
    currentUserId = USER_B;
    const supplierB = await seedSupplier(USER_B);
    supplierIds.push(supplierB.id);

    currentUserId = USER_A;
    await expect(
      createPattern({
        name: "Tentativa cross-user",
        document_type: "boleto",
        supplier_id: supplierB.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar padrão com institution_id de outro usuário", async () => {
    currentUserId = USER_B;
    const institutionB = await seedInstitution(USER_B);
    institutionIds.push(institutionB.id);

    currentUserId = USER_A;
    await expect(
      createPattern({
        name: "Tentativa cross-user",
        document_type: "boleto",
        institution_id: institutionB.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar padrão apontando supplier_id de outro usuário", async () => {
    currentUserId = USER_A;
    const a = await createPattern({ name: "Padrão A2", document_type: "boleto" });
    patternIds.push(a.id);

    currentUserId = USER_B;
    const supplierB = await seedSupplier(USER_B);
    supplierIds.push(supplierB.id);

    currentUserId = USER_A;
    await expect(updatePattern(a.id, { supplier_id: supplierB.id })).rejects.toThrow();
  });

  it("lança erro ao atualizar padrão apontando institution_id de outro usuário", async () => {
    currentUserId = USER_A;
    const a = await createPattern({ name: "Padrão A3", document_type: "boleto" });
    patternIds.push(a.id);

    currentUserId = USER_B;
    const institutionB = await seedInstitution(USER_B);
    institutionIds.push(institutionB.id);

    currentUserId = USER_A;
    await expect(updatePattern(a.id, { institution_id: institutionB.id })).rejects.toThrow();
  });
});
