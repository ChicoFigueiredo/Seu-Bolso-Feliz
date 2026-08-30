import "./test-db-setup"; // redireciona getDb() para TEST_DATABASE_URL — ver Task 1, Step 10
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createImportJob,
  createTransactionFromImport,
  getDefaultFinancialProductId,
  getImportJobStatus,
  updateImportJob,
} from "./import";

let currentUserId = "";
// A identidade vem do nosso próprio `requireUser()` desde a saída do Supabase.
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

describe.skipIf(!process.env.TEST_DATABASE_URL)("import actions", () => {
  const institutionIds: string[] = [];
  const importJobIds: string[] = [];
  const transactionIds: string[] = [];
  const sourceDocumentIds: string[] = [];
  const categoryIds: string[] = [];
  const supplierIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of transactionIds.splice(0)) {
      await db.delete(schema.transactions).where(eq(schema.transactions.id, id));
    }
    for (const id of importJobIds.splice(0)) {
      await db.delete(schema.import_jobs).where(eq(schema.import_jobs.id, id));
    }
    for (const id of sourceDocumentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
    for (const id of categoryIds.splice(0)) {
      await db.delete(schema.categories).where(eq(schema.categories.id, id));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id);
    }
  });

  it("isola jobs de importação e produto padrão por usuário", async () => {
    currentUserId = USER_A;
    const jobA = await createImportJob({ source_type: "csv", status: "processing", total_rows: 3 });
    importJobIds.push(jobA.id);
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const productA = await seedFinancialProduct(USER_A, instA.id, "checking_account");

    currentUserId = USER_B;
    const jobB = await createImportJob({ source_type: "csv", status: "processing", total_rows: 1 });
    importJobIds.push(jobB.id);
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);
    await seedFinancialProduct(USER_B, instB.id, "checking_account");

    currentUserId = USER_A;
    expect(await getImportJobStatus(jobB.id)).toBeNull();
    expect(await getDefaultFinancialProductId()).toBe(productA.id);
  });

  it("cria job, produto padrão, transação e atualiza o job (caminho feliz)", async () => {
    currentUserId = USER_A;
    const institution = await seedInstitution(USER_A);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(USER_A, institution.id, "checking_account");

    const job = await createImportJob({ source_type: "csv", status: "processing", total_rows: 1 });
    importJobIds.push(job.id);

    const defaultProductId = await getDefaultFinancialProductId();
    expect(defaultProductId).toBe(product.id);

    const tx = await createTransactionFromImport({
      description: "Linha 1",
      amount: 42.5,
      type: "expense",
      event_date: "2026-01-15",
      financial_product_id: defaultProductId!,
      origin_type: "import",
      is_confirmed: true,
    });
    transactionIds.push(tx.id);
    expect(tx.amount).toBe(42.5);

    await updateImportJob(job.id, {
      status: "completed",
      imported_rows: 1,
      skipped_rows: 0,
      error_rows: 0,
      completed_at: new Date().toISOString(),
    });

    const updated = await getImportJobStatus(job.id);
    expect(updated?.status).toBe("completed");
    expect(updated?.imported_rows).toBe(1);
  });

  it("createTransactionFromImport lança ao violar FK de financial_product_id inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      createTransactionFromImport({
        description: "Linha inválida",
        amount: 10,
        type: "expense",
        event_date: "2026-01-15",
        financial_product_id: "00000000-0000-0000-0000-000000000000",
        origin_type: "import",
        is_confirmed: true,
      }),
    ).rejects.toThrow();
  });

  it("createTransactionFromImport lança ao receber financial_product_id de outro usuário (IDOR)", async () => {
    currentUserId = USER_B;
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);
    const productB = await seedFinancialProduct(USER_B, instB.id, "checking_account");

    currentUserId = USER_A;
    await expect(
      createTransactionFromImport({
        description: "Tentativa de IDOR",
        amount: 10,
        type: "expense",
        event_date: "2026-01-15",
        financial_product_id: productB.id,
        origin_type: "import",
        is_confirmed: true,
      }),
    ).rejects.toThrow();
  });

  // Achado CRITICAL da revisão final de branch: createTransactionFromImport
  // recebe o TransactionInsert completo (8 FKs), não só financial_product_id.
  // source_document_id é o caso mais grave — sem checar posse, um usuário
  // contorna inteiramente a validação de linkTransactionToDocument
  // (document-transactions.ts) só criando a transação direto com o
  // documento de outro usuário.
  it("createTransactionFromImport lança ao receber source_document_id de outro usuário (IDOR crítico)", async () => {
    currentUserId = USER_B;
    const db = getDb();
    const [docB] = await db
      .insert(schema.source_documents)
      .values({
        user_id: USER_B,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "teste.pdf",
        status: "new",
      })
      .returning();
    if (!docB) throw new Error("Falha ao semear source_document de teste");
    sourceDocumentIds.push(docB.id);

    currentUserId = USER_A;
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const productA = await seedFinancialProduct(USER_A, instA.id, "checking_account");

    await expect(
      createTransactionFromImport({
        description: "Tentativa de IDOR via source_document_id",
        amount: 10,
        type: "expense",
        event_date: "2026-01-15",
        financial_product_id: productA.id,
        origin_type: "import",
        is_confirmed: true,
        source_document_id: docB.id,
      }),
    ).rejects.toThrow();
  });

  it("createTransactionFromImport lança ao receber category_id de outro usuário (IDOR)", async () => {
    currentUserId = USER_B;
    const db = getDb();
    const [catB] = await db
      .insert(schema.categories)
      .values({ user_id: USER_B, name: "Categoria B" })
      .returning();
    if (!catB) throw new Error("Falha ao semear category de teste");
    categoryIds.push(catB.id);

    currentUserId = USER_A;
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const productA = await seedFinancialProduct(USER_A, instA.id, "checking_account");

    await expect(
      createTransactionFromImport({
        description: "Tentativa de IDOR via category_id",
        amount: 10,
        type: "expense",
        event_date: "2026-01-15",
        financial_product_id: productA.id,
        origin_type: "import",
        is_confirmed: true,
        category_id: catB.id,
      }),
    ).rejects.toThrow();
  });

  it("createTransactionFromImport lança ao receber supplier_id de outro usuário (IDOR)", async () => {
    currentUserId = USER_B;
    const db = getDb();
    const [supB] = await db
      .insert(schema.suppliers)
      .values({ user_id: USER_B, name: "Fornecedor B" })
      .returning();
    if (!supB) throw new Error("Falha ao semear supplier de teste");
    supplierIds.push(supB.id);

    currentUserId = USER_A;
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const productA = await seedFinancialProduct(USER_A, instA.id, "checking_account");

    await expect(
      createTransactionFromImport({
        description: "Tentativa de IDOR via supplier_id",
        amount: 10,
        type: "expense",
        event_date: "2026-01-15",
        financial_product_id: productA.id,
        origin_type: "import",
        is_confirmed: true,
        supplier_id: supB.id,
      }),
    ).rejects.toThrow();
  });
});
