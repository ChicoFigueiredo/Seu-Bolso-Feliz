"use server";

import { createClient } from "@/lib/supabase/server";
import type { Transaction, Insert, Update } from "@sbf/shared-types";

type TransactionInsert = Omit<Insert<"transactions">, "user_id">;
type TransactionUpdate = Update<"transactions">;

export async function getTransactions(filters?: {
  financialProductId?: string;
  financialPeriodId?: string;
  statementCycleId?: string;
  type?: string;
  startDate?: string;
  endDate?: string;
  limit?: number;
}): Promise<Transaction[]> {
  const supabase = await createClient();
  let query = supabase.from("transactions").select("*").order("event_date", { ascending: false });

  if (filters?.financialProductId) {
    query = query.eq("financial_product_id", filters.financialProductId);
  }
  if (filters?.financialPeriodId) {
    query = query.eq("financial_period_id", filters.financialPeriodId);
  }
  if (filters?.statementCycleId) {
    query = query.eq("statement_cycle_id", filters.statementCycleId);
  }
  if (filters?.type) {
    query = query.eq("type", filters.type);
  }
  if (filters?.startDate) {
    query = query.gte("event_date", filters.startDate);
  }
  if (filters?.endDate) {
    query = query.lte("event_date", filters.endDate);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getTransaction(id: string): Promise<Transaction | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("transactions").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createTransaction(
  input: TransactionInsert,
  tagIds?: string[],
): Promise<Transaction> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("transactions")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);

  // Associar tags se fornecidas
  if (tagIds && tagIds.length > 0) {
    const tagInserts = tagIds.map((tag_id) => ({
      transaction_id: data.id,
      tag_id,
    }));
    const { error: tagError } = await supabase.from("transaction_tags").insert(tagInserts);
    if (tagError) throw new Error(tagError.message);
  }

  return data;
}

export async function updateTransaction(
  id: string,
  input: TransactionUpdate,
  tagIds?: string[],
): Promise<Transaction> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("transactions")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);

  // Atualizar tags se fornecidas
  if (tagIds !== undefined) {
    // Remover tags existentes
    await supabase.from("transaction_tags").delete().eq("transaction_id", id);

    // Inserir novas
    if (tagIds.length > 0) {
      const tagInserts = tagIds.map((tag_id) => ({
        transaction_id: id,
        tag_id,
      }));
      const { error: tagError } = await supabase.from("transaction_tags").insert(tagInserts);
      if (tagError) throw new Error(tagError.message);
    }
  }

  return data;
}

export async function deleteTransaction(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("transactions").delete().eq("id", id);

  if (error) throw new Error(error.message);
}

// ── Despesas deduplicadas (ADR-001) ──

export async function getDeduplicatedExpenses(filters?: {
  financialPeriodId?: string;
  statementCycleId?: string;
  supplierId?: string;
  categoryId?: string;
}) {
  const supabase = await createClient();
  let query = supabase
    .from("v_expenses_deduplicated")
    .select("*")
    .order("event_date", { ascending: false });

  if (filters?.financialPeriodId) {
    query = query.eq("financial_period_id", filters.financialPeriodId);
  }
  if (filters?.statementCycleId) {
    query = query.eq("statement_cycle_id", filters.statementCycleId);
  }
  if (filters?.supplierId) {
    query = query.eq("supplier_id", filters.supplierId);
  }
  if (filters?.categoryId) {
    query = query.eq("category_id", filters.categoryId);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}
