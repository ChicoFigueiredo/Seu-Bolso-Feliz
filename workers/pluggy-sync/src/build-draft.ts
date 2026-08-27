/**
 * NormalizedTransaction (Pluggy) → linha de insert em `draft_records`.
 * Função pura — nenhuma chamada de rede/banco aqui. `sync-runner.ts` cuida
 * do insert propriamente e da reconciliação.
 */
import { DRAFT_SCHEMA_VERSION } from "@sbf/contracts";
import { pluggyTransactionToDraft, type NormalizedTransaction } from "@sbf/financial-connectors";

export interface DraftRecordInsert {
  batch_id: string;
  user_id: string;
  source_document_id: null;
  extraction_result_id: null;
  draft_type: "transaction";
  status: "pending_review";
  draft_data: unknown;
  draft_schema_version: number;
  obligation_id: null;
  confidence_score: number;
  /** "pluggy:txn_<id>" — ver draft_records.external_ref, idempotência de sync. */
  external_ref: string;
}

/**
 * A Pluggy é uma fonte estruturada e conclusiva (não uma extração de OCR
 * sujeita a erro de leitura) — por isso confidence_score fixo em 1.0,
 * diferente dos drafts de documento, cuja confiança reflete a extração.
 */
const PLUGGY_CONFIDENCE_SCORE = 1.0;

export function buildDraftRecordInsert(
  tx: NormalizedTransaction,
  options: {
    batchId: string;
    userId: string;
    financialProductId: string | null;
    supplierId?: string | null;
  },
): DraftRecordInsert {
  const draftData = pluggyTransactionToDraft(tx, {
    financialProductId: options.financialProductId,
    supplierId: options.supplierId,
  });

  return {
    batch_id: options.batchId,
    user_id: options.userId,
    source_document_id: null,
    extraction_result_id: null,
    draft_type: "transaction",
    status: "pending_review",
    draft_data: draftData,
    draft_schema_version: DRAFT_SCHEMA_VERSION,
    obligation_id: null,
    confidence_score: PLUGGY_CONFIDENCE_SCORE,
    external_ref: `pluggy:txn_${tx.externalTransactionId}`,
  };
}
