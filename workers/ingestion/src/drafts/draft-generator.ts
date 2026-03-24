/**
 * Gerador de drafts financeiros.
 * Transforma extraction_results em draft_records para revisão do usuário.
 *
 * Tipos de draft suportados:
 * - transaction: despesa/receita simples
 * - recurring_template: despesa recorrente (ex: conta de luz mensal)
 * - consumption_metric: métrica de consumo (ex: kWh)
 * - liability: passivo/dívida (futuro)
 *
 * Itens 4.8-4.14 do checklist.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { writeLog, type LogContext } from "../logger";
import { IngestionLogLevel } from "@sbf/ingestion-types";

export type DraftType = "transaction" | "recurring_template" | "consumption_metric" | "liability";

export interface DraftGenerationInput {
  supabase: SupabaseClient;
  ctx: LogContext;
  userId: string;
  sourceDocumentId: string;
  extractionResultId: string | null;
  parsedVersionId: string;
  parserType: string;
  confidence: number;
}

export interface DraftGenerationResult {
  batchId: string;
  drafts: Array<{
    id: string;
    draftType: DraftType;
    confidence: number;
  }>;
}

/** Classificação automática: determina que tipo(s) de draft gerar */
export function classifyDraftTypes(extractionData: Record<string, unknown> | null): DraftType[] {
  const types: DraftType[] = [];

  if (!extractionData) {
    types.push("transaction");
    return types;
  }

  // Se tem totalAmount/dueDate, é no mínimo uma transaction
  if (extractionData.totalAmount || extractionData.total_amount || extractionData.dueDate || extractionData.due_date) {
    types.push("transaction");
  }

  // Se tem consumption_data ou kWh, gerar também consumption_metric
  const consumption = extractionData.consumption ?? extractionData.consumption_data;
  if (consumption && typeof consumption === "object") {
    const c = consumption as Record<string, unknown>;
    if (c.kwh || c.m3 || c.litros) {
      types.push("consumption_metric");
    }
  }

  // Se parece ser conta recorrente (fornecedor conhecido, competência mensal)
  const hasSupplier = extractionData.supplierNameRaw || extractionData.supplier_name_raw;
  const hasCompetence = extractionData.competenceDate || extractionData.competence_date;
  if (hasSupplier && hasCompetence) {
    types.push("recurring_template");
  }

  // Fallback: sempre pelo menos transaction
  if (types.length === 0) {
    types.push("transaction");
  }

  return types;
}

/** Cria o draft_data JSONB para uma transaction */
export function buildTransactionDraft(er: Record<string, unknown>): Record<string, unknown> {
  return {
    type: "despesa",
    description: er.supplier_name_raw ?? er.supplierNameRaw ?? "Documento importado",
    amount: er.total_amount ?? er.totalAmount ?? null,
    currency: er.currency ?? "BRL",
    due_date: er.due_date ?? er.dueDate ?? null,
    competence_date: er.competence_date ?? er.competenceDate ?? null,
    category: er.category_suggestion ?? null,
    tags: er.tags_suggestion ?? [],
    supplier_name: er.supplier_name_raw ?? er.supplierNameRaw ?? null,
    supplier_id: er.supplier_id ?? null,
    document_number: er.document_number ?? er.documentNumber ?? null,
    contract_identifier: er.contract_identifier ?? er.contractIdentifier ?? null,
  };
}

/** Cria o draft_data JSONB para um recurring_template */
export function buildRecurringTemplateDraft(er: Record<string, unknown>): Record<string, unknown> {
  return {
    name: `${er.supplier_name_raw ?? er.supplierNameRaw ?? "Conta"} - mensal`,
    type: "despesa",
    recurrence: "monthly",
    base_amount: er.total_amount ?? er.totalAmount ?? null,
    currency: er.currency ?? "BRL",
    category: er.category_suggestion ?? null,
    tags: er.tags_suggestion ?? [],
    supplier_name: er.supplier_name_raw ?? er.supplierNameRaw ?? null,
    supplier_id: er.supplier_id ?? null,
    contract_identifier: er.contract_identifier ?? er.contractIdentifier ?? null,
  };
}

