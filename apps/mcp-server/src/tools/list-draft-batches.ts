/**
 * @sbf/mcp-server — Tool: list_draft_batches
 * Lista batches de drafts pendentes de revisão.
 */
import { and, desc, eq, inArray } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
import { DraftBatchStatus } from "@sbf/ingestion-types";

export interface DraftBatchSummary {
  id: string;
  status: string;
  draft_count: number;
  source_document_filename: string | null;
  created_at: string;
  drafts: Array<{
    id: string;
    record_type: string;
    status: string;
    confidence: number | null;
    suggested_description: string | null;
  }>;
}

/**
 * `draft_records` nunca teve `record_type`/`confidence`/`payload` — as
 * colunas reais são `draft_type`/`confidence_score`/`draft_data`. O
 * `.select(...)` do driver anterior (string livre) não é checado
 * estaticamente, então essa ferramenta selecionava colunas inexistentes em
 * silêncio; o Postgres teria recusado a query e o `.select()` embutido
 * daquele driver (`source_documents(filename), draft_records(...)`) teria
 * devolvido erro, mascarado pelo `if (error || !batches) return [];` logo
 * abaixo. Corrigido para os nomes reais.
 */
function extractSuggestedDescription(draftData: unknown): string | null {
  if (!draftData || typeof draftData !== "object" || Array.isArray(draftData)) return null;
  const data = draftData as Record<string, unknown>;
  const value = data.description ?? data.suggested_description;
  return typeof value === "string" ? value : null;
}

export async function listDraftBatches(
  db: Database,
  userId: string,
  opts?: { status?: string; limit?: number },
): Promise<DraftBatchSummary[]> {
  const limit = opts?.limit ?? 20;
  const status = opts?.status ?? DraftBatchStatus.OPEN;

  // Sem API relacional (`db.query.*`) configurada neste monorepo — ver
  // packages/db/src/client.ts, que passa só `schema` para o drizzle(), sem
  // `relations`. O padrão já usado em apps/web/src/app/actions/ para
  // "um-para-muitos" é: uma query com LEFT/INNER JOIN para o lado 1:1
  // (aqui, o filename do documento) e uma segunda query + agrupamento em JS
  // para o lado 1:N (aqui, os drafts de cada batch).
  const batches = await db
    .select({
      id: schema.draft_batches.id,
      status: schema.draft_batches.status,
      created_at: schema.draft_batches.created_at,
      filename: schema.source_documents.filename,
    })
    .from(schema.draft_batches)
    .leftJoin(
      schema.source_documents,
      and(
        eq(schema.draft_batches.source_document_id, schema.source_documents.id),
        eq(schema.source_documents.user_id, userId),
      ),
    )
    .where(and(eq(schema.draft_batches.user_id, userId), eq(schema.draft_batches.status, status)))
    .orderBy(desc(schema.draft_batches.created_at))
    .limit(limit);

  if (batches.length === 0) return [];

  const batchIds = batches.map((b) => b.id);
  const drafts = await db
    .select({
      id: schema.draft_records.id,
      batch_id: schema.draft_records.batch_id,
      draft_type: schema.draft_records.draft_type,
      status: schema.draft_records.status,
      confidence_score: schema.draft_records.confidence_score,
      draft_data: schema.draft_records.draft_data,
    })
    .from(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.user_id, userId),
        inArray(schema.draft_records.batch_id, batchIds),
      ),
    );

  const draftsByBatch = new Map<string, typeof drafts>();
  for (const draft of drafts) {
    if (!draft.batch_id) continue;
    const list = draftsByBatch.get(draft.batch_id);
    if (list) {
      list.push(draft);
    } else {
      draftsByBatch.set(draft.batch_id, [draft]);
    }
  }

  return batches.map((b) => {
    const batchDrafts = draftsByBatch.get(b.id) ?? [];
    return {
      id: b.id,
      status: b.status,
      draft_count: batchDrafts.length,
      source_document_filename: b.filename ?? null,
      created_at: b.created_at ?? "",
      drafts: batchDrafts.map((d) => ({
        id: d.id,
        record_type: d.draft_type,
        status: d.status,
        confidence: d.confidence_score,
        suggested_description: extractSuggestedDescription(d.draft_data),
      })),
    };
  });
}
