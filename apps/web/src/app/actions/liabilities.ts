"use server";

import { createClient } from "@/lib/supabase/server";
import type { Liability, LiabilityInstallment, Insert, Update } from "@sbf/shared-types";

type LiabilityInsert = Omit<Insert<"liabilities">, "user_id">;
type LiabilityUpdate = Update<"liabilities">;
type InstallmentInsert = Omit<Insert<"liability_installments">, "user_id">;
type InstallmentUpdate = Update<"liability_installments">;

// ══════════════════════════════════════════════════════════════
// Liabilities (Dívidas / Passivos)
// ══════════════════════════════════════════════════════════════

export async function getLiabilities(filters?: {
  status?: string;
  financialProductId?: string;
}): Promise<Liability[]> {
  const supabase = await createClient();
  let query = supabase.from("liabilities").select("*").order("name");

  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.financialProductId) {
    query = query.eq("financial_product_id", filters.financialProductId);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getLiability(id: string): Promise<Liability | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("liabilities").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createLiability(
  input: LiabilityInsert,
  tagIds?: string[],
): Promise<Liability> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("liabilities")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);

  if (tagIds && tagIds.length > 0) {
    const tagInserts = tagIds.map((tag_id) => ({
      liability_id: data.id,
      tag_id,
    }));
    const { error: tagError } = await supabase.from("liability_tags").insert(tagInserts);
    if (tagError) throw new Error(tagError.message);
  }

  return data;
}

export async function updateLiability(
  id: string,
  input: LiabilityUpdate,
  tagIds?: string[],
): Promise<Liability> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("liabilities")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);

  if (tagIds !== undefined) {
    await supabase.from("liability_tags").delete().eq("liability_id", id);
    if (tagIds.length > 0) {
      const tagInserts = tagIds.map((tag_id) => ({
        liability_id: id,
        tag_id,
      }));
      const { error: tagError } = await supabase.from("liability_tags").insert(tagInserts);
      if (tagError) throw new Error(tagError.message);
    }
  }

  return data;
}

export async function deleteLiability(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("liabilities").delete().eq("id", id);

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// Liability Installments (Parcelas de Dívida)
// ══════════════════════════════════════════════════════════════

export async function getLiabilityInstallments(
  liabilityId: string,
  filters?: { status?: string },
): Promise<LiabilityInstallment[]> {
  const supabase = await createClient();
  let query = supabase
    .from("liability_installments")
    .select("*")
    .eq("liability_id", liabilityId)
    .order("installment_number", { ascending: true });

  if (filters?.status) {
    query = query.eq("status", filters.status);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function createLiabilityInstallment(
  input: InstallmentInsert,
): Promise<LiabilityInstallment> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("liability_installments")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateLiabilityInstallment(
  id: string,
  input: InstallmentUpdate,
): Promise<LiabilityInstallment> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("liability_installments")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteLiabilityInstallment(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("liability_installments").delete().eq("id", id);

  if (error) throw new Error(error.message);
}
