import "./test-db-setup"; // redireciona getDb() para TEST_DATABASE_URL — ver Task 1, Step 10
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createRecurringTemplate,
  deleteRecurringTemplate,
  getRecurringTemplate,
  getRecurringTemplates,
  updateRecurringInstance,
  updateRecurringTemplate,
} from "./recurring";

let currentUserId = "";
// A identidade vem inteiramente do nosso próprio `requireUser()`, sem dependência de provedor externo.
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

// noUncheckedIndexedAccess: insert().returning() sempre devolve a linha
// inserida — falhar aqui só seria possível por bug de infraestrutura de
// teste. Mesmo padrão de `required()` já usado em statement-cycles.test.ts.
function required<T>(value: T | undefined, message: string): T {
  if (value === undefined) throw new Error(message);
  return value;
}

async function seedCategory(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.categories)
    .values({ user_id: userId, name: `Categoria de teste ${crypto.randomUUID()}` })
    .returning();
  return required(row, "Falha ao semear category de teste");
}

async function seedSupplier(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.suppliers)
    .values({ user_id: userId, name: `Fornecedor de teste ${crypto.randomUUID()}` })
    .returning();
  return required(row, "Falha ao semear supplier de teste");
}

async function seedTag(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.tags)
    .values({ user_id: userId, name: `Tag de teste ${crypto.randomUUID()}` })
    .returning();
  return required(row, "Falha ao semear tag de teste");
}

// Institution + financial_product + transaction, mesmo padrão de
// document-transactions.test.ts (seedDocAndTx) sem a parte de source_document.
async function seedTransaction(userId: string) {
  const db = getDb();
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id);
  const [row] = await db
    .insert(schema.transactions)
    .values({
      user_id: userId,
      financial_product_id: product.id,
      type: "expense",
      event_date: "2026-01-01",
      amount: 100,
    })
    .returning();
  return {
    institutionId: institution.id,
    transaction: required(row, "Falha ao semear transaction de teste"),
  };
}

async function seedRecurringInstance(userId: string, templateId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.recurring_instances)
    .values({
      user_id: userId,
      recurring_template_id: templateId,
      expected_date: "2026-01-01",
      expected_amount: 100,
      status: "pending",
    })
    .returning();
  return required(row, "Falha ao semear recurring_instance de teste");
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("recurring actions", () => {
  const createdIds: string[] = [];
  const institutionIds: string[] = [];
  const categoryIds: string[] = [];
  const supplierIds: string[] = [];
  const tagRowIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db
        .delete(schema.recurring_template_tags)
        .where(eq(schema.recurring_template_tags.recurring_template_id, id));
      await db.delete(schema.recurring_templates).where(eq(schema.recurring_templates.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products
    }
    for (const id of categoryIds.splice(0)) {
      await db.delete(schema.categories).where(eq(schema.categories.id, id));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
    for (const id of tagRowIds.splice(0)) {
      await db.delete(schema.tags).where(eq(schema.tags.id, id));
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

  // IDOR fechado (achado da varredura retroativa, mesma classe do fix em
  // document-transactions.ts): create/update aceitavam financial_product_id/
  // category_id/supplier_id de outro usuário sem validar posse.
  it("lança erro ao criar template apontando para financial_product_id de outro usuário", async () => {
    currentUserId = USER_B;
    const institution = await seedInstitution(USER_B);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(USER_B, institution.id);

    currentUserId = USER_A;
    await expect(
      createRecurringTemplate({
        name: "Aluguel",
        type: "expense",
        frequency: "monthly",
        amount: 1500,
        financial_product_id: product.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar template apontando para category_id de outro usuário", async () => {
    currentUserId = USER_B;
    const category = await seedCategory(USER_B);
    categoryIds.push(category.id);

    currentUserId = USER_A;
    await expect(
      createRecurringTemplate({
        name: "Aluguel",
        type: "expense",
        frequency: "monthly",
        amount: 1500,
        category_id: category.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar template apontando para supplier_id de outro usuário", async () => {
    currentUserId = USER_B;
    const supplier = await seedSupplier(USER_B);
    supplierIds.push(supplier.id);

    currentUserId = USER_A;
    await expect(
      createRecurringTemplate({
        name: "Aluguel",
        type: "expense",
        frequency: "monthly",
        amount: 1500,
        supplier_id: supplier.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar template com category_id de outro usuário", async () => {
    currentUserId = USER_A;
    const own = await createRecurringTemplate({
      name: "Água",
      type: "expense",
      frequency: "monthly",
      amount: 80,
    });
    createdIds.push(own.id);

    currentUserId = USER_B;
    const category = await seedCategory(USER_B);
    categoryIds.push(category.id);

    currentUserId = USER_A;
    await expect(updateRecurringTemplate(own.id, { category_id: category.id })).rejects.toThrow();
  });

  // Achado secundário da mesma varredura: tag_id (dentro de tagIds) nunca era
  // validado contra tags.user_id antes de gravar em recurring_template_tags.
  it("lança erro ao criar template com tag_id de outro usuário", async () => {
    currentUserId = USER_B;
    const tag = await seedTag(USER_B);
    tagRowIds.push(tag.id);

    currentUserId = USER_A;
    await expect(
      createRecurringTemplate(
        {
          name: "Streaming",
          type: "expense",
          frequency: "monthly",
          amount: 40,
        },
        [tag.id],
      ),
    ).rejects.toThrow();
  });

  // Achado Important da revisão final de branch: updateRecurringInstance
  // ficou de fora do fix de IDOR acima (validação de FK só cobria
  // recurring_templates, não recurring_instances). Mesma classe de bug:
  // recurring_template_id/transaction_id vindos do cliente sem checar posse.
  it("lança erro ao atualizar instância apontando para recurring_template_id de outro usuário", async () => {
    currentUserId = USER_A;
    const ownTemplate = await createRecurringTemplate({
      name: "Academia",
      type: "expense",
      frequency: "monthly",
      amount: 90,
    });
    createdIds.push(ownTemplate.id);
    const instance = await seedRecurringInstance(USER_A, ownTemplate.id);

    currentUserId = USER_B;
    const otherTemplate = await createRecurringTemplate({
      name: "Academia B",
      type: "expense",
      frequency: "monthly",
      amount: 90,
    });
    createdIds.push(otherTemplate.id);

    currentUserId = USER_A;
    await expect(
      updateRecurringInstance(instance.id, { recurring_template_id: otherTemplate.id }),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar instância apontando para transaction_id de outro usuário", async () => {
    currentUserId = USER_A;
    const ownTemplate = await createRecurringTemplate({
      name: "Streaming 2",
      type: "expense",
      frequency: "monthly",
      amount: 45,
    });
    createdIds.push(ownTemplate.id);
    const instance = await seedRecurringInstance(USER_A, ownTemplate.id);

    currentUserId = USER_B;
    const seedB = await seedTransaction(USER_B);
    institutionIds.push(seedB.institutionId);

    currentUserId = USER_A;
    await expect(
      updateRecurringInstance(instance.id, { transaction_id: seedB.transaction.id }),
    ).rejects.toThrow();
  });
});
