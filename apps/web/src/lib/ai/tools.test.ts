import "../../app/actions/test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  cleanupInstitution,
  seedFinancialProduct,
  seedInstitution,
} from "../../app/actions/__tests__/fixtures";
import {
  approveDraft,
  listPendingDocuments,
  listRecentTransactions,
  registerDocumentPattern,
} from "./tools";

// Sessão corrente do teste. `id` é o ATOR, `ownerId` é o DONO dos dados —
// campos distintos de propósito (apps/web/src/lib/auth/session.ts). Os testes
// que os deixam DIFERENTES são os que impedem a troca silenciosa de um pelo
// outro no código.
let sessao: { id: string; ownerId: string } | null = null;

vi.mock("@/lib/auth", () => ({
  getUserOrNull: async () =>
    sessao ? { ...sessao, email: "teste@sbf.local", role: "owner" as const, name: null } : null,
  requireUser: async () => {
    if (!sessao) throw new Error("NEXT_REDIRECT");
    return { ...sessao, email: "teste@sbf.local", role: "owner" as const, name: null };
  },
}));

/** Açúcar para o caso comum: dono operando a própria conta. */
function entraComo(userId: string) {
  sessao = { id: userId, ownerId: userId };
}

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";
// Administrador de emergência: age em nome de USER_A, com id próprio.
const ADMIN = "00000000-0000-0000-0000-00000000ad11";
// Sem `as const`: `ToolExecutionOptions.messages` é `CoreMessage[]` (mutável)
// — um tuplo `readonly []` não é atribuível a ele.
const toolOpts = { toolCallId: "test", messages: [] };

async function seedSourceDocument(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: `test::${crypto.randomUUID()}`,
      filename: "doc.pdf",
      status: "new",
    })
    .returning();
  if (!row) throw new Error("Falha ao semear source_document de teste");
  return row;
}

