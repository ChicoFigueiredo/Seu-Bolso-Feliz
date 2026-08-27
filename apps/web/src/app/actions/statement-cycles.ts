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
