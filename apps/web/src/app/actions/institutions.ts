"use server";

import { createClient } from "@/lib/supabase/server";
import type { Institution, Insert, Update } from "@sbf/shared-types";

type InstitutionInsert = Omit<Insert<"institutions">, "user_id">;
type InstitutionUpdate = Update<"institutions">;

export async function getInstitutions(): Promise<Institution[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("institutions")
    .select("*")
    .order("display_order", { ascending: true, nullsFirst: false })
    .order("name");

  if (error) throw new Error(error.message);
  return data;
}

export async function getInstitution(id: string): Promise<Institution | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("institutions").select("*").eq("id", id).single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createInstitution(input: InstitutionInsert): Promise<Institution> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("institutions")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function updateInstitution(
  id: string,
  input: InstitutionUpdate,
): Promise<Institution> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("institutions")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function deleteInstitution(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("institutions").delete().eq("id", id);

  if (error) throw new Error(error.message);
}