async function seedDraft(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.draft_records)
    .values({
      user_id: userId,
      draft_type: "transaction",
      status: "pending_review",
      draft_data: { valor: 100 },
      materialization_key: `test::${crypto.randomUUID()}`,
    })
    .returning();
  if (!row) throw new Error("Falha ao semear draft_record de teste");
  return row;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("lib/ai/tools.ts", () => {
  const docIds: string[] = [];
  const draftIds: string[] = [];
  const institutionIds: string[] = [];
  const categoryIds: string[] = [];
  const supplierIds: string[] = [];
  const patternIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0))
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    for (const id of docIds.splice(0))
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    for (const id of patternIds.splice(0))
      await db.delete(schema.document_patterns).where(eq(schema.document_patterns.id, id));
    for (const id of categoryIds.splice(0))
      await db.delete(schema.categories).where(eq(schema.categories.id, id));
    for (const id of supplierIds.splice(0))
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    for (const id of institutionIds.splice(0)) await cleanupInstitution(id);
  });

  it("list_pending_documents isola documentos por usuário", async () => {
    const docA = await seedSourceDocument(USER_A);
    docIds.push(docA.id);
    const docB = await seedSourceDocument(USER_B);
    docIds.push(docB.id);

    entraComo(USER_A);
    const result = await listPendingDocuments.execute!({ limit: 10 }, toolOpts);
    expect("documents" in result).toBe(true);
    if ("documents" in result && result.documents) {
      const ids = result.documents.map((d) => d.id);
      expect(ids).toContain(docA.id);
      expect(ids).not.toContain(docB.id);
    }
  });

  // Este é o teste que separa `ownerId` de `id`: para o dono comum os dois
  // são iguais e trocar um pelo outro passaria despercebido até o dia em que
  // o administrador de emergência entrasse e visse um app vazio.
  it("list_pending_documents segue o ownerId da sessão, não o id do ator", async () => {
    const docDoDono = await seedSourceDocument(USER_A);
    docIds.push(docDoDono.id);
    const docDoAtor = await seedSourceDocument(ADMIN);
    docIds.push(docDoAtor.id);

    // Administrador de emergência operando a conta de USER_A.
    sessao = { id: ADMIN, ownerId: USER_A };
    const result = await listPendingDocuments.execute!({ limit: 50 }, toolOpts);

    expect("documents" in result).toBe(true);
    if ("documents" in result && result.documents) {
      const ids = result.documents.map((d) => d.id);
      expect(ids).toContain(docDoDono.id);
      expect(ids).not.toContain(docDoAtor.id);
    }
  });

  it("approve_draft aprova um draft do próprio usuário (caminho feliz)", async () => {
    entraComo(USER_A);
    const draft = await seedDraft(USER_A);
    draftIds.push(draft.id);

    const result = await approveDraft.execute!({ draftId: draft.id }, toolOpts);
    expect(result).toMatchObject({ success: true });

    const db = getDb();
    const [updated] = await db
      .select()
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, draft.id));
    expect(updated?.status).toBe("approved");
  });

  it("approve_draft NÃO altera draft de outro usuário, mesmo reportando success (quirk preservado)", async () => {
    const draftB = await seedDraft(USER_B);
    draftIds.push(draftB.id);

    entraComo(USER_A);
    const result = await approveDraft.execute!({ draftId: draftB.id }, toolOpts);
    expect(result).toMatchObject({ success: true }); // comportamento pré-existente, documentado no código

    const db = getDb();
    const [stillPending] = await db
      .select()
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, draftB.id));
    expect(stillPending?.status).toBe("pending_review"); // linha de outro usuário não foi tocada
  });

  it("list_recent_transactions traz nome de categoria e fornecedor via join", async () => {
    entraComo(USER_A);
    const institution = await seedInstitution(USER_A);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(USER_A, institution.id, "checking_account");

    const db = getDb();
    const [category] = await db
      .insert(schema.categories)
      .values({ user_id: USER_A, name: "Alimentação" })
      .returning();
    if (!category) throw new Error("Falha ao semear category de teste");
    categoryIds.push(category.id);
    const [supplier] = await db
      .insert(schema.suppliers)
      .values({ user_id: USER_A, name: "Mercado X" })
      .returning();
    if (!supplier) throw new Error("Falha ao semear supplier de teste");
    supplierIds.push(supplier.id);

    const [tx] = await db
      .insert(schema.transactions)
      .values({
        user_id: USER_A,
        financial_product_id: product.id,
        type: "expense",
        amount: 55.9,
        event_date: "2026-02-10",
        category_id: category.id,
        supplier_id: supplier.id,
      })
      .returning();
    if (!tx) throw new Error("Falha ao semear transaction de teste");

    const result = await listRecentTransactions.execute!({ limit: 10 }, toolOpts);
    expect("transactions" in result).toBe(true);
    if ("transactions" in result && result.transactions) {
      const found = result.transactions.find((t) => t.id === tx.id);
      expect(found?.category_name).toBe("Alimentação");
      expect(found?.supplier_name).toBe("Mercado X");
    }
  });

  // IDOR fechado nesta task (achado registrado desde a Task 15/patterns.ts):
  // register_document_pattern aceitava supplierId/institutionId do cliente
  // sem validar posse. Mesmo padrão de teste de patterns.test.ts.
  it("register_document_pattern recusa supplierId de outro usuário e não grava nada", async () => {
    const db = getDb();
    const [supplierB] = await db
      .insert(schema.suppliers)
      .values({ user_id: USER_B, name: "Fornecedor de B" })
      .returning();
    if (!supplierB) throw new Error("Falha ao semear supplier de teste");
    supplierIds.push(supplierB.id);

    entraComo(USER_A);
    const patternName = `Tentativa cross-user ${crypto.randomUUID()}`;
    const result = await registerDocumentPattern.execute!(
      {
        name: patternName,
        documentType: "boleto",
        confidenceThreshold: 0.7,
        supplierId: supplierB.id,
      },
      toolOpts,
    );
    expect("error" in result).toBe(true);
    if ("pattern" in result && result.pattern) patternIds.push(result.pattern.id);

    const rows = await db
      .select({ id: schema.document_patterns.id })
      .from(schema.document_patterns)
      .where(eq(schema.document_patterns.name, patternName));
    expect(rows).toHaveLength(0);
  });

  it("register_document_pattern recusa institutionId de outro usuário e não grava nada", async () => {
    const institutionB = await seedInstitution(USER_B);
    institutionIds.push(institutionB.id);

    entraComo(USER_A);
    const patternName = `Tentativa cross-user ${crypto.randomUUID()}`;
    const result = await registerDocumentPattern.execute!(
      {
        name: patternName,
        documentType: "boleto",
        confidenceThreshold: 0.7,
        institutionId: institutionB.id,
      },
      toolOpts,
    );
    expect("error" in result).toBe(true);
    if ("pattern" in result && result.pattern) patternIds.push(result.pattern.id);

    const db = getDb();
    const rows = await db
      .select({ id: schema.document_patterns.id })
      .from(schema.document_patterns)
      .where(eq(schema.document_patterns.name, patternName));
    expect(rows).toHaveLength(0);
  });

  it("register_document_pattern aceita supplierId/institutionId do próprio usuário (caminho feliz)", async () => {
    entraComo(USER_A);
    const institution = await seedInstitution(USER_A);
    institutionIds.push(institution.id);

    const db = getDb();
    const [supplier] = await db
      .insert(schema.suppliers)
      .values({ user_id: USER_A, name: "Fornecedor de A" })
      .returning();
    if (!supplier) throw new Error("Falha ao semear supplier de teste");
    supplierIds.push(supplier.id);

    const patternName = `Padrão próprio ${crypto.randomUUID()}`;
    const result = await registerDocumentPattern.execute!(
      {
        name: patternName,
        documentType: "boleto",
        confidenceThreshold: 0.7,
        supplierId: supplier.id,
        institutionId: institution.id,
      },
      toolOpts,
    );
    expect(result).toMatchObject({ success: true });
    if ("pattern" in result && result.pattern) patternIds.push(result.pattern.id);
  });
});
