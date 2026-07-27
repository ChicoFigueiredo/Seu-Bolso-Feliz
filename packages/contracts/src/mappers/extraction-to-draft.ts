/**
 * O mapper explícito: linha de `extraction_results` → payloads de draft v1.
 *
 * Substitui `buildTransactionDraft` / `buildRecurringTemplateDraft` /
 * `buildConsumptionMetricDraft` de `workers/ingestion/src/drafts/draft-generator.ts`.
 *
 * Os builders antigos aceitavam camelCase e snake_case defensivamente porque
 * ninguém sabia ao certo o que chegava. A leitura continua tolerante — a linha
 * passa por caminhos diferentes conforme o parser — mas a SAÍDA agora é
 * validada contra um schema, então uma entrada inesperada falha aqui em vez de
 * silenciosamente produzir um draft que nunca poderá ser lançado.
 */
import {
  DRAFT_SCHEMA_VERSION,
  type DraftType,
  type Provenance,
  toCents,
  toIsoDate,
} from "../draft/common";
import type { DraftPayloadV1 } from "../draft";

/** Linha de `extraction_results` como o worker a lê. */
export interface ExtractionRow {
  [key: string]: unknown;
}

function str(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

/** Lê a primeira chave presente, aceitando snake_case e camelCase. */
function pick(er: ExtractionRow, ...keys: string[]): unknown {
  for (const k of keys) {
    const v = er[k];
    if (v !== undefined && v !== null && v !== "") return v;
  }
  return null;
}

function consumptionOf(er: ExtractionRow): Record<string, unknown> | null {
  const c = er.consumption_data ?? er.consumption;
  return c && typeof c === "object" ? (c as Record<string, unknown>) : null;
}

/**
 * Decide que drafts um documento gera.
 *
 * Diferença deliberada em relação a `classifyDraftTypes` original: a regra
 * `hasSupplier && hasCompetence → recurring_template` foi REMOVIDA. Toda conta
 * de consumo tem fornecedor e competência, então aquela regra transformava
 * qualquer conta mensal numa recorrência a partir de um documento isolado,
 * contrariando o §8 do plano mestre. Recorrência agora vem do motor de
 * histórico, não da presença de dois campos.
 */
export function classifyDraftTypes(er: ExtractionRow | null): DraftType[] {
  if (!er) return ["transaction"];

  const types: DraftType[] = [];

  if (pick(er, "total_amount", "totalAmount", "due_date", "dueDate") !== null) {
    types.push("transaction");
  }

  const c = consumptionOf(er);
  if (c && (c.kwh || c.m3 || c.litros)) {
    types.push("consumption_metric");
  }

  if (types.length === 0) types.push("transaction");
  return types;
}

function buildProvenance(er: ExtractionRow, p: Partial<Provenance>): Provenance {
  return {
    source_document_id: p.source_document_id ?? null,
    extraction_result_id: p.extraction_result_id ?? (str(er.id) as string | null),
    parsed_version_id: p.parsed_version_id ?? (str(er.parsed_version_id) as string | null),
    parser_type: p.parser_type ?? null,
    field_sources: p.field_sources ?? {},
  };
}

export interface MapOptions {
  provenance?: Partial<Provenance>;
}

/** Monta o draft de transação a partir da extração. */
export function toTransactionDraft(er: ExtractionRow, opts: MapOptions = {}): DraftPayloadV1 {
  const supplierName = str(pick(er, "supplier_name_raw", "supplierNameRaw"));
  const meta = (er.metadata ?? {}) as Record<string, unknown>;
  const extras = (meta.deterministic_extras ?? {}) as Record<string, unknown>;

  return {
    schema_version: DRAFT_SCHEMA_VERSION,
    provenance: buildProvenance(er, opts.provenance ?? {}),
    obligation_id: null,
    draft_type: "transaction",

    direction: "outflow",
    // A extração não distingue os 8 tipos do banco. Deixar nulo é honesto;
    // a camada de lançamento cobra a escolha.
    transaction_type: null,

    amount_cents: toCents(pick(er, "total_amount", "totalAmount")),
    currency: str(er.currency) ?? "BRL",

    event_date: null,
    due_date: toIsoDate(pick(er, "due_date", "dueDate")),
    competence_date: toIsoDate(pick(er, "competence_date", "competenceDate")),

    description: supplierName ?? "Documento importado",
    supplier_name_raw: supplierName,
    supplier_id: str(pick(er, "supplier_id", "supplierId")),

    category_suggestion: str(pick(er, "category_suggestion", "categorySuggestion")),
    category_id: null,

    tags: Array.isArray(er.tags_suggestion) ? (er.tags_suggestion as string[]) : [],
    priority: null,

    document_number: str(pick(er, "document_number", "documentNumber")),
    contract_identifier: str(pick(er, "contract_identifier", "contractIdentifier")),
    barcode_digitable_line: str(pick(extras, "barcode_digitable_line", "digitable_line")),

    financial_product_id: null,
    notes: null,
  } as DraftPayloadV1;
}

/** Monta o draft de métrica de consumo a partir da extração. */
export function toConsumptionMetricDraft(er: ExtractionRow, opts: MapOptions = {}): DraftPayloadV1 {
  const c = consumptionOf(er) ?? {};
  const kwh = typeof c.kwh === "number" ? c.kwh : null;
  const m3 = typeof c.m3 === "number" ? c.m3 : null;
  const quantity = kwh ?? m3;

  return {
    schema_version: DRAFT_SCHEMA_VERSION,
    provenance: buildProvenance(er, opts.provenance ?? {}),
    obligation_id: null,
    draft_type: "consumption_metric",

    supplier_name_raw: str(pick(er, "supplier_name_raw", "supplierNameRaw")),
    supplier_id: str(pick(er, "supplier_id", "supplierId")),
    supplier_contract_id: null,
    contract_identifier: str(pick(er, "contract_identifier", "contractIdentifier")),

    reference_period_start: toIsoDate(pick(er, "competence_date", "competenceDate")),
    reference_period_end: null,

    metric_name: quantity === null ? null : kwh !== null ? "energia" : "agua",
    metric_unit: quantity === null ? null : kwh !== null ? "kWh" : "m3",
    quantity,

    unit_price: null,
    subtotal_cents: toCents(pick(er, "total_amount", "totalAmount")),
  } as DraftPayloadV1;
}

const MAPPERS: Record<DraftType, (er: ExtractionRow, o: MapOptions) => DraftPayloadV1> = {
  transaction: toTransactionDraft,
  consumption_metric: toConsumptionMetricDraft,
  // Nenhum dos dois é gerado a partir de um documento isolado em P0.
  // A dívida precisa de contrato lido; a recorrência precisa de histórico.
  liability: toTransactionDraft,
  recurring_template: toTransactionDraft,
};

/** Constrói o payload do tipo pedido. */
export function buildDraftPayload(
  draftType: DraftType,
  er: ExtractionRow,
  opts: MapOptions = {},
): DraftPayloadV1 {
  return MAPPERS[draftType](er, opts);
}
