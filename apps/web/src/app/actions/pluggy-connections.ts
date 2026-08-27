"use server";

import { createClient } from "@/lib/supabase/server";
import { PluggyProvider } from "@sbf/financial-connectors/pluggy";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

export type ProviderConnection = typeof schema.provider_connections.$inferSelect;
export type ExternalAccountMapping = typeof schema.external_account_mappings.$inferSelect;
type ExternalAccountMappingUpdate = Partial<
  Omit<typeof schema.external_account_mappings.$inferInsert, "user_id" | "id">
>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getProviderConnections(): Promise<ProviderConnection[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.provider_connections)
    .where(eq(schema.provider_connections.user_id, userId))
    .orderBy(desc(schema.provider_connections.created_at));
}

export async function getExternalAccountMappings(
  providerConnectionId: string,
): Promise<ExternalAccountMapping[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.external_account_mappings)
    .where(
      and(
        eq(schema.external_account_mappings.provider_connection_id, providerConnectionId),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    )
    .orderBy(asc(schema.external_account_mappings.created_at));
}

/** Token de vida curta (30min) pro widget Pluggy Connect no navegador. */
export async function createPluggyConnectToken(options?: {
  itemId?: string;
}): Promise<{ connectToken: string }> {
  const userId = await requireUserId();
  const provider = PluggyProvider.fromEnv();
  const { connectToken } = await provider.createConnectToken({
    itemId: options?.itemId,
    clientUserId: userId,
  });
  return { connectToken };
}

/**
 * Chamada pelo frontend depois que o widget Pluggy Connect termina e devolve
 * um `itemId`. Persiste a conexão e cria (sem sobrescrever mapeamentos já
 * feitos pelo usuário) um `external_account_mappings` "pending_mapping" pra
 * cada conta que a Pluggy retornar.
 */
export async function completePluggyConnection(
  externalItemId: string,
): Promise<ProviderConnection> {
  const userId = await requireUserId();
  const db = getDb();

  const provider = PluggyProvider.fromEnv();
  const connection = await provider.getConnection(externalItemId);
  const accounts = await provider.listAccounts(externalItemId);

  const [row] = await db
    .insert(schema.provider_connections)
    .values({
      user_id: userId,
      provider: "pluggy",
      external_item_id: externalItemId,
      institution_name: connection.institutionName ?? null,
      status: "active",
    })
    .onConflictDoUpdate({
      target: [
        schema.provider_connections.user_id,
        schema.provider_connections.provider,
        schema.provider_connections.external_item_id,
      ],
      set: { institution_name: connection.institutionName ?? null, status: "active" },
    })
    .returning();
  if (!row) throw new Error("Falha ao salvar conexão do provedor");

  for (const account of accounts) {
    await db
      .insert(schema.external_account_mappings)
      .values({
        user_id: userId,
        provider_connection_id: row.id,
        external_account_id: account.externalAccountId,
        external_account_name: account.name,
        external_account_type: account.type,
      })
      .onConflictDoNothing({
        target: [
          schema.external_account_mappings.provider_connection_id,
          schema.external_account_mappings.external_account_id,
        ],
      });
  }

  return row;
}

export async function updateExternalAccountMapping(
  id: string,
  input: ExternalAccountMappingUpdate,
): Promise<ExternalAccountMapping> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.external_account_mappings)
    .set(input)
    .where(
      and(
        eq(schema.external_account_mappings.id, id),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    )
    .returning();
  if (!row) throw new Error("Vínculo de conta externa não encontrado");
  return row;
}

export async function revokeProviderConnection(id: string): Promise<void> {
  // Divergência deliberada do brief original (que preservava o silêncio do
  // supabase-js em 0 linhas afetadas): por consistência com a decisão já
  // ratificada em institutions.ts (Task 2, deleteInstitution) — lançar erro é
  // mais seguro que falhar em silêncio numa ação que não pertence ao usuário,
  // e hoje não há call site que dependa do silêncio antigo (a UI de conexões
  // ainda chama Supabase direto, fora do escopo desta fase).
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .update(schema.provider_connections)
    .set({ status: "revoked" })
    .where(
      and(eq(schema.provider_connections.id, id), eq(schema.provider_connections.user_id, userId)),
    )
    .returning({ id: schema.provider_connections.id });
  if (result.length === 0) throw new Error("Conexão não encontrada");
}
