"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, inArray, lte } from "drizzle-orm";
import { requireTransactionForeignKeysOwned } from "./_shared/transaction-fk-ownership";

type Transaction = typeof schema.transactions.$inferSelect;
type TransactionInsert = Omit<typeof schema.transactions.$inferInsert, "user_id" | "id">;
type TransactionUpdate = Partial<TransactionInsert>;
type DeduplicatedExpense = typeof schema.v_expenses_deduplicated.$inferSelect;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

// A validação de posse das 8 FKs de `transactions` (achado CRITICAL da
// varredura de segurança) mora em ./_shared/transaction-fk-ownership.ts —
// compartilhada com import.ts, que grava na mesma tabela com o mesmo
// TransactionInsert e precisa da mesma checagem.

// transaction_tags.tag_id não tem user_id próprio (mesma nota do brief pra
// posse da linha inteira) — mas aponta pra `tags`, que tem. Sem checar,
// o usuário A conseguiria marcar a própria transação com a tag de outro
// usuário.
async function requireTagsOwned(tagIds: string[], userId: string): Promise<void> {
  const uniqueIds = [...new Set(tagIds)];
  if (uniqueIds.length === 0) return;
  const db = getDb();
  const rows = await db
    .select({ id: schema.tags.id })
    .from(schema.tags)
    .where(and(inArray(schema.tags.id, uniqueIds), eq(schema.tags.user_id, userId)));
  if (rows.length !== uniqueIds.length) throw new Error("Uma ou mais tags não encontradas");
}

export async function getTransactions(filters?: {
  financialProductId?: string;
  financialPeriodId?: string;
  statementCycleId?: string;
  type?: string;
  startDate?: string;
  endDate?: string;
  limit?: number;
}): Promise<Transaction[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.transactions.user_id, userId)];
  if (filters?.financialProductId)
    conditions.push(eq(schema.transactions.financial_product_id, filters.financialProductId));
  if (filters?.financialPeriodId)
    conditions.push(eq(schema.transactions.financial_period_id, filters.financialPeriodId));
  if (filters?.statementCycleId)
    conditions.push(eq(schema.transactions.statement_cycle_id, filters.statementCycleId));
  if (filters?.type) conditions.push(eq(schema.transactions.type, filters.type));
  if (filters?.startDate) conditions.push(gte(schema.transactions.event_date, filters.startDate));
  if (filters?.endDate) conditions.push(lte(schema.transactions.event_date, filters.endDate));

  const base = db
    .select()
    .from(schema.transactions)
    .where(and(...conditions))
    .orderBy(desc(schema.transactions.event_date));
  return filters?.limit ? base.limit(filters.limit) : base;
}

export async function getTransaction(id: string): Promise<Transaction | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.transactions)
    .where(and(eq(schema.transactions.id, id), eq(schema.transactions.user_id, userId)));
  return row ?? null;
}

export async function createTransaction(
  input: TransactionInsert,
  tagIds?: string[],
): Promise<Transaction> {
  const userId = await requireUserId();
  const db = getDb();

  await requireTransactionForeignKeysOwned(input, userId);
  if (tagIds && tagIds.length > 0) await requireTagsOwned(tagIds, userId);

  const [row] = await db
    .insert(schema.transactions)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar transação");

  if (tagIds && tagIds.length > 0) {
    await db
      .insert(schema.transaction_tags)
      .values(tagIds.map((tag_id) => ({ transaction_id: row.id, tag_id })));
  }

  return row;
}

export async function updateTransaction(
  id: string,
  input: TransactionUpdate,
  tagIds?: string[],
): Promise<Transaction> {
  const userId = await requireUserId();
  const db = getDb();

  await requireTransactionForeignKeysOwned(input, userId);
  if (tagIds !== undefined && tagIds.length > 0) await requireTagsOwned(tagIds, userId);

  const [row] = await db
    .update(schema.transactions)
    .set(input)
    .where(and(eq(schema.transactions.id, id), eq(schema.transactions.user_id, userId)))
    .returning();

  if (!row) throw new Error("Transação não encontrada");

  if (tagIds !== undefined) {
    await db.delete(schema.transaction_tags).where(eq(schema.transaction_tags.transaction_id, id));
    if (tagIds.length > 0) {
      await db
        .insert(schema.transaction_tags)
        .values(tagIds.map((tag_id) => ({ transaction_id: id, tag_id })));
    }
  }

  return row;
}

export async function deleteTransaction(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.transactions)
    .where(and(eq(schema.transactions.id, id), eq(schema.transactions.user_id, userId)))
    .returning({ id: schema.transactions.id });

  if (result.length === 0) throw new Error("Transação não encontrada");
}

// ── Despesas deduplicadas (ADR-001) ──

export async function getDeduplicatedExpenses(filters?: {
  financialPeriodId?: string;
  statementCycleId?: string;
  supplierId?: string;
  categoryId?: string;
}): Promise<DeduplicatedExpense[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.v_expenses_deduplicated.user_id, userId)];
  if (filters?.financialPeriodId)
    conditions.push(
      eq(schema.v_expenses_deduplicated.financial_period_id, filters.financialPeriodId),
    );
  if (filters?.statementCycleId)
    conditions.push(
      eq(schema.v_expenses_deduplicated.statement_cycle_id, filters.statementCycleId),
    );
  if (filters?.supplierId)
    conditions.push(eq(schema.v_expenses_deduplicated.supplier_id, filters.supplierId));
  if (filters?.categoryId)
    conditions.push(eq(schema.v_expenses_deduplicated.category_id, filters.categoryId));

  return db
    .select()
    .from(schema.v_expenses_deduplicated)
    .where(and(...conditions))
    .orderBy(desc(schema.v_expenses_deduplicated.event_date));
}
