"use server";

import { createClient } from "@/lib/supabase/server";
import type { Supplier, SupplierAlias, SupplierContract, Insert, Update } from "@sbf/shared-types";

type SupplierInsert = Omit<Insert<"suppliers">, "user_id">;
type SupplierUpdate = Update<"suppliers">;
type AliasInsert = Omit<Insert<"supplier_aliases">, "user_id">;
type AliasUpdate = Update<"supplier_aliases">;
type ContractInsert = Omit<Insert<"supplier_contracts">, "user_id">;
type ContractUpdate = Update<"supplier_contracts">;

// ══════════════════════════════════════════════════════════════
// Suppliers
// ══════════════════════════════════════════════════════════════

export async function getSuppliers(activeOnly = true): Promise<Supplier[]> {
  const supabase = await createClient();
  let query = supabase.from("suppliers").select("*").order("name");

  if (activeOnly) {
    query = query.eq("is_active", true);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getSupplier(id: string): Promise<Supplier | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("suppliers").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function searchSuppliers(query: string, limit = 10) {
  const supabase = await createClient();
  const { data, error } = await supabase.rpc("search_suppliers", {
    p_query: query,
    p_limit: limit,
  });

  if (error) throw new Error(error.message);
  return data;
}

export async function createSupplier(input: SupplierInsert): Promise<Supplier> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("suppliers")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateSupplier(id: string, input: SupplierUpdate): Promise<Supplier> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("suppliers")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteSupplier(id: string): Promise<void> {
  const supabase = await createClient();
  // Soft delete — marca como inativo
  const { error } = await supabase.from("suppliers").update({ is_active: false }).eq("id", id);

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// Supplier Aliases
// ══════════════════════════════════════════════════════════════

export async function getSupplierAliases(supplierId: string): Promise<SupplierAlias[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("supplier_aliases")
    .select("*")
    .eq("supplier_id", supplierId)
    .order("created_at", { ascending: false });

  if (error) throw new Error(error.message);
  return data;
}

export async function createSupplierAlias(input: AliasInsert): Promise<SupplierAlias> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("supplier_aliases")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateSupplierAlias(id: string, input: AliasUpdate): Promise<SupplierAlias> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("supplier_aliases")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteSupplierAlias(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("supplier_aliases").delete().eq("id", id);

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// Supplier Contracts
// ══════════════════════════════════════════════════════════════

export async function getSupplierContracts(supplierId: string): Promise<SupplierContract[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("supplier_contracts")
    .select("*")
    .eq("supplier_id", supplierId)
    .order("start_date", { ascending: false });

  if (error) throw new Error(error.message);
  return data;
}

export async function createSupplierContract(input: ContractInsert): Promise<SupplierContract> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("supplier_contracts")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateSupplierContract(
  id: string,
  input: ContractUpdate,
): Promise<SupplierContract> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("supplier_contracts")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteSupplierContract(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("supplier_contracts").delete().eq("id", id);

  if (error) throw new Error(error.message);
}
