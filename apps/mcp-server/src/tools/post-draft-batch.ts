/**
 * @sbf/mcp-server — Tool: post_draft_batch
 *
 * Lança os drafts JÁ APROVADOS de um batch, criando os registros financeiros
 * definitivos. Separada de `approve_draft_batch` de propósito: aprovar e
 * lançar são atos distintos (§9 do plano mestre), e nem o agente de IA nem o
 * MCP devem conseguir transformar um documento em dinheiro numa chamada só.
 *
 * `fn_materialize_draft_record` continua existindo no Neon (SECURITY
 * DEFINER), mas perdeu a amarração a `auth.uid()` que o RLS/JWT do Supabase
 * garantia. Por isso o draft é lido e confirmado como dono de `userId`
 * *antes* de chamar a função — a autorização é deste call site, não da
 * função (mesmo desenho de apps/web/src/app/actions/materialization.ts).
 */
import { and, eq, inArray, sql } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
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
  db: Database,
  userId: string,
  batchId: string,
): Promise<PostBatchResult> {
  const [batch] = await db
    .select({ id: schema.draft_batches.id })
    .from(schema.draft_batches)
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, userId)));

  if (!batch) {
    throw new Error(`Batch not found or not owned by user: ${batchId}`);
  }

  const drafts = await db
    .select({
      id: schema.draft_records.id,
      draft_type: schema.draft_records.draft_type,
      draft_data: schema.draft_records.draft_data,
      draft_schema_version: schema.draft_records.draft_schema_version,
      posted_record_id: schema.draft_records.posted_record_id,
    })
    .from(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.batch_id, batchId),
        eq(schema.draft_records.user_id, userId),
        inArray(schema.draft_records.status, POSTABLE_STATUSES),
      ),
    );

  const results: PostDraftResult[] = [];

  for (const draft of drafts) {
    if (draft.posted_record_id) {
      results.push({
        draftId: draft.id,
        success: true,
        postedRecordId: draft.posted_record_id,
        postedRecordType: null,
        message: "Já materializado anteriormente",
        validationErrors: [],
      });
      continue;
    }

    const draftType = draft.draft_type as DraftType;
    const postableSchema = POSTABLE_SCHEMAS[draftType];
    if (!postableSchema) {
      results.push({
        draftId: draft.id,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: `Tipo de draft desconhecido: ${draft.draft_type}`,
        validationErrors: [],
      });
      continue;
    }

    const parsed = parseDraftPayload({
      draft_type: draft.draft_type,
      draft_data: draft.draft_data,
      draft_schema_version: draft.draft_schema_version,
    });
    if (!parsed.ok) {
      results.push({
        draftId: draft.id,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: "Draft com dados inconsistentes",
        validationErrors: parsed.errors,
      });
      continue;
    }

    const postable = postableSchema.safeParse(parsed.payload);
    if (!postable.success) {
      results.push({
        draftId: draft.id,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: "Faltam dados obrigatórios para lançar",
        validationErrors: formatIssues(postable.error),
      });
      continue;
    }

    const targetTable = TARGET_TABLES[draftType];
    const buildInsert = TO_INSERT[draftType] as (p: unknown) => Record<string, unknown>;
    const insertPayload = buildInsert(postable.data);

    try {
      // RETURNS jsonb (escalar, não tabela): "SELECT fn(...) AS result" é a
      // forma correta — ver o comentário equivalente em
      // apps/web/src/app/actions/materialization.ts, confirmado
      // empiricamente contra o Postgres local.
      const result = await db.execute<{
        result: { status: string; posted_record_id: string; posted_record_type: string };
      }>(
        sql`SELECT fn_materialize_draft_record(
          ${userId}::uuid,
          ${draft.id}::uuid,
          ${targetTable}::text,
          ${JSON.stringify(insertPayload)}::jsonb,
          ${"mcp"}::text
        ) AS result`,
      );
      const rpcResult = result.rows[0]?.result;

      if (!rpcResult) {
        results.push({
          draftId: draft.id,
          success: false,
          postedRecordId: null,
          postedRecordType: null,
          message: "Falha ao lançar: função não retornou resultado",
          validationErrors: [],
        });
        continue;
      }

      results.push({
        draftId: draft.id,
        success: true,
        postedRecordId: rpcResult.posted_record_id,
        postedRecordType: rpcResult.posted_record_type,
        message:
          rpcResult.status === "already_posted" ? "Já materializado anteriormente" : "Lançado",
        validationErrors: [],
      });
    } catch (err) {
      results.push({
        draftId: draft.id,
        success: false,
        postedRecordId: null,
        postedRecordType: null,
        message: `Falha ao lançar: ${err instanceof Error ? err.message : String(err)}`,
        validationErrors: [],
      });
    }
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
