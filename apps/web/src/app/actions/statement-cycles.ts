"use server";

import { createClient } from "@/lib/supabase/server";
import type { StatementCycle, StatementItem, Insert, Update } from "@sbf/shared-types";

type CycleInsert = Omit<Insert<"statement_cycles">, "user_id">;
type CycleUpdate = Update<"statement_cycles">;
type ItemInsert = Omit<Insert<"statement_items">, "user_id">;
type ItemUpdate = Update<"statement_items">;

// ══════════════════════════════════════════════════════════════
// Statement Cycles (Faturas)
// ══════════════════════════════════════════════════════════════

export async function getStatementCycles(cardId?: string): Promise<StatementCycle[]> {
  const supabase = await createClient();
  let query = supabase.from("statement_cycles").select("*").order("due_date", { ascending: false });

  if (cardId) {
    query = query.eq("card_id", cardId);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getStatementCycle(id: string): Promise<StatementCycle | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("statement_cycles").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createStatementCycle(input: CycleInsert): Promise<StatementCycle> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("statement_cycles")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateStatementCycle(
  id: string,
  input: CycleUpdate,
): Promise<StatementCycle> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("statement_cycles")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteStatementCycle(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("statement_cycles").delete().eq("id", id);

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// Statement Items (Itens de Fatura)
// ══════════════════════════════════════════════════════════════

export async function getStatementItems(statementCycleId: string): Promise<StatementItem[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("statement_items")
    .select("*")
    .eq("statement_cycle_id", statementCycleId)
    .order("transaction_date", { ascending: true });

  if (error) throw new Error(error.message);
  return data;
}

export async function createStatementItem(input: ItemInsert): Promise<StatementItem> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("statement_items")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateStatementItem(id: string, input: ItemUpdate): Promise<StatementItem> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("statement_items")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteStatementItem(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("statement_items").delete().eq("id", id);

  if (error) throw new Error(error.message);
}
