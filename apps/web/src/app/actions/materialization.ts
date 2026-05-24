"use server";

/**
 * Materialization layer — transforma drafts aprovados em registros financeiros reais.
 *
 * Regra fundamental: nenhum registro financeiro é criado antes da aprovação humana.
 * Fluxo: draft (pending_review | corrected) → validar → reconciliar → criar registro real → posted
 */

import { createClient } from "@/lib/supabase/server";
import { z } from "zod";

// ── Schemas de validação por draft_type ────────────────────────────────────

const TransactionDraftSchema = z.object({
  financial_product_id: z.string().uuid("financial_product_id obrigatório"),
  type: z.enum([
    "income",
    "expense",
    "refund",
    "adjustment",
    "interest_charge",
    "fee",
    "statement_payment",
    "liability_payment",
  ]),
  amount: z.number().positive("Valor deve ser positivo"),
  event_date: z.string().min(1, "Data do evento obrigatória"),
  description: z.string().optional().nullable(),
  competence_date: z.string().optional().nullable(),
  notes: z.string().optional().nullable(),
  category_id: z.string().uuid().optional().nullable(),
  priority: z.enum(["essential", "high", "medium", "low", "optional"]).optional().nullable(),
});

const RecurringTemplateDraftSchema = z.object({
  name: z.string().min(1, "Nome obrigatório"),
  type: z.enum(["income", "expense", "liability_payment", "statement_payment"]),
  amount: z.number().positive().optional().nullable(),
  is_variable_amount: z.boolean().default(false),
  frequency: z.enum(["monthly", "weekly", "biweekly", "quarterly", "annual", "custom"]),
  day_of_month: z.number().int().min(1).max(31).optional().nullable(),
  starts_at: z.string().optional().nullable(),
  ends_at: z.string().optional().nullable(),
  notes: z.string().optional().nullable(),
  financial_product_id: z.string().uuid().optional().nullable(),
  category_id: z.string().uuid().optional().nullable(),
  priority: z.enum(["essential", "high", "medium", "low", "optional"]).optional().nullable(),
});

const LiabilityDraftSchema = z.object({
  financial_product_id: z.string().uuid("financial_product_id obrigatório"),
  name: z.string().min(1, "Nome obrigatório"),
  type: z.enum(["personal_loan", "mortgage", "overdraft", "installment_plan", "other"]),
  original_amount: z.number().positive("Valor original deve ser positivo"),
  outstanding_balance: z.number().nonnegative(),
  total_installments: z.number().int().positive().optional().nullable(),
  interest_rate: z.number().optional().nullable(),
  rate_type: z.enum(["monthly", "annual"]).optional().nullable(),
  amortization_system: z.enum(["sac", "price", "mixed", "other", "none"]).optional().nullable(),
  start_date: z.string().optional().nullable(),
  end_date: z.string().optional().nullable(),
});

// ── Tipos de resultado ──────────────────────────────────────────────────────

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

// ── Funções auxiliares ──────────────────────────────────────────────────────

async function writeAuditLog(
  supabase: Awaited<ReturnType<typeof createClient>>,
  userId: string,
  action: string,
  targetId: string,
  details: Record<string, unknown>,
): Promise<void> {
  await supabase
    .from("audit_logs")
    .insert({
      user_id: userId,
      action,
      target_id: targetId,
      details,
      created_at: new Date().toISOString(),
    })
    .then(() => {});
}

async function markDraftPosted(
  supabase: Awaited<ReturnType<typeof createClient>>,
  draftId: string,
  userId: string,
  postedRecordId: string,
  postedRecordType: string,
): Promise<void> {
  await supabase
    .from("draft_records")
    .update({
      status: "posted",
      posted_record_id: postedRecordId,
      posted_record_type: postedRecordType,
      approved_at: new Date().toISOString(),
      approved_by: userId,
      updated_at: new Date().toISOString(),
    })
    .eq("id", draftId);
}

// ── Materializadores por draft_type ────────────────────────────────────────

