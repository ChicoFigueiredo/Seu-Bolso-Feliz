/**
 * `NormalizedTransaction` → `TransactionDraftV1` (@sbf/contracts).
 *
 * Mesmo destino que o pipeline de documentos (draft_records, draft_type
 * "transaction"), sem passar por EvidenceEnvelope/OCR — ver
 * docs/planejamento/2026-08-24-plano-integracao-pluggy.md §0.2 e ADR-008.
 * `source_document_id` fica null; `parser_type: "pluggy_provider"` no lugar
 * do nome do parser de documento, para que a origem continue rastreável.
 */
import {
  DRAFT_SCHEMA_VERSION,
  TransactionDraftV1Schema,
  type TransactionDraftV1,
} from "@sbf/contracts";
import type {
  NormalizedTransaction,
  NormalizedTransactionDirection,
} from "../financial-data-provider";

export interface MapToDraftOptions {
  /**
   * Resolvido de `external_account_mappings.financial_product_id` pelo
   * chamador — null se a conta Pluggy ainda não foi vinculada a um produto
   * financeiro (a materialização exige isso; o draft honesto, não).
   */
  financialProductId: string | null;
}

/**
 * A Pluggy classifica DEBIT/CREDIT com mais confiança do que qualquer
 * heurística de OCR — diferente do mapper de documento
 * (`packages/contracts/src/mappers/extraction-to-draft.ts`), que deixa
 * `transaction_type` null "enquanto a classificação não é conclusiva". Aqui a
 * fonte já é conclusiva, então preenchemos direto.
 */
function inferTransactionType(direction: NormalizedTransactionDirection): "income" | "expense" {
  return direction === "inflow" ? "income" : "expense";
}

export function pluggyTransactionToDraft(
  tx: NormalizedTransaction,
  options: MapToDraftOptions,
): TransactionDraftV1 {
  const draft: TransactionDraftV1 = {
    schema_version: DRAFT_SCHEMA_VERSION,
    provenance: {
      source_document_id: null,
      extraction_result_id: null,
      parsed_version_id: null,
      parser_type: "pluggy_provider",
      field_sources: {
        amount_cents: "pluggy_provider",
        direction: "pluggy_provider",
        transaction_type: "pluggy_provider",
        event_date: "pluggy_provider",
        supplier_name_raw: "pluggy_provider",
      },
    },
    obligation_id: null,
    draft_type: "transaction",
    direction: tx.direction,
    transaction_type: inferTransactionType(tx.direction),
    amount_cents: tx.amountCents,
    currency: tx.currency,
    event_date: tx.date,
    due_date: null,
    competence_date: null,
    description: tx.description || tx.descriptionRaw || null,
    supplier_name_raw: tx.merchantName ?? null,
    supplier_id: null,
    category_suggestion: tx.category ?? null,
    category_id: null,
    tags: [],
    priority: null,
    document_number: null,
    contract_identifier: null,
    barcode_digitable_line: null,
    financial_product_id: options.financialProductId,
    notes: null,
  };

  return TransactionDraftV1Schema.parse(draft);
}
