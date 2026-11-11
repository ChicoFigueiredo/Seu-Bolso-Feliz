/**
 * Gerador de drafts financeiros.
 * Transforma extraction_results em draft_records para revisão do usuário.
 *
 * A classificação e a montagem do payload vivem em @sbf/contracts, importadas
 * também pelo materializador. Antes, cada lado tinha a sua própria ideia do
 * formato — o gerador emitia `type: "despesa"`/`due_date`/`base_amount` e o
 * materializador exigia `type: "expense"`/`event_date`/`amount` — e nada os
 * obrigava a concordar, então nenhum draft gerado jamais foi materializável.
 * Este arquivo agora cuida só de persistência.
 */
import { and, eq, notInArray, sql } from "drizzle-orm";
import { draft_batches, draft_records, extraction_results, type Database } from "@sbf/db";
import {
  DRAFT_SCHEMA_VERSION,
  DRAFT_SCHEMAS,
  buildDraftPayload,
  classifyDraftTypes,
  type DraftType,
} from "@sbf/contracts";
import { writeLog, type LogContext } from "../logger";
import { IngestionLogLevel } from "@sbf/ingestion-types";

/** O tipo que o schema declara para as colunas jsonb de draft_records. */
type JsonColumn = (typeof draft_records.$inferInsert)["draft_data"];

export type { DraftType };
export { classifyDraftTypes };

export interface DraftGenerationInput {
  db: Database;
  ctx: LogContext;
  userId: string;
  sourceDocumentId: string;
  extractionResultId: string | null;
  parsedVersionId: string;
  parserType: string;
  confidence: number;
  /** Obrigação canônica desta evidência, quando houve convergência (P0-7). */
  obligationId?: string | null;
}

export interface DraftGenerationResult {
  batchId: string;
  drafts: Array<{
    id: string;
    draftType: DraftType;
    confidence: number;
  }>;
}

/** Pipeline completo de geração de drafts */
export async function generateDrafts(input: DraftGenerationInput): Promise<DraftGenerationResult> {
  const {
    db,
    ctx,
    userId,
    sourceDocumentId,
    extractionResultId,
    parsedVersionId,
    parserType,
    confidence,
    obligationId = null,
  } = input;

  // Buscar extraction_result se existir
  let extractionData: Record<string, unknown> | null = null;
  if (extractionResultId) {
    const [er] = await db
      .select()
      .from(extraction_results)
      .where(eq(extraction_results.id, extractionResultId))
      .limit(1);

    if (er) {
      extractionData = er as unknown as Record<string, unknown>;
    }
  }

  // Supressão por obrigação: se esta obrigação já tem draft vivo, uma segunda
  // evidência dela (o mesmo boleto vindo do Gmail e da pasta local) não deve
  // gerar um segundo lote de revisão. Este é o retorno concreto de convergir
  // evidências para uma obrigação canônica.
  if (obligationId) {
    const existing = await db
      .select({
        id: draft_records.id,
        batch_id: draft_records.batch_id,
        draft_type: draft_records.draft_type,
      })
      .from(draft_records)
      .where(
        and(
          eq(draft_records.user_id, userId),
          eq(draft_records.obligation_id, obligationId),
          notInArray(draft_records.status, ["rejected", "archived"]),
        ),
      );

    if (existing.length > 0) {
      const batchId = existing[0]!.batch_id;
      await writeLog(
        db,
        ctx,
        IngestionLogLevel.INFO,
        `Obrigação ${obligationId.slice(0, 8)} já possui ${existing.length} draft(s); evidência anexada sem gerar novo lote.`,
      );
      return {
        batchId: batchId ?? "",
        drafts: existing.map((d) => ({
          id: d.id,
          draftType: d.draft_type as DraftType,
          confidence,
        })),
      };
    }
  }

  // Classificar tipos de draft
  const draftTypes = classifyDraftTypes(extractionData);

  await writeLog(
    db,
    ctx,
    IngestionLogLevel.INFO,
    `Classificado: ${draftTypes.join(", ")} (confiança: ${(confidence * 100).toFixed(0)}%)`,
  );

  // Criar batch
  let batch: { id: string } | undefined;
  try {
    [batch] = await db
      .insert(draft_batches)
      .values({
        user_id: userId,
        run_id: ctx.runId ?? null,
        source_document_id: sourceDocumentId,
        name: `Auto-import ${new Date().toISOString().split("T")[0]}`,
        status: "open",
        total_drafts: draftTypes.length,
      })
      .returning({ id: draft_batches.id });
  } catch (err) {
    throw new Error(
      `Failed to create draft_batch: ${err instanceof Error ? err.message : String(err)}`,
      { cause: err },
    );
  }

  if (!batch) throw new Error("Failed to create draft_batch: insert não devolveu linha");

  // Gerar draft_records
  const drafts: DraftGenerationResult["drafts"] = [];

  for (const draftType of draftTypes) {
    const draftData = extractionData
      ? buildDraftPayload(draftType, extractionData, {
          provenance: {
            source_document_id: sourceDocumentId,
            extraction_result_id: extractionResultId,
            parsed_version_id: parsedVersionId,
            parser_type: parserType,
          },
        })
      : null;

    // Um payload que não passa no próprio schema honesto indica extração
    // corrompida. Registrar e pular é melhor que gravar um draft que a tela
    // de revisão não conseguirá abrir.
    if (draftData) {
      const check = DRAFT_SCHEMAS[draftType].safeParse(draftData);
      if (!check.success) {
        await writeLog(
          db,
          ctx,
          IngestionLogLevel.WARN,
          `Payload inválido para draft ${draftType}: ${check.error.errors
            .map((e) => `${e.path.join(".")}: ${e.message}`)
            .join("; ")}`,
        );
        continue;
      }
    }

    // Ajustar confiança por tipo
    const draftConfidence =
      draftType === "transaction" ? confidence : Math.max(confidence - 0.1, 0);

    let draft: { id: string } | undefined;
    try {
      [draft] = await db
        .insert(draft_records)
        .values({
          batch_id: batch.id,
          user_id: userId,
          source_document_id: sourceDocumentId,
          extraction_result_id: extractionResultId,
          draft_type: draftType,
          // Todo draft nasce aguardando revisão humana. O ternário anterior
          // tinha os dois ramos idênticos, sugerindo uma regra de confiança
          // que nunca existiu.
          status: "pending_review",
          draft_data: (draftData ?? { description: "Documento sem dados extraídos" }) as JsonColumn,
          draft_schema_version: draftData ? DRAFT_SCHEMA_VERSION : 0,
          obligation_id: obligationId,
          confidence_score: draftConfidence,
          // NOT NULL sem DEFAULT no DDL, mas preenchida pelo trigger BEFORE
          // INSERT `fn_draft_records_set_materialization_key`. `DEFAULT`
          // deixa o trigger agir; omitir a coluna não é opção porque o
          // schema Drizzle a declara obrigatória.
          materialization_key: sql`default`,
        })
        .returning({ id: draft_records.id });
    } catch (err) {
      await writeLog(
        db,
        ctx,
        IngestionLogLevel.WARN,
        `Failed to create draft (${draftType}): ${err instanceof Error ? err.message : String(err)}`,
      );
      continue;
    }

    if (draft) {
      drafts.push({ id: draft.id, draftType, confidence: draftConfidence });
    }
  }

  await writeLog(
    db,
    ctx,
    IngestionLogLevel.INFO,
    `Gerados ${drafts.length} drafts no batch ${batch.id}`,
    { batchId: batch.id, draftTypes },
  );

  return { batchId: batch.id, drafts };
}
