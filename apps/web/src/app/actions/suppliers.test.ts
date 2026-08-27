import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createSupplier,
  createSupplierAlias,
  createSupplierContract,
  deleteSupplierAlias,
  deleteSupplierContract,
  getSupplier,
  getSupplierDetail,
  getSuppliers,
  updateSupplier,
  updateSupplierActive,
  updateSupplierAlias,
  updateSupplierContract,
} from "./suppliers";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("suppliers actions", () => {
  const supplierIds: string[] = [];
  const sourceDocumentIds: string[] = [];
  const institutionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sourceDocumentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await db.delete(schema.institutions).where(eq(schema.institutions.id, id));
    }
  });

  it("isola fornecedores por usuário", async () => {
    currentUserId = USER_A;
    const a = await createSupplier({ name: "Fornecedor A", type: "company" });
    supplierIds.push(a.id);
    currentUserId = USER_B;
    const b = await createSupplier({ name: "Fornecedor B", type: "company" });
    supplierIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getSuppliers(false);
    expect(listA.map((s) => s.id)).toContain(a.id);
    expect(listA.map((s) => s.id)).not.toContain(b.id);
    expect(await getSupplier(b.id)).toBeNull();
  });

  it("getSupplierDetail devolve fornecedor + aliases + contratos + documentos do próprio usuário", async () => {
    currentUserId = USER_A;
    const supplier = await createSupplier({ name: "Cemig", type: "utility" });
    supplierIds.push(supplier.id);

    const alias = await createSupplierAlias({
      supplier_id: supplier.id,
      alias_name: "Companhia Energética de Minas Gerais",
      alias_type: "former_name",
    });
    const contract = await createSupplierContract({
      supplier_id: supplier.id,
      contract_type: "utility",
      identifier: "UC-123456",
    });

    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: USER_A,
        origin_type: "manual_upload",
        origin_key: `test-${crypto.randomUUID()}`,
        filename: "fatura-cemig.pdf",
        supplier_id: supplier.id,
        metadata: { amount: 150.5 },
      })
      .returning();
    if (!doc) throw new Error("Falha ao semear documento de teste");
    sourceDocumentIds.push(doc.id);

    const detail = await getSupplierDetail(supplier.id);
    expect(detail.supplier?.id).toBe(supplier.id);
    expect(detail.aliases.map((a) => a.id)).toContain(alias.id);
    expect(detail.contracts.map((c) => c.id)).toContain(contract.id);
    expect(detail.documents.map((d) => d.id)).toContain(doc.id);

    // Teste de contrato de forma (source_documents é tabela crítica, ver spec
    // "Testes") — chaves snake_case, tipos batendo com o contrato de hoje.
    const docRef = detail.documents.find((d) => d.id === doc.id)!;
    expect(typeof docRef.id).toBe("string");
    expect(typeof docRef.filename).toBe("string");
    expect(typeof docRef.origin_type).toBe("string");
    expect(typeof docRef.status).toBe("string");
    expect(docRef).toHaveProperty("created_at");
    expect(docRef).toHaveProperty("metadata");

    currentUserId = USER_B;
    const emptyDetail = await getSupplierDetail(supplier.id);
    expect(emptyDetail.supplier).toBeNull();
    expect(emptyDetail.aliases).toEqual([]);
    expect(emptyDetail.contracts).toEqual([]);
    expect(emptyDetail.documents).toEqual([]);
  });

  it("updateSupplierActive ativa/desativa e lança erro para id inexistente", async () => {
    currentUserId = USER_A;
    const supplier = await createSupplier({ name: "Fornecedor Ativo", type: "company" });
    supplierIds.push(supplier.id);

    expect((await updateSupplierActive(supplier.id, false)).is_active).toBe(false);
    expect((await updateSupplierActive(supplier.id, true)).is_active).toBe(true);

    await expect(
      updateSupplierActive("00000000-0000-0000-0000-000000000000", false),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar fornecedor inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateSupplier("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("deleteSupplierAlias remove o alias, mas não lança erro (nem remove) para alias de outro usuário", async () => {
    currentUserId = USER_A;
    const supplierA = await createSupplier({ name: "Fornecedor Alias A", type: "company" });
    supplierIds.push(supplierA.id);
    const aliasA = await createSupplierAlias({
      supplier_id: supplierA.id,
      alias_name: "Alias A",
      alias_type: "other",
    });

    await deleteSupplierAlias(aliasA.id);
    const db = getDb();
    const [goneA] = await db
      .select()
      .from(schema.supplier_aliases)
      .where(eq(schema.supplier_aliases.id, aliasA.id));
    expect(goneA).toBeUndefined();

    currentUserId = USER_B;
    const supplierB = await createSupplier({ name: "Fornecedor Alias B", type: "company" });
    supplierIds.push(supplierB.id);
    const aliasB = await createSupplierAlias({
      supplier_id: supplierB.id,
      alias_name: "Alias B",
      alias_type: "other",
    });

    currentUserId = USER_A;
    await deleteSupplierAlias(aliasB.id); // não deve lançar
    const [stillThereB] = await db
      .select()
      .from(schema.supplier_aliases)
      .where(eq(schema.supplier_aliases.id, aliasB.id));
    expect(stillThereB).toBeDefined();
  });

  it("deleteSupplierContract remove o contrato, mas não lança erro (nem remove) para contrato de outro usuário", async () => {
    currentUserId = USER_A;
    const supplierA = await createSupplier({ name: "Fornecedor Contrato A", type: "company" });
    supplierIds.push(supplierA.id);
    const contractA = await createSupplierContract({
      supplier_id: supplierA.id,
      contract_type: "service",
      identifier: "CTR-A",
    });

    await deleteSupplierContract(contractA.id);
    const db = getDb();
    const [goneA] = await db
      .select()
      .from(schema.supplier_contracts)
      .where(eq(schema.supplier_contracts.id, contractA.id));
    expect(goneA).toBeUndefined();

    currentUserId = USER_B;
    const supplierB = await createSupplier({ name: "Fornecedor Contrato B", type: "company" });
    supplierIds.push(supplierB.id);
    const contractB = await createSupplierContract({
      supplier_id: supplierB.id,
      contract_type: "service",
      identifier: "CTR-B",
    });

    currentUserId = USER_A;
    await deleteSupplierContract(contractB.id); // não deve lançar
    const [stillThereB] = await db
      .select()
      .from(schema.supplier_contracts)
      .where(eq(schema.supplier_contracts.id, contractB.id));
    expect(stillThereB).toBeDefined();
  });

  // IDOR (critério fixo de revisão desta fase — ver Task 18 brief): funções
  // de escrita que aceitam uma FK relacionada do cliente sem validar posse.
  // `suppliers.institution_id` referencia `institutions`, tabela de outro
  // usuário; `supplier_aliases.supplier_id`/`supplier_contracts.supplier_id`
  // referenciam `suppliers` — em nenhum dos dois casos o FK em si garante
  // que a linha referenciada pertence ao usuário autenticado.
  it("createSupplier lança erro se institution_id pertence a outro usuário", async () => {
    currentUserId = USER_B;
    const db = getDb();
    const [institutionB] = await db
      .insert(schema.institutions)
      .values({ user_id: USER_B, name: "Banco B", type: "bank" })
      .returning();
    if (!institutionB) throw new Error("Falha ao semear instituição de teste");
    institutionIds.push(institutionB.id);

    currentUserId = USER_A;
    await expect(
      createSupplier({ name: "Fornecedor IDOR", type: "company", institution_id: institutionB.id }),
    ).rejects.toThrow();
  });

  it("updateSupplier lança erro se institution_id pertence a outro usuário", async () => {
    currentUserId = USER_A;
    const supplier = await createSupplier({ name: "Fornecedor Update IDOR", type: "company" });
    supplierIds.push(supplier.id);

    currentUserId = USER_B;
    const db = getDb();
    const [institutionB] = await db
      .insert(schema.institutions)
      .values({ user_id: USER_B, name: "Banco B 2", type: "bank" })
      .returning();
    if (!institutionB) throw new Error("Falha ao semear instituição de teste");
    institutionIds.push(institutionB.id);

    currentUserId = USER_A;
    await expect(
      updateSupplier(supplier.id, { institution_id: institutionB.id }),
    ).rejects.toThrow();
  });

  it("createSupplierAlias lança erro se supplier_id pertence a outro usuário", async () => {
    currentUserId = USER_B;
    const supplierB = await createSupplier({ name: "Fornecedor Alias IDOR B", type: "company" });
    supplierIds.push(supplierB.id);

    currentUserId = USER_A;
    await expect(
      createSupplierAlias({
        supplier_id: supplierB.id,
        alias_name: "Alias Malicioso",
        alias_type: "other",
      }),
    ).rejects.toThrow();
  });

  it("createSupplierContract lança erro se supplier_id pertence a outro usuário", async () => {
    currentUserId = USER_B;
    const supplierB = await createSupplier({ name: "Fornecedor Contrato IDOR B", type: "company" });
    supplierIds.push(supplierB.id);

    currentUserId = USER_A;
    await expect(
      createSupplierContract({
        supplier_id: supplierB.id,
        contract_type: "service",
        identifier: "CTR-MALICIOSO",
      }),
    ).rejects.toThrow();
  });

  it("updateSupplierAlias lança erro se tenta reatribuir supplier_id para fornecedor de outro usuário", async () => {
    currentUserId = USER_A;
    const supplierA = await createSupplier({
      name: "Fornecedor Alias Reassign A",
      type: "company",
    });
    supplierIds.push(supplierA.id);
    const aliasA = await createSupplierAlias({
      supplier_id: supplierA.id,
      alias_name: "Alias Reassign",
      alias_type: "other",
    });

    currentUserId = USER_B;
    const supplierB = await createSupplier({
      name: "Fornecedor Alias Reassign B",
      type: "company",
    });
    supplierIds.push(supplierB.id);

    currentUserId = USER_A;
    await expect(updateSupplierAlias(aliasA.id, { supplier_id: supplierB.id })).rejects.toThrow();
  });

  it("updateSupplierContract lança erro se tenta reatribuir supplier_id para fornecedor de outro usuário", async () => {
    currentUserId = USER_A;
    const supplierA = await createSupplier({
      name: "Fornecedor Contrato Reassign A",
      type: "company",
    });
    supplierIds.push(supplierA.id);
    const contractA = await createSupplierContract({
      supplier_id: supplierA.id,
      contract_type: "service",
      identifier: "CTR-REASSIGN",
    });

    currentUserId = USER_B;
    const supplierB = await createSupplier({
      name: "Fornecedor Contrato Reassign B",
      type: "company",
    });
    supplierIds.push(supplierB.id);

    currentUserId = USER_A;
    await expect(
      updateSupplierContract(contractA.id, { supplier_id: supplierB.id }),
    ).rejects.toThrow();
  });
});
