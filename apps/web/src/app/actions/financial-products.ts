"use server";

import { createClient } from "@/lib/supabase/server";
import type { FinancialProduct, Insert, Update } from "@sbf/shared-types";

type ProductInsert = Omit<Insert<"financial_products">, "user_id">;
type ProductUpdate = Update<"financial_products">;

export async function getFinancialProducts(institutionId?: string): Promise<FinancialProduct[]> {
  const supabase = await createClient();
  let query = supabase
    .from("financial_products")
    .select("*")
    .order("display_order", { ascending: true, nullsFirst: false })
    .order("name");

  if (institutionId) {
    query = query.eq("institution_id", institutionId);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getFinancialProduct(id: string): Promise<FinancialProduct | null> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("financial_products")
    .select("*")
    .eq("id", id)
    .single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createFinancialProduct(input: ProductInsert): Promise<FinancialProduct> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("financial_products")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateFinancialProduct(
  id: string,
  input: ProductUpdate,
): Promise<FinancialProduct> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("financial_products")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteFinancialProduct(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("financial_products").delete().eq("id", id);

  if (error) throw new Error(error.message);
}