/** Cria o draft_data JSONB para um consumption_metric */
export function buildConsumptionMetricDraft(er: Record<string, unknown>): Record<string, unknown> {
  const consumption = (er.consumption_data ?? er.consumption) as Record<string, unknown> | undefined;
  return {
    supplier_name: er.supplier_name_raw ?? er.supplierNameRaw ?? null,
    contract_identifier: er.contract_identifier ?? er.contractIdentifier ?? null,
    competence_date: er.competence_date ?? er.competenceDate ?? null,
    kwh: consumption?.kwh ?? null,
    m3: consumption?.m3 ?? null,
    days: consumption?.days ?? null,
    amount: er.total_amount ?? er.totalAmount ?? null,
    unit_cost: null, // Calculável: amount / kwh
  };
}

const DRAFT_BUILDERS: Record<DraftType, (er: Record<string, unknown>) => Record<string, unknown>> = {
  transaction: buildTransactionDraft,
  recurring_template: buildRecurringTemplateDraft,
  consumption_metric: buildConsumptionMetricDraft,
  liability: buildTransactionDraft, // Fallback para MVP
};

/** Pipeline completo de geração de drafts */
export async function generateDrafts(input: DraftGenerationInput): Promise<DraftGenerationResult> {
  const { supabase, ctx, userId, sourceDocumentId, extractionResultId, confidence } = input;

  // Buscar extraction_result se existir
  let extractionData: Record<string, unknown> | null = null;
  if (extractionResultId) {
    const { data: er } = await supabase
      .from("extraction_results")
      .select("*")
      .eq("id", extractionResultId)
      .single();

    if (er) {
      extractionData = er as unknown as Record<string, unknown>;
    }
  }

  // Classificar tipos de draft
  const draftTypes = classifyDraftTypes(extractionData);

  await writeLog(supabase, ctx, IngestionLogLevel.INFO,
    `Classificado: ${draftTypes.join(", ")} (confiança: ${(confidence * 100).toFixed(0)}%)`,
  );

  // Criar batch
  const { data: batch, error: batchError } = await supabase
    .from("draft_batches")
    .insert({
      user_id: userId,
      run_id: ctx.runId,
      name: `Auto-import ${new Date().toISOString().split("T")[0]}`,
      status: "open",
      total_drafts: draftTypes.length,
    })
    .select("id")
    .single();

  if (batchError || !batch) {
    throw new Error(`Failed to create draft_batch: ${batchError?.message}`);
  }

  // Gerar draft_records
  const drafts: DraftGenerationResult["drafts"] = [];

  for (const draftType of draftTypes) {
    const builder = DRAFT_BUILDERS[draftType];
    const draftData = extractionData ? builder(extractionData) : { description: "Documento sem dados extraídos" };

    // Ajustar confiança por tipo
    const draftConfidence = draftType === "transaction" ? confidence : Math.max(confidence - 0.1, 0);

    const { data: draft, error: draftError } = await supabase
      .from("draft_records")
      .insert({
        batch_id: batch.id,
        user_id: userId,
        source_document_id: sourceDocumentId,
        extraction_result_id: extractionResultId,
        draft_type: draftType,
        status: confidence >= 0.7 ? "pending_review" : "pending_review",
        draft_data: draftData,
        confidence_score: draftConfidence,
      })
      .select("id")
      .single();

    if (draftError) {
      await writeLog(supabase, ctx, IngestionLogLevel.WARN,
        `Failed to create draft (${draftType}): ${draftError.message}`,
      );
      continue;
    }

    if (draft) {
      drafts.push({ id: draft.id, draftType, confidence: draftConfidence });
    }
  }

  await writeLog(supabase, ctx, IngestionLogLevel.INFO,
    `Gerados ${drafts.length} drafts no batch ${batch.id}`,
    { batchId: batch.id, draftTypes },
  );

  return { batchId: batch.id, drafts };
}
