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
  registerPatternFeedback,
  updatePattern,
} from "./patterns";

let currentUserId = "";
// `ownerId` é o dono dos dados; `id` é o ator. Toda action filtra por
// `ownerId` — o mock usa ids DIFERENTES de propósito, para que um filtro
// feito por `id` por engano apareça como falha em vez de passar despercebido.
vi.mock("@/lib/auth", () => ({
  requireUser: async () => ({
    id: "00000000-0000-0000-0000-00000000ac70",
    ownerId: currentUserId,
    email: "teste@exemplo.com",
    role: "owner" as const,
    name: "Teste",
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
    // Nome único por execução (não fixo) e captura do id fora do `rejects` —
    // se a guarda de posse algum dia regredir, o insert chega a acontecer;
    // sem isso, a linha ficaria órfã no banco de teste e o 2º teste (mesmo
    // nome fixo, mesmo user_id, version 1) falharia por violação de
    // constraint única em vez de pela checagem de posse, mascarando a
    // regressão real. Ver `document_patterns_user_id_name_version_key`.
    let createdId: string | undefined;
    try {
      await expect(
        (async () => {
          const row = await createPattern({
            name: `Tentativa cross-user ${crypto.randomUUID()}`,
            document_type: "boleto",
            supplier_id: supplierB.id,
          });
          createdId = row.id;
          return row;
        })(),
      ).rejects.toThrow();
    } finally {
      if (createdId) patternIds.push(createdId);
    }
  });

  it("lança erro ao criar padrão com institution_id de outro usuário", async () => {
    currentUserId = USER_B;
    const institutionB = await seedInstitution(USER_B);
    institutionIds.push(institutionB.id);

    currentUserId = USER_A;
    let createdId: string | undefined;
    try {
      await expect(
        (async () => {
          const row = await createPattern({
            name: `Tentativa cross-user ${crypto.randomUUID()}`,
            document_type: "boleto",
            institution_id: institutionB.id,
          });
          createdId = row.id;
          return row;
        })(),
      ).rejects.toThrow();
    } finally {
      if (createdId) patternIds.push(createdId);
    }
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

// ══════════════════════════════════════════════════════════════
// registerPatternFeedback (era a RPC register_pattern_feedback)
// ══════════════════════════════════════════════════════════════

describe.skipIf(!process.env.TEST_DATABASE_URL)("registerPatternFeedback", () => {
  const patternIds: string[] = [];
  const documentIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    // pattern_feedback cai por CASCADE junto com o padrão.
    for (const id of patternIds.splice(0)) {
      await db.delete(schema.document_patterns).where(eq(schema.document_patterns.id, id));
    }
    for (const id of documentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  async function seedDocumento(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.source_documents)
      .values({
        user_id: userId,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "fatura.pdf",
        status: "new",
      })
      .returning();
    if (!row) throw new Error("Falha ao semear source_document de teste");
    documentIds.push(row.id);
    return row;
  }

  async function contadores(patternId: string) {
    const db = getDb();
    const [row] = await db
      .select({
        success_count: schema.document_patterns.success_count,
        feedback_count: schema.document_patterns.feedback_count,
      })
      .from(schema.document_patterns)
      .where(eq(schema.document_patterns.id, patternId));
    return row!;
  }

  async function seedPattern(userId: string) {
    currentUserId = userId;
    const pattern = await createPattern({
      name: `Padrão feedback ${crypto.randomUUID()}`,
      document_type: "boleto",
    });
    patternIds.push(pattern.id);
    return pattern;
  }

  it("grava o feedback com o dono correto e vincula ao padrão", async () => {
    const pattern = await seedPattern(USER_A);
    const documento = await seedDocumento(USER_A);

    const row = await registerPatternFeedback({
      pattern_id: pattern.id,
      source_document_id: documento.id,
      feedback_type: "correct",
      corrections: { amount: "corrigido" },
    });

    expect(row.pattern_id).toBe(pattern.id);
    expect(row.user_id).toBe(USER_A);
    expect(row.source_document_id).toBe(documento.id);
    expect(row.corrections).toEqual({ amount: "corrigido" });

    // E aparece na leitura pela action, não só no retorno.
    const { feedback } = await getPattern(pattern.id);
    expect(feedback.map((f) => f.id)).toContain(row.id);
  });

  it("'correct' incrementa success_count e só ele", async () => {
    const pattern = await seedPattern(USER_A);
    const antes = await contadores(pattern.id);

    await registerPatternFeedback({ pattern_id: pattern.id, feedback_type: "correct" });

    const depois = await contadores(pattern.id);
    expect(depois.success_count).toBe(antes.success_count + 1);
    expect(depois.feedback_count).toBe(antes.feedback_count);
  });

  it("'incorrect' e 'improved' incrementam feedback_count e só ele", async () => {
    const pattern = await seedPattern(USER_A);
    const antes = await contadores(pattern.id);

    await registerPatternFeedback({ pattern_id: pattern.id, feedback_type: "incorrect" });
    await registerPatternFeedback({ pattern_id: pattern.id, feedback_type: "improved" });

    const depois = await contadores(pattern.id);
    expect(depois.feedback_count).toBe(antes.feedback_count + 2);
    expect(depois.success_count).toBe(antes.success_count);
  });

  it("'partial' não mexe em contador nenhum (ambíguo por design)", async () => {
    const pattern = await seedPattern(USER_A);
    const antes = await contadores(pattern.id);

    await registerPatternFeedback({ pattern_id: pattern.id, feedback_type: "partial" });

    const depois = await contadores(pattern.id);
    expect(depois).toEqual(antes);
  });

  it("recusa feedback em padrão de outro usuário, sem gravar nada", async () => {
    const patternB = await seedPattern(USER_B);

    currentUserId = USER_A;
    await expect(
      registerPatternFeedback({ pattern_id: patternB.id, feedback_type: "correct" }),
    ).rejects.toThrow("Pattern not found or access denied");

    const db = getDb();
    const linhas = await db
      .select()
      .from(schema.pattern_feedback)
      .where(eq(schema.pattern_feedback.pattern_id, patternB.id));
    expect(linhas).toHaveLength(0);
  });

  // IDOR que a RPC original NÃO fechava: `source_document_id` ia direto para
  // a FK sem checar de quem era o documento.
  it("recusa source_document_id de outro usuário, sem gravar nada", async () => {
    const documentoB = await seedDocumento(USER_B);
    const patternA = await seedPattern(USER_A);

    currentUserId = USER_A;
    await expect(
      registerPatternFeedback({
        pattern_id: patternA.id,
        source_document_id: documentoB.id,
        feedback_type: "correct",
      }),
    ).rejects.toThrow();

    const db = getDb();
    const linhas = await db
      .select()
      .from(schema.pattern_feedback)
      .where(eq(schema.pattern_feedback.pattern_id, patternA.id));
    expect(linhas).toHaveLength(0);
    // E o contador também não pode ter andado.
    expect((await contadores(patternA.id)).success_count).toBe(0);
  });

  it("recusa feedback_type fora da lista", async () => {
    const pattern = await seedPattern(USER_A);
    await expect(
      registerPatternFeedback({
        pattern_id: pattern.id,
        // Simula entrada não tipada vinda do cliente.
        feedback_type: "excelente" as unknown as "correct",
      }),
    ).rejects.toThrow(/feedback_type/);
  });
});