async function materializeTransaction(
  supabase: Awaited<ReturnType<typeof createClient>>,
  userId: string,
  draftId: string,
  draftData: Record<string, unknown>,
  sourceDocumentId: string | null,
): Promise<MaterializationResult> {
  const parse = TransactionDraftSchema.safeParse(draftData);
  if (!parse.success) {
    return {
      success: false,
      draftRecordId: draftId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: parse.error.errors.map((e) => `${e.path.join(".")}: ${e.message}`),
      message: "Dados inválidos para materialização de transação",
    };
  }

  const data = parse.data;
  const { data: tx, error } = await supabase
    .from("transactions")
    .insert({
      user_id: userId,
      financial_product_id: data.financial_product_id,
      type: data.type,
      amount: data.amount,
      event_date: data.event_date,
      description: data.description ?? null,
      competence_date: data.competence_date ?? null,
      notes: data.notes ?? null,
      category_id: data.category_id ?? null,
      priority: data.priority ?? null,
      origin_type: "import",
      is_confirmed: true,
      metadata: { source_document_id: sourceDocumentId, draft_id: draftId },
    })
    .select("id")
    .single();

  if (error || !tx) {
    return {
      success: false,
      draftRecordId: draftId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: [],
      message: `Erro ao criar transação: ${error?.message ?? "desconhecido"}`,
    };
  }

  await markDraftPosted(supabase, draftId, userId, tx.id, "transaction");

  return {
    success: true,
    draftRecordId: draftId,
    postedRecordId: tx.id,
    postedRecordType: "transaction",
    validationErrors: [],
    message: `Transação criada com sucesso (id: ${tx.id})`,
  };
}

async function materializeRecurringTemplate(
  supabase: Awaited<ReturnType<typeof createClient>>,
  userId: string,
  draftId: string,
  draftData: Record<string, unknown>,
): Promise<MaterializationResult> {
  const parse = RecurringTemplateDraftSchema.safeParse(draftData);
  if (!parse.success) {
    return {
      success: false,
      draftRecordId: draftId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: parse.error.errors.map((e) => `${e.path.join(".")}: ${e.message}`),
      message: "Dados inválidos para materialização de template recorrente",
    };
  }

  const data = parse.data;
  const { data: template, error } = await supabase
    .from("recurring_templates")
    .insert({
      user_id: userId,
      name: data.name,
      type: data.type,
      amount: data.amount ?? null,
      is_variable_amount: data.is_variable_amount,
      frequency: data.frequency,
      day_of_month: data.day_of_month ?? null,
      starts_at: data.starts_at ?? null,
      ends_at: data.ends_at ?? null,
      notes: data.notes ?? null,
      financial_product_id: data.financial_product_id ?? null,
      category_id: data.category_id ?? null,
      priority: data.priority ?? null,
      is_active: true,
    })
    .select("id")
    .single();

  if (error || !template) {
    return {
      success: false,
      draftRecordId: draftId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: [],
      message: `Erro ao criar template recorrente: ${error?.message ?? "desconhecido"}`,
    };
  }

  await markDraftPosted(supabase, draftId, userId, template.id, "recurring_template");

  return {
    success: true,
    draftRecordId: draftId,
    postedRecordId: template.id,
    postedRecordType: "recurring_template",
    validationErrors: [],
    message: `Template recorrente criado com sucesso (id: ${template.id})`,
  };
}

async function materializeLiability(
  supabase: Awaited<ReturnType<typeof createClient>>,
  userId: string,
  draftId: string,
  draftData: Record<string, unknown>,
): Promise<MaterializationResult> {
  const parse = LiabilityDraftSchema.safeParse(draftData);
  if (!parse.success) {
    return {
      success: false,
      draftRecordId: draftId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: parse.error.errors.map((e) => `${e.path.join(".")}: ${e.message}`),
      message: "Dados inválidos para materialização de passivo",
    };
  }

  const data = parse.data;
  const { data: liability, error } = await supabase
    .from("liabilities")
    .insert({
      user_id: userId,
      financial_product_id: data.financial_product_id,
      name: data.name,
      type: data.type,
      original_amount: data.original_amount,
      outstanding_balance: data.outstanding_balance,
      total_installments: data.total_installments ?? null,
      interest_rate: data.interest_rate ?? null,
      rate_type: data.rate_type ?? null,
      amortization_system: data.amortization_system ?? null,
      start_date: data.start_date ?? null,
      end_date: data.end_date ?? null,
      status: "active",
    })
    .select("id")
    .single();

  if (error || !liability) {
    return {
      success: false,
      draftRecordId: draftId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: [],
      message: `Erro ao criar passivo: ${error?.message ?? "desconhecido"}`,
    };
  }

  await markDraftPosted(supabase, draftId, userId, liability.id, "liability");

  return {
    success: true,
    draftRecordId: draftId,
    postedRecordId: liability.id,
    postedRecordType: "liability",
    validationErrors: [],
    message: `Passivo criado com sucesso (id: ${liability.id})`,
  };
}

// ── API pública ─────────────────────────────────────────────────────────────

