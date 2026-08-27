"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, gte, lte } from "drizzle-orm";

type FinancialPeriod = typeof schema.financial_periods.$inferSelect;
type FinancialPeriodInsert = Omit<typeof schema.financial_periods.$inferInsert, "user_id" | "id">;
type FinancialPeriodUpdate = Partial<FinancialPeriodInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

// ══════════════════════════════════════════════════════════════
// Financial Periods
// ══════════════════════════════════════════════════════════════

export async function getFinancialPeriods(filters?: {
  isCurrent?: boolean;
  startDate?: string;
  endDate?: string;
}): Promise<FinancialPeriod[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.financial_periods.user_id, userId)];
  if (filters?.isCurrent !== undefined) {
    conditions.push(eq(schema.financial_periods.is_current, filters.isCurrent));
  }
  if (filters?.startDate) {
    conditions.push(gte(schema.financial_periods.start_date, filters.startDate));
  }
  if (filters?.endDate) {
    conditions.push(lte(schema.financial_periods.end_date, filters.endDate));
  }

  return db
    .select()
    .from(schema.financial_periods)
    .where(and(...conditions))
    .orderBy(asc(schema.financial_periods.start_date));
}

export async function getCurrentFinancialPeriod(): Promise<FinancialPeriod | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.financial_periods)
    .where(
      and(
        eq(schema.financial_periods.is_current, true),
        eq(schema.financial_periods.user_id, userId),
      ),
    );
  return row ?? null;
}

export async function createFinancialPeriod(
  input: FinancialPeriodInsert,
): Promise<FinancialPeriod> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.financial_periods)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar período financeiro");
  return row;
}

export async function updateFinancialPeriod(
  id: string,
  input: FinancialPeriodUpdate,
): Promise<FinancialPeriod> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.financial_periods)
    .set(input)
    .where(and(eq(schema.financial_periods.id, id), eq(schema.financial_periods.user_id, userId)))
    .returning();
  if (!row) throw new Error("Período financeiro não encontrado");
  return row;
}

export async function deleteFinancialPeriod(id: string): Promise<void> {
  // Nota: o código Supabase original não faz `.select().single()` aqui — só
  // verifica `error`. 0 linhas afetadas (id inexistente ou de outro usuário)
  // não lança erro hoje; esta reescrita preserva esse comportamento (exceção
  // documentada à regra geral de "lança erro em 0 linhas").
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.financial_periods)
    .where(and(eq(schema.financial_periods.id, id), eq(schema.financial_periods.user_id, userId)));
}

// ══════════════════════════════════════════════════════════════
// RPC: Gerar Períodos Financeiros — BLOQUEADA, NÃO REESCRITA NESTA TASK
// ══════════════════════════════════════════════════════════════
//
// generate_financial_periods foi descartada por completo na migração pro Neon
// (packages/db/schema.sql, nota de gaps item 4). Decisão de recriar com
// p_user_id explícito ou reimplementar em TypeScript é separada desta fase.
// Permanece EXATAMENTE como está hoje:

export async function generateFinancialPeriods(
  startDay: number,
  monthsAhead: number = 12,
  anchorDate?: string,
): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.rpc("generate_financial_periods", {
    p_start_day: startDay,
    p_months_ahead: monthsAhead,
    p_anchor_date: anchorDate ?? new Date().toISOString().split("T")[0]!,
  });

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// RPC: Encontrar Período para Data — BLOQUEADA, NÃO REESCRITA NESTA TASK
// ══════════════════════════════════════════════════════════════
//
// get_financial_period_for_date também está na lista de "SEIS funções
// descartadas por inteiro". Mesma decisão pendente. Permanece como está hoje:

export async function getFinancialPeriodForDate(date: string): Promise<FinancialPeriod | null> {
  const supabase = await createClient();
  const { data: periodId, error } = await supabase.rpc("get_financial_period_for_date", {
    p_date: date,
  });

  if (error) throw new Error(error.message);
  if (!periodId) return null;

  const { data, error: fetchError } = await supabase
    .from("financial_periods")
    .select("*")
    .eq("id", periodId)
    .single();

  if (fetchError) throw new Error(fetchError.message);
  return data;
}

// ══════════════════════════════════════════════════════════════
// User Financial Preferences
// ══════════════════════════════════════════════════════════════

export async function getFinancialPreferences() {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, userId));
  return row ?? null;
}

export async function upsertFinancialPreferences(input: {
  financial_cycle_start_day?: number | null;
  financial_cycle_anchor_date?: string | null;
  default_currency?: string;
}) {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.user_financial_preferences)
    .values({ ...input, user_id: userId })
    .onConflictDoUpdate({
      target: schema.user_financial_preferences.user_id,
      set: input,
    })
    .returning();
  return row;
}
