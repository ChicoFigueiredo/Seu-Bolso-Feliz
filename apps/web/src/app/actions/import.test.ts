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
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("import actions", () => {
  const institutionIds: string[] = [];
  const importJobIds: string[] = [];
  const transactionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of transactionIds.splice(0)) {
      await db.delete(schema.transactions).where(eq(schema.transactions.id, id));
    }
    for (const id of importJobIds.splice(0)) {
      await db.delete(schema.import_jobs).where(eq(schema.import_jobs.id, id));
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
});
