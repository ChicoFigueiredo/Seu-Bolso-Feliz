"use server";

import { createClient } from "@/lib/supabase/server";
import { PluggyProvider } from "@sbf/financial-connectors/pluggy";
import type { Row, Update } from "@sbf/shared-types";

export type ProviderConnection = Row<"provider_connections">;
export type ExternalAccountMapping = Row<"external_account_mappings">;
type ExternalAccountMappingUpdate = Update<"external_account_mappings">;

export async function getProviderConnections(): Promise<ProviderConnection[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("provider_connections")
    .select("*")
    .order("created_at", { ascending: false });

  if (error) throw new Error(error.message);
  return data;
}

export async function getExternalAccountMappings(
  providerConnectionId: string,
): Promise<ExternalAccountMapping[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("external_account_mappings")
    .select("*")
    .eq("provider_connection_id", providerConnectionId)
    .order("created_at", { ascending: true });

  if (error) throw new Error(error.message);
  return data;
}

/** Token de vida curta (30min) pro widget Pluggy Connect no navegador. */
export async function createPluggyConnectToken(options?: {
  itemId?: string;
}): Promise<{ connectToken: string }> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const provider = PluggyProvider.fromEnv();
  const { connectToken } = await provider.createConnectToken({
    itemId: options?.itemId,
    clientUserId: user.id,
  });
  return { connectToken };
}

/**
 * Chamada pelo frontend depois que o widget Pluggy Connect termina e devolve
 * um `itemId`. Persiste a conexão e cria (sem sobrescrever mapeamentos já
 * feitos pelo usuário — `ignoreDuplicates`) um `external_account_mappings`
 * "pending_mapping" pra cada conta que a Pluggy retornar. O usuário vincula
 * cada uma a um `financial_products` depois — nunca automático.
 */
export async function completePluggyConnection(
  externalItemId: string,
): Promise<ProviderConnection> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const provider = PluggyProvider.fromEnv();
  const connection = await provider.getConnection(externalItemId);
  const accounts = await provider.listAccounts(externalItemId);

  const { data: row, error } = await supabase
    .from("provider_connections")
    .upsert(
      {
        user_id: user.id,
        provider: "pluggy",
        external_item_id: externalItemId,
        institution_name: connection.institutionName ?? null,
        status: "active",
      },
      { onConflict: "user_id,provider,external_item_id" },
    )
    .select()
    .single();

  if (error) throw new Error(error.message);

  for (const account of accounts) {
    const { error: mappingError } = await supabase.from("external_account_mappings").upsert(
      {
        user_id: user.id,
        provider_connection_id: row.id,
        external_account_id: account.externalAccountId,
        external_account_name: account.name,
        external_account_type: account.type,
      },
      { onConflict: "provider_connection_id,external_account_id", ignoreDuplicates: true },
    );
    if (mappingError) throw new Error(mappingError.message);
  }

  return row;
}

export async function updateExternalAccountMapping(
  id: string,
  input: ExternalAccountMappingUpdate,
): Promise<ExternalAccountMapping> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("external_account_mappings")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

export async function revokeProviderConnection(id: string): Promise<void> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { error } = await supabase
    .from("provider_connections")
    .update({ status: "revoked" })
    .eq("id", id);

  if (error) throw new Error(error.message);
}
