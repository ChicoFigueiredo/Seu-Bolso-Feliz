/**
 * Converge uma evidência para a obrigação financeira canônica.
 *
 * Este módulo é o que faltava para `financial_obligations` sair do papel: as
 * tabelas existiam desde maio e nenhuma linha de TypeScript as tocava. Aqui a
 * extração vira chaves de identidade, e as chaves viram uma obrigação
 * deduplicada entre documentos e canais.
 *
 * O ganho concreto: a mesma conta chegando pelo Gmail e pela pasta local passa
 * a produzir UMA obrigação com DUAS evidências, em vez de dois lotes de
 * revisão independentes que virariam duas despesas.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import { buildFinancialIdentityKeys, type FinancialIdentityInput } from "@sbf/domain";
import { toObligationType, shouldCreateObligation } from "@sbf/contracts";

export interface ObligationUpsertInput {
  supabase: SupabaseClient;
  userId: string;
  sourceDocumentId: string;
  /** Linha de `extraction_results`, quando houve extração. */
  extraction: Record<string, unknown> | null;
  /** Intenção classificada, gravada em `ingestion_jobs.metadata` pelo scanner. */
  intent: string | null;
  confidence: number | null;
}

export interface ObligationUpsertResult {
  obligationId: string;
  isNew: boolean;
  matchedBy: string;
  evidenceCount: number;
}

function str(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

function num(v: unknown): number | null {
  if (v === null || v === undefined || v === "") return null;
  const n = typeof v === "string" ? Number(v) : v;
  return typeof n === "number" && Number.isFinite(n) ? n : null;
}

function pick(o: Record<string, unknown>, ...keys: string[]): unknown {
  for (const k of keys) {
    const v = o[k];
    if (v !== undefined && v !== null && v !== "") return v;
  }
  return null;
}

/** Monta a entrada da identity key a partir da linha de extração. */
export function buildIdentityInput(
  userId: string,
  extraction: Record<string, unknown> | null,
): FinancialIdentityInput {
  const er = extraction ?? {};
  const meta = (er.metadata ?? {}) as Record<string, unknown>;
  const extras = (meta.deterministic_extras ?? {}) as Record<string, unknown>;

  return {
    userId,
    supplierName: str(pick(er, "supplier_name_raw", "supplierNameRaw")),
    supplierCnpj: str(pick(extras, "supplier_cnpj", "cnpj")),
    institutionName: str(pick(extras, "institution_name")),
    amount: num(pick(er, "total_amount", "totalAmount")),
    dueDate: str(pick(er, "due_date", "dueDate")),
    competenceDate: str(pick(er, "competence_date", "competenceDate")),
    cycleStartDate: str(pick(extras, "cycle_start_date")),
    cycleEndDate: str(pick(extras, "cycle_end_date")),
    documentNumber: str(pick(er, "document_number", "documentNumber")),
    barcodeDigitableLine: str(pick(extras, "barcode_digitable_line", "digitable_line")),
    cardLast4: str(pick(extras, "card_last4", "card_last_four")),
  };
}

/**
 * Encontra ou cria a obrigação da evidência e anexa o documento a ela.
 *
 * Devolve `null` — sem erro — quando a intenção não gera obrigação (extrato,
 * histórico) ou quando não há campos suficientes para computar identidade
 * alguma. O pipeline segue normalmente nesses casos.
 */
export async function upsertObligationFromEvidence(
  input: ObligationUpsertInput,
): Promise<ObligationUpsertResult | null> {
  const { supabase, userId, sourceDocumentId, extraction, intent, confidence } = input;

  if (!shouldCreateObligation(intent)) return null;

  const identityInput = buildIdentityInput(userId, extraction);
  const keys = buildFinancialIdentityKeys(identityInput);

  // Sem chave computável não há como deduplicar; criar uma obrigação órfã só
  // acrescentaria ruído à revisão.
  if (keys.entries.length === 0) return null;

  const er = extraction ?? {};
  const meta = (er.metadata ?? {}) as Record<string, unknown>;
  const extras = (meta.deterministic_extras ?? {}) as Record<string, unknown>;

  const payload = {
    obligation_type: toObligationType(intent),
    supplier_id: str(pick(er, "supplier_id", "supplierId")),
    supplier_name_raw: identityInput.supplierName,
    amount: identityInput.amount,
    due_date: identityInput.dueDate,
    competence_date: identityInput.competenceDate,
    cycle_start_date: identityInput.cycleStartDate,
    cycle_end_date: identityInput.cycleEndDate,
    document_number: identityInput.documentNumber,
    barcode_digitable_line: identityInput.barcodeDigitableLine,
    financial_identity_key: keys.primary,
    confidence_score: confidence,
    metadata: { intent, card_last4: extras.card_last4 ?? null },
  };

  const { data, error } = await supabase.rpc("fn_upsert_financial_obligation", {
    p_user_id: userId,
    p_payload: payload,
    p_keys: keys.entries,
    p_evidence: {
      source_document_id: sourceDocumentId,
      confidence_score: confidence,
      reasons: [],
    },
  });

  if (error) throw new Error(`fn_upsert_financial_obligation: ${error.message}`);

  const r = data as {
    obligation_id: string;
    is_new: boolean;
    matched_by: string;
    evidence_count: number;
  };

  return {
    obligationId: r.obligation_id,
    isNew: r.is_new,
    matchedBy: r.matched_by,
    evidenceCount: r.evidence_count,
  };
}
