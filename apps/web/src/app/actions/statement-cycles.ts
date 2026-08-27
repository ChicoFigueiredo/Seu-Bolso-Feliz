"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

type StatementCycle = typeof schema.statement_cycles.$inferSelect;
type StatementCycleInsert = Omit<typeof schema.statement_cycles.$inferInsert, "user_id" | "id">;
type StatementCycleUpdate = Partial<StatementCycleInsert>;
type StatementItem = typeof schema.statement_items.$inferSelect;
type StatementItemInsert = Omit<typeof schema.statement_items.$inferInsert, "user_id" | "id">;
type StatementItemUpdate = Partial<StatementItemInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

// Valida posse das FKs recebidas do cliente antes de gravar — sem isso, o
// usuário A poderia apontar um statement_cycle/statement_item para um
// card/statement_cycle/supplier/transaction que pertence a outro usuário. O
// RLS antigo cobria esse gap de graça; sem RLS, a checagem precisa ser
// explícita aqui (mesmo padrão do fix de document-transactions.ts, achado
// CRITICAL da revisão de segurança).

async function assertCardOwnership(cardId: string, userId: string): Promise<void> {
  const db = getDb();
  const [card] = await db
    .select({ id: schema.cards.id })
    .from(schema.cards)
    .where(and(eq(schema.cards.id, cardId), eq(schema.cards.user_id, userId)));
  if (!card) throw new Error("Cartão não encontrado");
}

async function assertStatementCycleOwnership(
  statementCycleId: string,
  userId: string,
): Promise<void> {
  const db = getDb();
  const [cycle] = await db
    .select({ id: schema.statement_cycles.id })
    .from(schema.statement_cycles)
    .where(
      and(
        eq(schema.statement_cycles.id, statementCycleId),
        eq(schema.statement_cycles.user_id, userId),
      ),
    );
  if (!cycle) throw new Error("Ciclo de fatura não encontrado");
}

async function assertSupplierOwnership(supplierId: string, userId: string): Promise<void> {
  const db = getDb();
  const [supplier] = await db
    .select({ id: schema.suppliers.id })
    .from(schema.suppliers)
    .where(and(eq(schema.suppliers.id, supplierId), eq(schema.suppliers.user_id, userId)));
  if (!supplier) throw new Error("Fornecedor não encontrado");
}

async function assertTransactionOwnership(transactionId: string, userId: string): Promise<void> {
  const db = getDb();
  const [transaction] = await db
    .select({ id: schema.transactions.id })
    .from(schema.transactions)
    .where(and(eq(schema.transactions.id, transactionId), eq(schema.transactions.user_id, userId)));
  if (!transaction) throw new Error("Transação não encontrada");
}

// ══════════════════════════════════════════════════════════════
// Statement Cycles (Faturas)
// ══════════════════════════════════════════════════════════════

export async function getStatementCycles(cardId?: string): Promise<StatementCycle[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.statement_cycles.user_id, userId)];
  if (cardId) conditions.push(eq(schema.statement_cycles.card_id, cardId));
  return db
    .select()
    .from(schema.statement_cycles)
    .where(and(...conditions))
    .orderBy(desc(schema.statement_cycles.due_date));
}

export async function getStatementCycle(id: string): Promise<StatementCycle | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.statement_cycles)
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, userId)));
  return row ?? null;
}

export async function createStatementCycle(input: StatementCycleInsert): Promise<StatementCycle> {
  const userId = await requireUserId();
  await assertCardOwnership(input.card_id, userId);
  const db = getDb();
  const [row] = await db
    .insert(schema.statement_cycles)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar ciclo de fatura");
  return row;
}

export async function updateStatementCycle(
  id: string,
  input: StatementCycleUpdate,
): Promise<StatementCycle> {
  const userId = await requireUserId();
  if (input.card_id !== undefined) await assertCardOwnership(input.card_id, userId);
  const db = getDb();
  const [row] = await db
    .update(schema.statement_cycles)
    .set(input)
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, userId)))
    .returning();
  if (!row) throw new Error("Ciclo de fatura não encontrado");
  return row;
}

export async function deleteStatementCycle(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.statement_cycles)
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, userId)));
}

// ══════════════════════════════════════════════════════════════
// Statement Items (Itens de Fatura)
// ══════════════════════════════════════════════════════════════

export async function getStatementItems(statementCycleId: string): Promise<StatementItem[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.statement_items)
    .where(
      and(
        eq(schema.statement_items.statement_cycle_id, statementCycleId),
        eq(schema.statement_items.user_id, userId),
      ),
    )
    .orderBy(asc(schema.statement_items.transaction_date));
}

export async function createStatementItem(input: StatementItemInsert): Promise<StatementItem> {
  const userId = await requireUserId();
  await assertStatementCycleOwnership(input.statement_cycle_id, userId);
  if (input.supplier_id) await assertSupplierOwnership(input.supplier_id, userId);
  if (input.transaction_id) await assertTransactionOwnership(input.transaction_id, userId);
  const db = getDb();
  const [row] = await db
    .insert(schema.statement_items)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar item de fatura");
  return row;
}

export async function updateStatementItem(
  id: string,
  input: StatementItemUpdate,
): Promise<StatementItem> {
  const userId = await requireUserId();
  if (input.statement_cycle_id !== undefined) {
    await assertStatementCycleOwnership(input.statement_cycle_id, userId);
  }
  if (input.supplier_id) await assertSupplierOwnership(input.supplier_id, userId);
  if (input.transaction_id) await assertTransactionOwnership(input.transaction_id, userId);
  const db = getDb();
  const [row] = await db
    .update(schema.statement_items)
    .set(input)
    .where(and(eq(schema.statement_items.id, id), eq(schema.statement_items.user_id, userId)))
    .returning();
  if (!row) throw new Error("Item de fatura não encontrado");
  return row;
}

export async function deleteStatementItem(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.statement_items)
    .where(and(eq(schema.statement_items.id, id), eq(schema.statement_items.user_id, userId)));
}
