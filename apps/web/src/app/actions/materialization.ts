"use server";

/**
 * Materialização — transforma drafts APROVADOS em registros financeiros reais.
 *
 * Regra fundamental: nenhum registro financeiro é criado antes da aprovação
 * humana. Aprovar e lançar são atos distintos (§9 do plano mestre), e é a RPC
 * `fn_materialize_draft_record` que faz valer a regra — não este arquivo.
 *
 * Divisão de responsabilidade:
 *   - aqui: autenticar, ler o draft, validar com os schemas de @sbf/contracts
 *     (os MESMOS que o gerador usa) e montar o payload de INSERT;
 *   - na RPC: travar, conferir invariantes, inserir, marcar e auditar, tudo
 *     em uma única transação.
 *
 * A versão anterior tinha 533 linhas, os schemas duplicados aqui (divergentes
 * dos do gerador, de modo que nenhum draft passava), inserts em dois
 * round-trips sem transação e uma checagem de idempotência TOCTOU.
 */

import { createClient } from "@/lib/supabase/server";
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
 * RPC. A checagem dupla é intencional — a daqui dá mensagem útil à UI, a da
 * RPC é a que nenhum cliente consegue contornar.
 */
export async function postApprovedDraftRecord(
  draftRecordId: string,
): Promise<MaterializationResult> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) return failure(draftRecordId, "Não autenticado");

  const { data: draft, error } = await supabase
    .from("draft_records")
    .select(
      "id, draft_type, status, draft_data, draft_schema_version, source_document_id, posted_record_id, posted_record_type",
    )
    .eq("id", draftRecordId)
    .eq("user_id", user.id)
    .single();

  if (error || !draft) {
    return failure(draftRecordId, `Draft não encontrado: ${error?.message ?? "inexistente"}`);
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
  // para a RPC como jsonb, então o tipo de saída é Json.
  const buildInsert = TO_INSERT[draftType] as unknown as (p: unknown) => Json;
  const insertPayload = buildInsert(postable.data);

  const { data: rpcResult, error: rpcError } = await supabase.rpc("fn_materialize_draft_record", {
    p_user_id: user.id,
    p_draft_id: draftRecordId,
    p_target_table: targetTable,
    p_insert_payload: insertPayload,
    p_actor: "web",
  });

  if (rpcError) {
    // Registro best-effort do motivo, para a tela de revisão poder explicar a
    // falha sem obrigar ninguém a abrir logs. O draft segue 'approved' e
    // retentável, porque a RPC reverteu tudo.
    await supabase
      .from("draft_records")
      .update({
        materialization_error: { message: rpcError.message, at: new Date().toISOString() },
      })
      .eq("id", draftRecordId)
      .eq("user_id", user.id);

    return failure(draftRecordId, `Falha ao lançar: ${rpcError.message}`);
  }

  const result = rpcResult as {
    status: string;
    posted_record_id: string;
    posted_record_type: string;
  };

  return {
    success: true,
    draftRecordId,
    postedRecordId: result.posted_record_id,
    postedRecordType: result.posted_record_type,
    validationErrors: [],
    message:
      result.status === "already_posted"
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

  if (!user) {
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [failure("", "Não autenticado")],
    };
  }

  const { data: drafts, error } = await supabase
    .from("draft_records")
    .select("id")
    .eq("batch_id", batchId)
    .eq("user_id", user.id)
    .in("status", POSTABLE_STATUSES);

  if (error) {
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [failure("", `Erro ao buscar drafts: ${error.message}`)],
    };
  }

  if (!drafts || drafts.length === 0) {
    return { batchId, totalProcessed: 0, succeeded: 0, failed: 0, results: [] };
  }

  const results: MaterializationResult[] = [];
  for (const draft of drafts) {
    results.push(await postApprovedDraftRecord(draft.id));
  }

  const succeeded = results.filter((r) => r.success).length;
  const failed = results.length - succeeded;

  const batchStatus = failed === 0 ? "approved" : succeeded === 0 ? "rejected" : "partial";
  await supabase
    .from("draft_batches")
    .update({
      status: batchStatus,
      approved_count: succeeded,
      rejected_count: failed,
      updated_at: new Date().toISOString(),
    })
    .eq("id", batchId)
    .eq("user_id", user.id);

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
