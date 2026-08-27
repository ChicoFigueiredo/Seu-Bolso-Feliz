import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  cleanupInstitution,
  seedCard,
  seedFinancialProduct,
  seedInstitution,
} from "./__tests__/fixtures";
import {
  createStatementCycle,
  createStatementItem,
  deleteStatementCycle,
  getStatementCycle,
  getStatementCycles,
  updateStatementCycle,
  updateStatementItem,
} from "./statement-cycles";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedCardFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id, "credit_card");
  const card = await seedCard(userId, product.id);
  return { institutionId: institution.id, product, card };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("statement-cycles actions", () => {
  const institutionIds: string[] = [];
  // `suppliers` não entra no CASCADE de institutions (a FK
  // suppliers.institution_id usa ON DELETE SET NULL, não CASCADE) — precisa
  // de limpeza manual.
  const supplierIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/cards/statement_cycles/transactions
    }
  });

  // Semeia card + ciclo de fatura + supplier + transaction, todos pertencentes
  // a `userId` — usado pelos testes de IDOR abaixo para ter um recurso "de
  // outro usuário" a apontar indevidamente.
  async function seedResourcesFor(userId: string) {
    const previousUserId = currentUserId;
    currentUserId = userId;
    const seed = await seedCardFor(userId);
    const cycle = await createStatementCycle({
      card_id: seed.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    const db = getDb();
    const [supplier] = await db
      .insert(schema.suppliers)
      .values({ user_id: userId, name: `Fornecedor de teste ${crypto.randomUUID()}` })
      .returning();
    if (!supplier) throw new Error("Falha ao semear supplier de teste");

    const [transaction] = await db
      .insert(schema.transactions)
      .values({
        user_id: userId,
        financial_product_id: seed.product.id,
        type: "expense",
        amount: 10,
        event_date: "2026-01-01",
      })
      .returning();
    if (!transaction) throw new Error("Falha ao semear transaction de teste");

    currentUserId = previousUserId;
    return { institutionId: seed.institutionId, card: seed.card, cycle, supplier, transaction };
  }

  it("isola ciclos de fatura por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedCardFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createStatementCycle({
      card_id: seedA.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    currentUserId = USER_B;
    const seedB = await seedCardFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createStatementCycle({
      card_id: seedB.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    currentUserId = USER_A;
    const listA = await getStatementCycles();
    expect(listA.map((c) => c.id)).toContain(a.id);
    expect(listA.map((c) => c.id)).not.toContain(b.id);
    expect(await getStatementCycle(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedCardFor(USER_A);
    institutionIds.push(seed.institutionId);

    const created = await createStatementCycle({
      card_id: seed.card.id,
      reference_month: "2026-02-01",
      cycle_start_date: "2026-01-01",
      cycle_end_date: "2026-01-31",
      due_date: "2026-02-10",
    });

    const updated = await updateStatementCycle(created.id, { due_date: "2026-02-12" });
    expect(updated.due_date).toBe("2026-02-12");

    await deleteStatementCycle(created.id);
    expect(await getStatementCycle(created.id)).toBeNull();
  });

  it("lança erro ao atualizar ciclo inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateStatementCycle("00000000-0000-0000-0000-000000000000", { due_date: "2026-01-01" }),
    ).rejects.toThrow();
  });

  // IDOR fechado (achado CRITICAL da varredura de segurança): createStatementCycle
  // não validava posse de card_id antes de gravar — um usuário conseguia criar
  // um ciclo de fatura apontando para o cartão de outro usuário.
  it("lança erro ao criar ciclo de fatura com card_id de outro usuário", async () => {
    currentUserId = USER_B;
    const seedB = await seedCardFor(USER_B);
    institutionIds.push(seedB.institutionId);

    currentUserId = USER_A;
    await expect(
      createStatementCycle({
        card_id: seedB.card.id,
        reference_month: "2026-01-01",
        cycle_start_date: "2025-12-01",
        cycle_end_date: "2025-12-31",
        due_date: "2026-01-10",
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar ciclo de fatura apontando card_id para outro usuário", async () => {
    currentUserId = USER_B;
    const seedB = await seedCardFor(USER_B);
    institutionIds.push(seedB.institutionId);

    currentUserId = USER_A;
    const seedA = await seedCardFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const created = await createStatementCycle({
      card_id: seedA.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    await expect(updateStatementCycle(created.id, { card_id: seedB.card.id })).rejects.toThrow();
  });

  // IDOR fechado (achado CRITICAL da varredura de segurança, reproduzido
  // empiricamente pelo varredor): createStatementItem não validava posse de
  // statement_cycle_id/supplier_id/transaction_id — um usuário B conseguia
  // criar um statement_item apontando para o statement_cycle_id de A.
  it("lança erro ao criar item de fatura apontando para statement_cycle de outro usuário", async () => {
    const b = await seedResourcesFor(USER_B);
    institutionIds.push(b.institutionId);
    supplierIds.push(b.supplier.id);

    currentUserId = USER_A;
    await expect(
      createStatementItem({ statement_cycle_id: b.cycle.id, amount: 10 }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar item de fatura apontando para supplier de outro usuário", async () => {
    const b = await seedResourcesFor(USER_B);
    institutionIds.push(b.institutionId);
    supplierIds.push(b.supplier.id);

    const a = await seedResourcesFor(USER_A);
    institutionIds.push(a.institutionId);
    supplierIds.push(a.supplier.id);

    currentUserId = USER_A;
    await expect(
      createStatementItem({
        statement_cycle_id: a.cycle.id,
        amount: 10,
        supplier_id: b.supplier.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao criar item de fatura apontando para transaction de outro usuário", async () => {
    const b = await seedResourcesFor(USER_B);
    institutionIds.push(b.institutionId);
    supplierIds.push(b.supplier.id);

    const a = await seedResourcesFor(USER_A);
    institutionIds.push(a.institutionId);
    supplierIds.push(a.supplier.id);

    currentUserId = USER_A;
    await expect(
      createStatementItem({
        statement_cycle_id: a.cycle.id,
        amount: 10,
        transaction_id: b.transaction.id,
      }),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar item de fatura apontando statement_cycle_id para outro usuário", async () => {
    const b = await seedResourcesFor(USER_B);
    institutionIds.push(b.institutionId);
    supplierIds.push(b.supplier.id);

    const a = await seedResourcesFor(USER_A);
    institutionIds.push(a.institutionId);
    supplierIds.push(a.supplier.id);

    currentUserId = USER_A;
    const item = await createStatementItem({ statement_cycle_id: a.cycle.id, amount: 10 });

    await expect(
      updateStatementItem(item.id, { statement_cycle_id: b.cycle.id }),
    ).rejects.toThrow();
  });
});
