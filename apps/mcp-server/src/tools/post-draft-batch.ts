/**
 * @sbf/mcp-server — Tool: post_draft_batch
 *
 * Lança os drafts JÁ APROVADOS de um batch, criando os registros financeiros
 * definitivos. Separada de `approve_draft_batch` de propósito: aprovar e
 * lançar são atos distintos (§9 do plano mestre), e nem o agente de IA nem o
 * MCP devem conseguir transformar um documento em dinheiro numa chamada só.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import {
  POSTABLE_SCHEMAS,
  TARGET_TABLES,
  TO_INSERT,
  formatIssues,
  parseDraftPayload,
  type DraftType,
} from "@sbf/contracts";

export interface PostDraftResult {
  draftId: string;
  success: boolean;
  postedRecordId: string | null;
  postedRecordType: string | null;
  message: string;
  validationErrors: string[];
}

export interface PostBatchResult {
  batchId: string;
  totalProcessed: number;
  succeeded: number;
  failed: number;
  results: PostDraftResult[];
}

/** Status a partir dos quais lançar é permitido. `pending_review` NÃO está aqui. */
const POSTABLE_STATUSES = ["approved", "corrected"];

export async function postDraftBatch(
  supabase: SupabaseClient,
  userId: string,
  batchId: string,
): Promise<PostBatchResult> {
  const { data: batch, error: batchError } = await supabase
    .from("draft_batches")
    .select("id")
    .eq("id", batchId)
    .eq("user_id", userId)
    .single();

  if (batchError || !batch) {
    throw new Error(`Batch not found or not owned by user: ${batchId}`);
  }

  const { data: drafts, error } = await supabase
    .from("draft_records")
    .select("id, draft_type, status, draft_data, draft_schema_version, posted_record_id")
    .eq("batch_id", batchId)
    .eq("user_id", userId)
    .in("status", POSTABLE_STATUSES);

  if (error) throw new Error(`Failed to read drafts: ${error.message}`);

  const results: PostDraftResult[] = [];

  for (const draft of drafts ?? []) {
    const draftId = draft.id as string;

    if (draft.posted_record_id) {
      results.push({
        draftId,
        success: true,
        postedRecordId: draft.posted_record_id as string,
        postedRecordType: null,
        message: "Já materializado anteriormente",
        validationErrors: [],
      });
      continue;
    }

    const draftType = draft.draft_type as DraftType;
    const schema = POSTABLE_SCHEMAS[draftType];
    if (!schema) {
      results.push({
        draftId,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: `Tipo de draft desconhecido: ${draft.draft_type}`,
        validationErrors: [],
      });
      continue;
    }

    const parsed = parseDraftPayload({
      draft_type: draft.draft_type as string,
      draft_data: draft.draft_data,
      draft_schema_version: draft.draft_schema_version as number | null,
    });
    if (!parsed.ok) {
      results.push({
        draftId,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: "Draft com dados inconsistentes",
        validationErrors: parsed.errors,
      });
      continue;
    }

    const postable = schema.safeParse(parsed.payload);
    if (!postable.success) {
      results.push({
        draftId,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: "Faltam dados obrigatórios para lançar",
        validationErrors: formatIssues(postable.error),
      });
      continue;
    }

    const buildInsert = TO_INSERT[draftType] as (p: unknown) => Record<string, unknown>;

    const { data: rpcResult, error: rpcError } = await supabase.rpc("fn_materialize_draft_record", {
      p_user_id: userId,
      p_draft_id: draftId,
      p_target_table: TARGET_TABLES[draftType],
      p_insert_payload: buildInsert(postable.data),
      p_actor: "mcp",
    });

    if (rpcError) {
      results.push({
        draftId,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: rpcError.message,
        validationErrors: [],
      });
      continue;
    }

    const r = rpcResult as { status: string; posted_record_id: string; posted_record_type: string };
    results.push({
      draftId,
      success: true,
      postedRecordId: r.posted_record_id,
      postedRecordType: r.posted_record_type,
      message: r.status === "already_posted" ? "Já materializado anteriormente" : "Lançado",
      validationErrors: [],
    });
  }

  const succeeded = results.filter((r) => r.success).length;

  return {
    batchId,
    totalProcessed: results.length,
    succeeded,
    failed: results.length - succeeded,
    results,
  };
}
