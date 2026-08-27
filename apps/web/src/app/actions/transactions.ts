"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, inArray, lte } from "drizzle-orm";
import type { AnyPgColumn, PgTable } from "drizzle-orm/pg-core";

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

// IDOR fechado (achado CRITICAL da varredura de segurança): `transactions`
// tem 8 colunas de FK que vêm direto do `input` do cliente
// (financial_product_id, financial_period_id, statement_cycle_id,
// category_id, supplier_id, liability_installment_id,
// recurring_instance_id, source_document_id). Sem RLS, gravar/atualizar sem
// checar posse permite que o usuário A aponte qualquer uma delas pra um
// registro de outro usuário — `source_document_id` é o caso mais grave
// porque contorna inteiramente a validação de posse já aplicada em
// `linkTransactionToDocument` (document-transactions.ts). Este helper faz o
// mesmo `SELECT id FROM <tabela> WHERE id = <fk> AND user_id = userId` pra
// cada FK presente, antes de qualquer insert/update.
async function requireOwned(
  table: PgTable,
  idColumn: AnyPgColumn,
  userIdColumn: AnyPgColumn,
  id: string,
  userId: string,
  label: string,
): Promise<void> {
  const db = getDb();
  const [row] = await db
    .select({ id: idColumn })
    .from(table)
    .where(and(eq(idColumn, id), eq(userIdColumn, userId)));
  if (!row) throw new Error(`${label} não encontrado(a)`);
}

async function requireTransactionForeignKeysOwned(
  input: Partial<TransactionInsert>,
  userId: string,
): Promise<void> {
  if (input.financial_product_id) {
    await requireOwned(
      schema.financial_products,
      schema.financial_products.id,
      schema.financial_products.user_id,
      input.financial_product_id,
      userId,
      "Produto financeiro",
    );
  }
  if (input.financial_period_id) {
    await requireOwned(
      schema.financial_periods,
      schema.financial_periods.id,
      schema.financial_periods.user_id,
      input.financial_period_id,
      userId,
      "Período financeiro",
    );
  }
  if (input.statement_cycle_id) {
    await requireOwned(
      schema.statement_cycles,
      schema.statement_cycles.id,
      schema.statement_cycles.user_id,
      input.statement_cycle_id,
      userId,
      "Ciclo de fatura",
    );
  }
  if (input.category_id) {
    await requireOwned(
      schema.categories,
      schema.categories.id,
      schema.categories.user_id,
      input.category_id,
      userId,
      "Categoria",
    );
  }
  if (input.supplier_id) {
    await requireOwned(
      schema.suppliers,
      schema.suppliers.id,
      schema.suppliers.user_id,
      input.supplier_id,
      userId,
      "Fornecedor",
    );
  }
  if (input.liability_installment_id) {
    await requireOwned(
      schema.liability_installments,
      schema.liability_installments.id,
      schema.liability_installments.user_id,
      input.liability_installment_id,
      userId,
      "Parcela de dívida",
    );
  }
  if (input.recurring_instance_id) {
    await requireOwned(
      schema.recurring_instances,
      schema.recurring_instances.id,
      schema.recurring_instances.user_id,
      input.recurring_instance_id,
      userId,
      "Instância recorrente",
    );
  }
  // source_document_id é o achado mais grave: sem esta checagem, um usuário
  // contorna inteiramente a validação de posse de linkTransactionToDocument
  // (document-transactions.ts) só criando/editando a transação direto com o
  // documento de outro usuário.
  if (input.source_document_id) {
    await requireOwned(
      schema.source_documents,
      schema.source_documents.id,
      schema.source_documents.user_id,
      input.source_document_id,
      userId,
      "Documento de origem",
    );
  }
}

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
