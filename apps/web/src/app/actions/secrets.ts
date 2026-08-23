"use server";

/**
 * Gestão de senhas de documentos protegidos.
 *
 * Existe porque, até aqui, NENHUM caminho de escrita de segredo existia: nem
 * UI, nem action, nem RPC. A tabela `user_secrets` só podia ser populada à mão
 * via SQL, e o pipeline de PDFs protegidos portanto nunca tinha o que ler.
 *
 * O valor em claro nunca volta para o cliente. A listagem devolve apenas
 * escopo e rótulo — quem precisa da senha é o worker, via `fn_get_secrets`,
 * que é concedida somente a service_role.
 */

import { createClient } from "@/lib/supabase/server";
import { revalidatePath } from "next/cache";

export interface SecretSummary {
  id: string;
  secretType: string;
  entityType: string | null;
  entityId: string | null;
  contractIdentifier: string | null;
  label: string | null;
  lastUsedAt: string | null;
  successCount: number;
  createdAt: string | null;
}

/** Lista os segredos do usuário SEM os valores. */
export async function listSecrets(): Promise<SecretSummary[]> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("user_secrets")
    .select(
      "id, secret_type, entity_type, entity_id, contract_identifier, label, last_used_at, success_count, created_at",
    )
    .eq("user_id", user.id)
    .order("last_used_at", { ascending: false, nullsFirst: false });

  if (error) throw new Error(error.message);

  return (data ?? []).map((s) => ({
    id: s.id,
    secretType: s.secret_type,
    entityType: s.entity_type,
    entityId: s.entity_id,
    contractIdentifier: s.contract_identifier,
    label: s.label,
    lastUsedAt: s.last_used_at,
    successCount: s.success_count ?? 0,
    createdAt: s.created_at,
  }));
}

export interface SetSecretInput {
  plaintext: string;
  label?: string | null;
  entityType?:
    "supplier" | "financial_product" | "card" | "supplier_contract" | "institution" | null;
  entityId?: string | null;
  contractIdentifier?: string | null;
  secretType?: string;
}

/**
 * Grava (ou substitui) a senha de um escopo.
 *
 * Delega a `fn_set_secret`, que criptografa dentro do banco e usa `auth.uid()`
 * diretamente — não há parâmetro de usuário para forjar.
 */
export async function setSecret(input: SetSecretInput): Promise<{ id: string }> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  if (!input.plaintext || input.plaintext.length === 0) {
    throw new Error("Informe a senha");
  }

  const { data, error } = await supabase.rpc("fn_set_secret", {
    p_secret_type: input.secretType ?? "pdf_password",
    p_plaintext: input.plaintext,
    p_entity_type: input.entityType ?? null,
    p_entity_id: input.entityId ?? null,
    p_contract_identifier: input.contractIdentifier ?? null,
    p_label: input.label ?? null,
  });

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/settings/passwords");
  return { id: data as unknown as string };
}

export async function deleteSecret(id: string): Promise<void> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { error } = await supabase
    .from("user_secrets")
    .delete()
    .eq("id", id)
    .eq("user_id", user.id);

  if (error) throw new Error(error.message);
  revalidatePath("/dashboard/settings/passwords");
}
