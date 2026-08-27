"use server";

/**
 * Materialização — transforma drafts APROVADOS em registros financeiros reais.
 *
 * Regra fundamental: nenhum registro financeiro é criado antes da aprovação
 * humana. Aprovar e lançar são atos distintos (§9 do plano mestre), e é a
 * função `fn_materialize_draft_record` que faz valer a regra — não este
 * arquivo.
 *
 * Divisão de responsabilidade:
 *   - aqui: autenticar, ler o draft, validar com os schemas de @sbf/contracts
 *     (os MESMOS que o gerador usa) e montar o payload de INSERT;
 *   - na função SQL: travar, conferir invariantes, inserir, marcar e
 *     auditar, tudo em uma única transação (`SECURITY DEFINER`,
 *     `packages/db/schema.sql`).
 *
 * `p_user_id` é sempre o `user.id` da sessão autenticada (nunca um valor
 * vindo do cliente) — a função é `SECURITY DEFINER` e materializa dados
 * financeiros de verdade.
 */

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, eq, inArray, sql } from "drizzle-orm";
import {
  POSTABLE_SCHEMAS,
  TARGET_TABLES,
  TO_INSERT,
  formatIssues,
  parseDraftPayload,
  type DraftType,
} from "@sbf/contracts";
import type { Json } from "@sbf/shared-types";

// ── Tipos de resultado (superfície pública inalterada para a UI) ────────────

export interface MaterializationResult {
  success: boolean;
  draftRecordId: string;
  postedRecordId: string | null;
  postedRecordType: string | null;
  validationErrors: string[];
  message: string;
}

export interface MaterializationBatchResult {
  batchId: string;
  totalProcessed: number;
  succeeded: number;
  failed: number;
  results: MaterializationResult[];
}

/** Status a partir dos quais lançar é permitido. `pending_review` NÃO está aqui. */
const POSTABLE_STATUSES = ["approved", "corrected"] as const;

function failure(
  draftRecordId: string,
  message: string,
  validationErrors: string[] = [],
): MaterializationResult {
  return {
    success: false,
    draftRecordId,
    postedRecordId: null,
    postedRecordType: null,
    validationErrors,
    message,
  };
}

/**
 * Lança um draft aprovado, criando o registro financeiro definitivo.
 *
 * Não aprova nada: um draft ainda em `pending_review` é recusado, aqui e na
 * função SQL. A checagem dupla é intencional — a daqui dá mensagem útil à
 * UI, a de lá é a que nenhum cliente consegue contornar.
 *
 * "Sem usuário" e demais recusas são reportadas como objeto de falha
 * (`success: false`), não exceção — a UI depende desse contrato, por isso
 * não se usa aqui o padrão `requireUserId()` que lança.
 */
