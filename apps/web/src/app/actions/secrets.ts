"use server";

/**
 * Gestão de senhas de documentos protegidos.
 *
 * O valor em claro nunca volta para o cliente. A listagem devolve apenas
 * escopo e rótulo — quem precisa da senha é o worker, via `fn_get_secrets`
 * (Fase 4b, fora deste arquivo), concedida somente a service_role.
 *
 * `fn_set_secret` era uma das SEIS funções descartadas por inteiro na
 * migração Supabase → Neon (packages/db/schema.sql, NOTA item 4): sabia quem
 * era o usuário só via `auth.uid()`. Confirmado por consulta ao `pg_proc` do
 * Neon: NÃO existe lá. `setSecret` abaixo reimplementa o corpo dela.
 *
 * A criptografia, que era o segundo bloqueio, deixou de ser: o schema
 * `private` (crypto_keys + get_crypto_key) foi portado para o Neon, e o
 * round-trip `decrypt_secret(encrypt_secret(x)) = x` foi verificado no banco
 * real antes desta reescrita. O texto em claro continua sem passar pelo
 * TypeScript: ele vai como parâmetro de bind direto para `encrypt_secret()`
 * dentro do INSERT, e o que a aplicação enxerga de volta é só o `id`.
 */

import { requireUser } from "@/lib/auth";
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

/** Mesma lista do CHECK `chk_user_secrets_entity_type` no banco. */
const TIPOS_DE_ENTIDADE = [
  "supplier",
  "financial_product",
  "card",
  "supplier_contract",
  "institution",
] as const;

export type SecretEntityType = (typeof TIPOS_DE_ENTIDADE)[number];

export interface SetSecretInput {
  plaintext: string;
  label?: string | null;
  entityType?: SecretEntityType | null;
  entityId?: string | null;
  contractIdentifier?: string | null;
  secretType?: string;
}

/**
 * `user_secrets.entity_id` é polimórfico: não tem (nem pode ter) FK, e o
 * comentário da migration que o criou prometia um trigger de validação que
 * nunca chegou ao banco — confirmado por consulta a `pg_trigger` no Neon,
 * onde `user_secrets` só tem o trigger de `updated_at`. Sem RLS e sem FK, a
 * única checagem de posse que existe é esta.
 */
const TABELA_POR_ENTIDADE = {
  supplier: schema.suppliers,
  financial_product: schema.financial_products,
  card: schema.cards,
  supplier_contract: schema.supplier_contracts,
  institution: schema.institutions,
} as const;

/**
 * `entityType` chega de um cliente: o tipo do parâmetro é só uma promessa de
 * compilação. Sem esta checagem, indexar o objeto acima com `"constructor"`
 * ou `"toString"` devolveria algo truthy — uma chave que não é entidade
 * nenhuma passaria pela validação de posse.
 */
function ehTipoDeEntidade(valor: string): valor is SecretEntityType {
  return TIPOS_DE_ENTIDADE.includes(valor as SecretEntityType);
}

async function assertOwnsEntity(
  userId: string,
  entityType: SecretEntityType | null | undefined,
  entityId: string | null | undefined,
): Promise<void> {
  if (!entityId) return;
  // Um id sem tipo não diz em qual tabela procurar — não dá para validar
  // posse, e gravar assim mesmo seria exatamente o IDOR que esta função
  // existe para impedir.
  if (!entityType) throw new Error("Informe o tipo da entidade junto com o id");
  if (!ehTipoDeEntidade(entityType)) {
    throw new Error(`Tipo de entidade desconhecido: ${entityType}`);
  }

  const tabela = TABELA_POR_ENTIDADE[entityType];

  const db = getDb();
  const [row] = await db
    .select({ id: tabela.id })
    .from(tabela)
    .where(and(eq(tabela.id, entityId), eq(tabela.user_id, userId)));
  if (!row) throw new Error("Entidade não encontrada");
}

/**
 * Grava (ou substitui) a senha de um escopo. Reimplementa `fn_set_secret`;
 * ver o cabeçalho do arquivo.
 *
 * O `ON CONFLICT` reproduz literalmente o índice único por expressão
 * `uq_user_secrets_scope` — um segredo por (dono, tipo, entidade, contrato).
 * Isso não é representável pelo `onConflictDoUpdate` do Drizzle (que só
 * aceita colunas como alvo), então o statement vai como `sql` parametrizado.
 */
export async function setSecret(input: SetSecretInput): Promise<{ id: string }> {
  const userId = await requireUserId();

  if (!input.plaintext || input.plaintext.length === 0) {
    throw new Error("Informe a senha");
  }

  await assertOwnsEntity(userId, input.entityType, input.entityId);

  const db = getDb();
  const secretType = input.secretType ?? "pdf_password";

  const inserido = await db.execute<{ id: string }>(sql`
    INSERT INTO public.user_secrets (
      user_id, secret_type, entity_type, entity_id, contract_identifier,
      label, encrypted_value, encryption_version
    ) VALUES (
      ${userId}::uuid,
      ${secretType}::text,
      ${input.entityType ?? null}::text,
      ${input.entityId ?? null}::uuid,
      ${input.contractIdentifier ?? null}::text,
      ${input.label ?? null}::text,
      public.encrypt_secret(${input.plaintext}::text),
      1
    )
    ON CONFLICT (user_id, secret_type, COALESCE(entity_type, '-'::text),
                 COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uuid),
                 COALESCE(contract_identifier, '-'::text))
    DO UPDATE SET encrypted_value    = EXCLUDED.encrypted_value,
                  encryption_version = EXCLUDED.encryption_version,
                  label              = COALESCE(EXCLUDED.label, user_secrets.label),
                  updated_at         = now()
    RETURNING id
  `);

  const id = inserido.rows[0]?.id;
  if (!id) throw new Error("Falha ao gravar a senha");

  // Auditoria sem o valor — nunca registrar o segredo em lugar algum.
  await db.insert(schema.audit_logs).values({
    user_id: userId,
    action: "secret_written",
    entity_type: "user_secrets",
    entity_id: id,
    new_values: { secret_type: secretType, entity_type: input.entityType ?? null },
  });

  revalidatePath("/dashboard/settings/passwords");
  return { id };
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
