/**
 * Gravação de segredos (senhas de PDF de fatura), compartilhada entre a
 * aplicação web e o worker de ingestão.
 *
 * Isto vive aqui, e não numa Server Action, porque **os dois lados precisam
 * gravar**: a web quando você cadastra a senha à mão, e o worker quando
 * descobre que uma senha abriu o PDF de um fornecedor e quer que o próximo
 * documento do mesmo fornecedor acerte de primeira.
 *
 * Antes existia uma função `public.fn_set_secret` no Postgres fazendo isso. Ela
 * foi descartada na migração para o Neon (uma das seis da NOTA de `schema.sql`)
 * e o call site do worker continuou chamando — com o erro engolido num
 * `catch {}`, de modo que a associação simplesmente nunca acontecia e ninguém
 * percebia. Reimplementada em SQL explícito aqui, um só lugar para os dois.
 *
 * ⚠️ Esta função **não autoriza nada**. Quem chama é responsável por provar que
 * `entityId` pertence a `userId` antes — não há mais RLS para segurar isso.
 */
import { sql } from "drizzle-orm";
import { getDb } from "./client";
import * as schema from "./schema";

export interface GravarSegredoInput {
  userId: string;
  /** O segredo em claro. Nunca é registrado em log nem em auditoria. */
  plaintext: string;
  secretType?: string;
  entityType?: string | null;
  entityId?: string | null;
  contractIdentifier?: string | null;
  label?: string | null;
  /** Quem provocou a gravação, para a trilha de auditoria. */
  origem?: string;
}

export async function gravarSegredo(input: GravarSegredoInput): Promise<{ id: string }> {
  if (!input.plaintext) throw new Error("segredo vazio");

  const db = getDb();
  const secretType = input.secretType ?? "pdf_password";

  // O índice único é por expressão (COALESCE sobre três colunas anuláveis), o
  // que o `onConflictDoUpdate` do query builder não sabe representar — daí o
  // SQL cru. `encrypt_secret` roda dentro do banco: a chave nunca sai de lá.
  const gravado = await db.execute<{ id: string }>(sql`
    INSERT INTO public.user_secrets (
      user_id, secret_type, entity_type, entity_id, contract_identifier,
      label, encrypted_value, encryption_version
    ) VALUES (
      ${input.userId}::uuid,
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

  const id = gravado.rows[0]?.id;
  if (!id) throw new Error("falha ao gravar o segredo");

  // Auditoria sem o valor — o segredo não entra em log, nem aqui nem em
  // lugar nenhum.
  await db.insert(schema.audit_logs).values({
    user_id: input.userId,
    action: "secret_written",
    entity_type: "user_secrets",
    entity_id: id,
    new_values: {
      secret_type: secretType,
      entity_type: input.entityType ?? null,
      origem: input.origem ?? "web",
    },
  });

  return { id };
}
