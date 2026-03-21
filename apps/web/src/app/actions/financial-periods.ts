"use server";

import { createClient } from "@/lib/supabase/server";
import type { FinancialPeriod, Insert, Update } from "@sbf/shared-types";

type PeriodInsert = Omit<Insert<"financial_periods">, "user_id">;
type PeriodUpdate = Update<"financial_periods">;

// ══════════════════════════════════════════════════════════════
// Financial Periods
// ══════════════════════════════════════════════════════════════

export async function getFinancialPeriods(filters?: {
  isCurrent?: boolean;
  startDate?: string;
  endDate?: string;
}): Promise<FinancialPeriod[]> {
  const supabase = await createClient();
  let query = supabase
    .from("financial_periods")
    .select("*")
    .order("start_date", { ascending: true });

  if (filters?.isCurrent !== undefined) {
    query = query.eq("is_current", filters.isCurrent);
  }
  if (filters?.startDate) {
    query = query.gte("start_date", filters.startDate);
  }
  if (filters?.endDate) {
    query = query.lte("end_date", filters.endDate);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getCurrentFinancialPeriod(): Promise<FinancialPeriod | null> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("financial_periods")
    .select("*")
    .eq("is_current", true)
    .single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createFinancialPeriod(input: PeriodInsert): Promise<FinancialPeriod> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("financial_periods")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateFinancialPeriod(
  id: string,
  input: PeriodUpdate,
): Promise<FinancialPeriod> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("financial_periods")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteFinancialPeriod(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("financial_periods").delete().eq("id", id);

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// RPC: Gerar Períodos Financeiros
// ══════════════════════════════════════════════════════════════

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
// RPC: Encontrar Período para Data
// ══════════════════════════════════════════════════════════════

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
  const supabase = await createClient();
  const { data, error } = await supabase.from("user_financial_preferences").select("*").single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function upsertFinancialPreferences(input: {
  financial_cycle_start_day?: number | null;
  financial_cycle_anchor_date?: string | null;
  default_currency?: string;
}) {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("user_financial_preferences")
    .upsert({ ...input, user_id: user.id }, { onConflict: "user_id" })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}
