/**
 * Migração v0 → v1 dos payloads de draft.
 *
 * v0 é o formato que `draft-generator.ts` produzia antes da unificação:
 * `type: "despesa"`, `amount` cru (que o Supabase devolve como string para
 * colunas `numeric`), `due_date` sem `event_date`, `category` textual,
 * `recurrence`/`base_amount` em vez de `frequency`/`amount`.
 *
 * Esta função é a ÚNICA definição da conversão. O backfill em
 * `scripts/backfill-draft-schema-v1.ts` a importa daqui em vez de reimplementá-la
 * em plpgsql — duas cópias da mesma regra divergem, e uma divergência aqui
 * corromperia silenciosamente drafts históricos.
 */
import {
  DRAFT_SCHEMA_VERSION,
  type DraftType,
  type Provenance,
  toCents,
  toIsoDate,
} from "./common";
import type { DraftPayloadV1 } from "./index";

const EMPTY_PROVENANCE: Provenance = {
  source_document_id: null,
  extraction_result_id: null,
  parsed_version_id: null,
  parser_type: null,
  field_sources: {},
};

function str(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

function pick(o: Record<string, unknown>, ...keys: string[]): unknown {
  for (const k of keys) {
    const v = o[k];
    if (v !== undefined && v !== null && v !== "") return v;
  }
  return null;
}

/**
 * Traduz a direção. v0 gravava o literal português `"despesa"`; qualquer outro
 * valor conhecido de entrada vira `inflow`.
 */
function directionFromV0(raw: unknown): "outflow" | "inflow" {
  const t = typeof raw === "string" ? raw.toLowerCase() : "";
  if (t === "receita" || t === "income" || t === "inflow") return "inflow";
  return "outflow";
}

function transactionTypeFromV0(
  raw: unknown,
): DraftPayloadV1["draft_type"] extends never ? never : string | null {
  const t = typeof raw === "string" ? raw.toLowerCase() : "";
  if (t === "despesa" || t === "expense") return "expense";
  if (t === "receita" || t === "income") return "income";
  const known = [
    "refund",
    "adjustment",
    "interest_charge",
    "fee",
    "statement_payment",
    "liability_payment",
  ];
  return known.includes(t) ? t : null;
}

/** Mapeia `recurrence` de v0 para o enum `frequency` do banco. */
function frequencyFromV0(raw: unknown): string | null {
  const t = typeof raw === "string" ? raw.toLowerCase() : "";
  const known = ["monthly", "weekly", "biweekly", "quarterly", "annual", "custom"];
  if (known.includes(t)) return t;
  if (t === "mensal") return "monthly";
  if (t === "semanal") return "weekly";
  if (t === "quinzenal") return "biweekly";
  if (t === "trimestral") return "quarterly";
  if (t === "anual") return "annual";
  return null;
}

/**
 * Converte um `draft_data` v0 em `DraftPayloadV1`.
 *
 * Nunca lança: um payload irreconhecível vira um draft mínimo válido do tipo
 * pedido, para que um registro histórico corrompido não derrube a tela de
 * revisão inteira.
 */
export function migrateV0ToV1(
  draftType: DraftType,
  raw: unknown,
  provenance: Provenance = EMPTY_PROVENANCE,
): DraftPayloadV1 {
  const o: Record<string, unknown> =
    raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};

  const base = {
    schema_version: DRAFT_SCHEMA_VERSION,
    provenance,
    obligation_id: null,
  } as const;

  const supplierName = str(pick(o, "supplier_name", "supplier_name_raw", "supplierNameRaw"));
  const supplierId = str(pick(o, "supplier_id", "supplierId"));
  const contractIdentifier = str(pick(o, "contract_identifier", "contractIdentifier"));

  if (draftType === "transaction") {
    const dueDate = toIsoDate(pick(o, "due_date", "dueDate"));
    return {
      ...base,
      draft_type: "transaction",
      direction: directionFromV0(o.type),
      transaction_type: transactionTypeFromV0(o.type) as never,
      amount_cents: toCents(pick(o, "amount", "total_amount", "totalAmount")),
      currency: str(o.currency) ?? "BRL",
      // v0 nunca gravava event_date. Não inventamos uma: a lacuna é real e a
      // camada de lançamento vai cobrá-la do humano.
      event_date: toIsoDate(pick(o, "event_date", "eventDate")),
      due_date: dueDate,
      competence_date: toIsoDate(pick(o, "competence_date", "competenceDate")),
      description: str(o.description),
      supplier_name_raw: supplierName,
      supplier_id: supplierId,
      // v0 guardava categoria como texto livre. Vira sugestão, nunca category_id.
      category_suggestion: str(pick(o, "category", "category_suggestion")),
      category_id: str(o.category_id),
      tags: Array.isArray(o.tags) ? (o.tags as string[]) : [],
      priority: null,
      document_number: str(pick(o, "document_number", "documentNumber")),
      contract_identifier: contractIdentifier,
      barcode_digitable_line: str(pick(o, "barcode_digitable_line", "digitable_line")),
      financial_product_id: str(o.financial_product_id),
      notes: str(o.notes),
    } as DraftPayloadV1;
  }

  if (draftType === "consumption_metric") {
    const competence = toIsoDate(pick(o, "competence_date", "competenceDate"));
    const kwh = o.kwh ?? null;
    const m3 = o.m3 ?? null;
    const quantity = typeof kwh === "number" ? kwh : typeof m3 === "number" ? m3 : null;
    return {
      ...base,
      draft_type: "consumption_metric",
      supplier_name_raw: supplierName,
      supplier_id: supplierId,
      supplier_contract_id: null,
      contract_identifier: contractIdentifier,
      // v0 tinha apenas competence_date; usamos como início e deixamos o fim
      // em aberto para revisão humana em vez de fabricar um intervalo.
      reference_period_start: competence,
      reference_period_end: null,
      metric_name: quantity === null ? null : typeof kwh === "number" ? "energia" : "agua",
      metric_unit: quantity === null ? null : typeof kwh === "number" ? "kWh" : "m3",
      quantity,
      unit_price: typeof o.unit_cost === "number" ? o.unit_cost : null,
      subtotal_cents: toCents(pick(o, "amount", "total_amount")),
    } as DraftPayloadV1;
  }

  if (draftType === "liability") {
    // v0 usava buildTransactionDraft aqui, então quase nada é aproveitável.
    return {
      ...base,
      draft_type: "liability",
      name: str(pick(o, "name", "description")) ?? supplierName,
      liability_type: null,
      original_amount_cents: toCents(pick(o, "original_amount", "amount", "total_amount")),
      outstanding_balance_cents: toCents(pick(o, "outstanding_balance")),
      total_installments: null,
      interest_rate: null,
      rate_type: null,
      amortization_system: null,
      start_date: toIsoDate(pick(o, "start_date")),
      end_date: toIsoDate(pick(o, "end_date")),
      financial_product_id: str(o.financial_product_id),
      supplier_name_raw: supplierName,
      contract_identifier: contractIdentifier,
    } as DraftPayloadV1;
  }

  return {
    ...base,
    draft_type: "recurring_template",
    name: str(o.name) ?? supplierName,
    recurring_type: transactionTypeFromV0(o.type) === "income" ? "income" : "expense",
    // v0 cravava "monthly" a partir de um documento só. Preservamos o valor
    // como pista, mas evidence_count = 1 impede que ele seja lançado.
    frequency: frequencyFromV0(pick(o, "frequency", "recurrence")) as never,
    day_of_month: null,
    amount_cents: toCents(pick(o, "amount", "base_amount")),
    is_variable_amount: o.is_variable_amount === true,
    starts_at: toIsoDate(pick(o, "starts_at")),
    ends_at: toIsoDate(pick(o, "ends_at")),
    supplier_name_raw: supplierName,
    supplier_id: supplierId,
    financial_product_id: str(o.financial_product_id),
    category_suggestion: str(pick(o, "category", "category_suggestion")),
    category_id: str(o.category_id),
    priority: null,
    notes: str(o.notes),
    evidence_count: 1,
    occurrences: [],
    explanation: ["Migrado de draft v0, que inferia recorrência a partir de um único documento."],
    confidence: null,
  } as DraftPayloadV1;
}
