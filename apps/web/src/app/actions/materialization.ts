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
 * `p_user_id` é sempre o `ownerId` da sessão autenticada (nunca um valor
 * vindo do cliente) — a função é `SECURITY DEFINER` e materializa dados
 * financeiros de verdade.
 *
 * ⚠️ Dívida que este arquivo paga: antes, `fn_materialize_draft_record`
 * amarrava `p_user_id` ao JWT (`auth.uid() <> p_user_id`). No Neon essa
 * checagem não existe mais (NOTA nº 3 de `packages/db/schema.sql`), então o
 * `p_user_id` que chega na função é confiável PORQUE aqui ele vem da sessão
 * e o draft é lido antes com `user_id = ownerId` — a autorização é deste
 * call site, não da função.
 */

import { requireUser } from "@/lib/auth";
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
function ehTipoDeDraft(valor: string): valor is DraftType {
  return valor in POSTABLE_SCHEMAS;
}

/**
 * Valida o payload e monta a linha a ser inserida, num passo só.
 *
 * O `switch` com acesso literal (`TO_INSERT.transaction`) não é verbosidade:
 * `POSTABLE_SCHEMAS[t]` e `TO_INSERT[t]` indexados pela mesma variável
 * produzem duas uniões que o TypeScript não correlaciona entre si, e a única
 * saída era um cast duplo — que apagava justamente a checagem de que o schema
 * e o construtor são do mesmo tipo de draft. Aqui os dois lados estreitam
 * juntos, sem nenhum cast no caminho.
 *
 * O retorno é a união dos `*Insert`, não `Record<string, unknown>`: eles são
 * `interface` e não têm index signature. O payload vira jsonb logo em seguida,
 * que aceita a união sem conversão.
 */
function construirPayloadDeInsert(
  draftType: DraftType,
  payload: unknown,
):
  { ok: true; value: ReturnType<(typeof TO_INSERT)[DraftType]> } | { ok: false; errors: string[] } {
  switch (draftType) {
    case "transaction": {
      const r = POSTABLE_SCHEMAS.transaction.safeParse(payload);
      return r.success
        ? { ok: true, value: TO_INSERT.transaction(r.data) }
        : { ok: false, errors: formatIssues(r.error) };
    }
    case "consumption_metric": {
      const r = POSTABLE_SCHEMAS.consumption_metric.safeParse(payload);
      return r.success
        ? { ok: true, value: TO_INSERT.consumption_metric(r.data) }
        : { ok: false, errors: formatIssues(r.error) };
    }
    case "liability": {
      const r = POSTABLE_SCHEMAS.liability.safeParse(payload);
      return r.success
        ? { ok: true, value: TO_INSERT.liability(r.data) }
        : { ok: false, errors: formatIssues(r.error) };
    }
    case "recurring_template": {
      const r = POSTABLE_SCHEMAS.recurring_template.safeParse(payload);
      return r.success
        ? { ok: true, value: TO_INSERT.recurring_template(r.data) }
        : { ok: false, errors: formatIssues(r.error) };
    }
  }
}

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
 * As recusas de NEGÓCIO (draft inexistente, ainda pendente, payload
 * inválido) continuam sendo objeto de falha (`success: false`), porque a UI
 * depende desse contrato. Falta de sessão não é recusa de negócio: sem
 * cookie válido, `requireUser()` manda para o /login como no resto do app.
 */
export async function postApprovedDraftRecord(
  draftRecordId: string,
): Promise<MaterializationResult> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo
  // ator. Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();

  const db = getDb();

  const [draft] = await db
    .select()
    .from(schema.draft_records)
    .where(
      and(eq(schema.draft_records.id, draftRecordId), eq(schema.draft_records.user_id, ownerId)),
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

  if (!ehTipoDeDraft(draft.draft_type)) {
    return failure(draftRecordId, `Tipo de draft desconhecido: ${draft.draft_type}`);
  }
  const draftType = draft.draft_type;

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

  const construido = construirPayloadDeInsert(draftType, parsed.payload);
  if (!construido.ok) {
    return failure(draftRecordId, "Faltam dados obrigatórios para lançar", construido.errors);
  }

  const targetTable = TARGET_TABLES[draftType];
  const insertPayload = construido.value;

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
        ${ownerId}::uuid,
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
        and(eq(schema.draft_records.id, draftRecordId), eq(schema.draft_records.user_id, ownerId)),
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
  const { ownerId } = await requireUser();

  const db = getDb();

  let drafts: { id: string }[];
  try {
    drafts = await db
      .select({ id: schema.draft_records.id })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.batch_id, batchId),
          eq(schema.draft_records.user_id, ownerId),
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
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, ownerId)));

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