export async function postApprovedDraftRecord(
  draftRecordId: string,
): Promise<MaterializationResult> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  // `!user?.id` (não só `!user`): um usuário com id vazio não é um usuário
  // válido, e comparar user_id (uuid) contra "" faria a query abaixo
  // estourar em erro de sintaxe em vez de recusar de forma limpa.
  if (!user?.id) return failure(draftRecordId, "Não autenticado");

  const db = getDb();

  const [draft] = await db
    .select()
    .from(schema.draft_records)
    .where(
      and(eq(schema.draft_records.id, draftRecordId), eq(schema.draft_records.user_id, user.id)),
    );

  if (!draft) {
    return failure(draftRecordId, "Draft não encontrado: inexistente");
  }

  if (draft.posted_record_id) {
    return {
      success: true,
      draftRecordId,
      postedRecordId: draft.posted_record_id,
      postedRecordType: draft.posted_record_type,
      validationErrors: [],
      message: "Draft já foi materializado anteriormente",
    };
  }

  if (!POSTABLE_STATUSES.includes(draft.status as (typeof POSTABLE_STATUSES)[number])) {
    return failure(
      draftRecordId,
      `Draft em status "${draft.status}" não pode ser lançado. Aprove primeiro.`,
    );
  }

  const draftType = draft.draft_type as DraftType;
  const postableSchema = POSTABLE_SCHEMAS[draftType];
  if (!postableSchema) {
    return failure(draftRecordId, `Tipo de draft desconhecido: ${draft.draft_type}`);
  }

  // Tolera payloads v0 convertendo-os em memória, para que um backfill
  // incompleto não bloqueie o lançamento.
  const parsed = parseDraftPayload({
    draft_type: draft.draft_type,
    draft_data: draft.draft_data,
    draft_schema_version: draft.draft_schema_version,
  });
  if (!parsed.ok) {
    return failure(draftRecordId, "Draft com dados inconsistentes", parsed.errors);
  }

  const postable = postableSchema.safeParse(parsed.payload);
  if (!postable.success) {
    return failure(
      draftRecordId,
      "Faltam dados obrigatórios para lançar",
      formatIssues(postable.error),
    );
  }

  const targetTable = TARGET_TABLES[draftType];
  // TO_INSERT é uma união de funções com parâmetros distintos por tipo de
  // draft; o despacho por `draftType` já garante o par correto, mas o
  // TypeScript não consegue estreitar os dois lados juntos. O payload segue
  // para a função SQL como jsonb, então o tipo de saída é Json.
  const buildInsert = TO_INSERT[draftType] as unknown as (p: unknown) => Json;
  const insertPayload = buildInsert(postable.data);

  let rpcResult:
    { status: string; posted_record_id: string; posted_record_type: string } | undefined;
  try {
    // `fn_materialize_draft_record` RETURNS jsonb (um escalar, não uma
    // tabela): "SELECT * FROM fn(...)" produziria uma coluna com o nome da
    // função contendo o jsonb inteiro como valor único, não as três colunas
    // esperadas. "SELECT fn(...) AS result" é a forma correta — confirmado
    // empiricamente contra o Postgres local (driver `pg`, hostname
    // localhost): a coluna `result` chega já desserializada em objeto JS.
    const result = await db.execute<{
      result: { status: string; posted_record_id: string; posted_record_type: string };
    }>(
      sql`SELECT fn_materialize_draft_record(
        ${user.id}::uuid,
        ${draftRecordId}::uuid,
        ${targetTable}::text,
        ${JSON.stringify(insertPayload)}::jsonb,
        ${"web"}::text
      ) AS result`,
    );
    rpcResult = result.rows[0]?.result;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);

    await db
      .update(schema.draft_records)
      .set({ materialization_error: { message, at: new Date().toISOString() } })
      .where(
        and(eq(schema.draft_records.id, draftRecordId), eq(schema.draft_records.user_id, user.id)),
      );

    return failure(draftRecordId, `Falha ao lançar: ${message}`);
  }

  if (!rpcResult) {
    return failure(draftRecordId, "Falha ao lançar: função não retornou resultado");
  }

  return {
    success: true,
    draftRecordId,
    postedRecordId: rpcResult.posted_record_id,
    postedRecordType: rpcResult.posted_record_type,
    validationErrors: [],
    message:
      rpcResult.status === "already_posted"
        ? "Draft já foi materializado anteriormente"
        : "Lançamento criado",
  };
}

/**
 * Lança todos os drafts aprovados de um batch.
 * Item a item — falha individual não cancela os demais.
 */
export async function postApprovedDraftBatch(batchId: string): Promise<MaterializationBatchResult> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user?.id) {
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [failure("", "Não autenticado")],
    };
  }

  const db = getDb();

  let drafts: { id: string }[];
  try {
    drafts = await db
      .select({ id: schema.draft_records.id })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.batch_id, batchId),
          eq(schema.draft_records.user_id, user.id),
          inArray(schema.draft_records.status, POSTABLE_STATUSES),
        ),
      );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [failure("", `Erro ao buscar drafts: ${message}`)],
    };
  }

  if (drafts.length === 0) {
    return { batchId, totalProcessed: 0, succeeded: 0, failed: 0, results: [] };
  }

  const results: MaterializationResult[] = [];
  for (const draft of drafts) {
    results.push(await postApprovedDraftRecord(draft.id));
  }

  const succeeded = results.filter((r) => r.success).length;
  const failed = results.length - succeeded;

  const batchStatus = failed === 0 ? "approved" : succeeded === 0 ? "rejected" : "partial";
  await db
    .update(schema.draft_batches)
    .set({
      status: batchStatus,
      approved_count: succeeded,
      rejected_count: failed,
      updated_at: new Date().toISOString(),
    })
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, user.id)));

  return { batchId, totalProcessed: drafts.length, succeeded, failed, results };
}

// ── Conveniência: aprovar e lançar em sequência ────────────────────────────

export interface ApproveAndPostResult {
  approved: boolean;
  approvalError: string | null;
  materialization: MaterializationResult | null;
}

/**
 * Aprova e, em seguida, lança.
 *
 * São literalmente duas chamadas, e o retorno reporta as duas fases: o draft
 * passa mesmo por `approved` no banco, e a UI consegue mostrar um resultado
 * parcial honesto ("aprovado, mas não lançado: falta a conta") em vez de
 * apresentar a aprovação como fracassada.
 */
export async function approveAndPostDraftRecord(
  draftRecordId: string,
): Promise<ApproveAndPostResult> {
  const { approveDraftRecord } = await import("./ingestion");

  try {
    await approveDraftRecord(draftRecordId);
  } catch (err) {
    return {
      approved: false,
      approvalError: err instanceof Error ? err.message : String(err),
      materialization: null,
    };
  }

  return {
    approved: true,
    approvalError: null,
    materialization: await postApprovedDraftRecord(draftRecordId),
  };
}