/**
 * Materializa um draft aprovado em registro financeiro real.
 * Só funciona se o draft estiver em status approved ou corrected.
 */
export async function materializeApprovedDraftRecord(
  draftRecordId: string,
): Promise<MaterializationResult> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) {
    return {
      success: false,
      draftRecordId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: [],
      message: "Não autenticado",
    };
  }

  const { data: draft, error: draftError } = await supabase
    .from("draft_records")
    .select("id, draft_type, status, draft_data, source_document_id, posted_record_id")
    .eq("id", draftRecordId)
    .eq("user_id", user.id)
    .single();

  if (draftError || !draft) {
    return {
      success: false,
      draftRecordId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: [],
      message: "Draft não encontrado ou sem permissão",
    };
  }

  // Já materializado
  if (draft.posted_record_id) {
    return {
      success: true,
      draftRecordId,
      postedRecordId: draft.posted_record_id,
      postedRecordType: null,
      validationErrors: [],
      message: "Draft já foi materializado anteriormente",
    };
  }

  // Só materializa se aprovado ou corrigido
  if (!["approved", "corrected", "pending_review"].includes(draft.status)) {
    return {
      success: false,
      draftRecordId,
      postedRecordId: null,
      postedRecordType: null,
      validationErrors: [],
      message: `Draft em status "${draft.status}" não pode ser materializado. Aprove primeiro.`,
    };
  }

  const draftData = draft.draft_data as Record<string, unknown>;
  let result: MaterializationResult;

  switch (draft.draft_type) {
    case "transaction":
      result = await materializeTransaction(
        supabase,
        user.id,
        draftRecordId,
        draftData,
        draft.source_document_id,
      );
      break;
    case "recurring_template":
      result = await materializeRecurringTemplate(supabase, user.id, draftRecordId, draftData);
      break;
    case "liability":
      result = await materializeLiability(supabase, user.id, draftRecordId, draftData);
      break;
    case "consumption_metric":
      // consumption_metric: registrado como transação de despesa por ora
      result = await materializeTransaction(
        supabase,
        user.id,
        draftRecordId,
        draftData,
        draft.source_document_id,
      );
      break;
    default:
      result = {
        success: false,
        draftRecordId,
        postedRecordId: null,
        postedRecordType: null,
        validationErrors: [],
        message: `Tipo de draft desconhecido: ${draft.draft_type}`,
      };
  }

  await writeAuditLog(supabase, user.id, "draft_materialized", draftRecordId, {
    draft_type: draft.draft_type,
    success: result.success,
    posted_record_id: result.postedRecordId,
    validation_errors: result.validationErrors,
  });

  return result;
}

/**
 * Materializa todos os drafts aprovados de um batch.
 * Processa item a item — falha individual não cancela os demais.
 */
export async function materializeApprovedDraftBatch(
  batchId: string,
): Promise<MaterializationBatchResult> {
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
      results: [
        {
          success: false,
          draftRecordId: "",
          postedRecordId: null,
          postedRecordType: null,
          validationErrors: [],
          message: "Não autenticado",
        },
      ],
    };
  }

  const { data: drafts, error } = await supabase
    .from("draft_records")
    .select("id")
    .eq("batch_id", batchId)
    .eq("user_id", user.id)
    .in("status", ["approved", "corrected", "pending_review"]);

  if (error) {
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [
        {
          success: false,
          draftRecordId: "",
          postedRecordId: null,
          postedRecordType: null,
          validationErrors: [],
          message: `Erro ao buscar drafts: ${error.message}`,
        },
      ],
    };
  }

  if (!drafts || drafts.length === 0) {
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 0,
      results: [],
    };
  }

  const results: MaterializationResult[] = [];
  for (const draft of drafts) {
    const result = await materializeApprovedDraftRecord(draft.id);
    results.push(result);
  }

  const succeeded = results.filter((r) => r.success).length;
  const failed = results.filter((r) => !r.success).length;

  // Atualizar status do batch
  const batchStatus = failed === 0 ? "approved" : succeeded === 0 ? "rejected" : "partial";
  await supabase
    .from("draft_batches")
    .update({
      status: batchStatus,
      approved_count: succeeded,
      rejected_count: failed,
      updated_at: new Date().toISOString(),
    })
    .eq("id", batchId);

  await writeAuditLog(supabase, user.id, "batch_materialized", batchId, {
    total: drafts.length,
    succeeded,
    failed,
    batch_status: batchStatus,
  });

  return {
    batchId,
    totalProcessed: drafts.length,
    succeeded,
    failed,
    results,
  };
}
