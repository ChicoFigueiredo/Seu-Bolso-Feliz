"use server";

/**
 * Gestão de senhas de documentos protegidos.
 *
 * O valor em claro nunca volta para o cliente. A listagem devolve apenas
 * escopo e rótulo — quem precisa da senha é o worker, via `fn_get_secrets`
 * (Fase 4b, fora deste arquivo), concedida somente a service_role.
 *
 * ATENÇÃO: `setSecret` abaixo chama `fn_set_secret`, que NÃO existe hoje em
 * packages/db/schema.sql — uma das seis funções descartadas por inteiro na
 * migração Supabase → Neon (NOTA no topo daquele arquivo), por depender de
 * auth.uid() sem receber user_id como parâmetro. Mesmo recriada com
 * p_user_id explícito, a criptografia (encrypt_secret/decrypt_secret) depende
 * de private.get_crypto_key(), também não portado. Ver Apêndice "RPCs
 * bloqueadas" do plano de implementação desta fase — setSecret permanece
 * intocada (ainda supabase.rpc()) até essa migração separada acontecer.
 */

import { requireUser } from "@/lib/auth";
import { createClient } from "@/lib/supabase/server";
import { revalidatePath } from "next/cache";
import { getDb, schema } from "@sbf/db";
import { and, eq, sql } from "drizzle-orm";

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

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

/** Lista os segredos do usuário SEM os valores. */
export async function listSecrets(): Promise<SecretSummary[]> {
  const userId = await requireUserId();
  const db = getDb();

  // asc()/desc() do drizzle-orm não encadeiam .nullsLast() em orderBy() nesta
  // versão — fragmento sql`` cru replica "NULLS LAST" de hoje.
  const rows = await db
    .select({
      id: schema.user_secrets.id,
      secretType: schema.user_secrets.secret_type,
      entityType: schema.user_secrets.entity_type,
      entityId: schema.user_secrets.entity_id,
      contractIdentifier: schema.user_secrets.contract_identifier,
      label: schema.user_secrets.label,
      lastUsedAt: schema.user_secrets.last_used_at,
      successCount: schema.user_secrets.success_count,
      createdAt: schema.user_secrets.created_at,
    })
    .from(schema.user_secrets)
    .where(eq(schema.user_secrets.user_id, userId))
    .orderBy(sql`${schema.user_secrets.last_used_at} desc nulls last`);

  return rows.map((s) => ({ ...s, successCount: s.successCount ?? 0 }));
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
 * BLOQUEADA — NÃO CONVERTIDA. Ver comentário de cabeçalho do arquivo e o
 * Apêndice "RPCs bloqueadas" do plano. Permanece com o código Supabase atual.
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
    p_entity_type: input.entityType ?? undefined,
    p_entity_id: input.entityId ?? undefined,
    p_contract_identifier: input.contractIdentifier ?? undefined,
    p_label: input.label ?? undefined,
  });

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/settings/passwords");
  return { id: data as unknown as string };
}

export async function deleteSecret(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.user_secrets)
    .where(and(eq(schema.user_secrets.id, id), eq(schema.user_secrets.user_id, userId)))
    .returning({ id: schema.user_secrets.id });

  if (result.length === 0) throw new Error("Senha não encontrada");

  revalidatePath("/dashboard/settings/passwords");
}
