/**
 * @sbf/worker-ingestion — State machine transitions
 * Define transições válidas para jobs de ingestão.
 */
import { IngestionJobStatus } from "@sbf/ingestion-types";

type Status = (typeof IngestionJobStatus)[keyof typeof IngestionJobStatus];

/** Mapa de transições permitidas: status atual → estados válidos */
const TRANSITIONS: Record<Status, readonly Status[]> = {
  [IngestionJobStatus.DISCOVERED]: [IngestionJobStatus.DOWNLOADED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.DOWNLOADED]: [IngestionJobStatus.HASHED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.HASHED]: [IngestionJobStatus.QUEUED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.QUEUED]: [IngestionJobStatus.PARSING, IngestionJobStatus.FAILED],
  [IngestionJobStatus.PARSING]: [IngestionJobStatus.PARSED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.PARSED]: [
    IngestionJobStatus.AI_LITE_ENRICHING,
    IngestionJobStatus.CLASSIFIED,
    IngestionJobStatus.FAILED,
  ],
  [IngestionJobStatus.AI_LITE_ENRICHING]: [
    IngestionJobStatus.CLASSIFIED,
    IngestionJobStatus.FAILED,
  ],
  [IngestionJobStatus.CLASSIFIED]: [IngestionJobStatus.RECONCILED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.RECONCILED]: [IngestionJobStatus.DRAFTED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.DRAFTED]: [IngestionJobStatus.PENDING_REVIEW, IngestionJobStatus.FAILED],
  [IngestionJobStatus.PENDING_REVIEW]: [IngestionJobStatus.APPROVED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.APPROVED]: [IngestionJobStatus.POSTED, IngestionJobStatus.FAILED],
  [IngestionJobStatus.POSTED]: [],
  [IngestionJobStatus.FAILED]: [
    // Qualquer estado pode ser retomado por reprocessamento
    IngestionJobStatus.DISCOVERED,
    IngestionJobStatus.QUEUED,
  ],
};

/**
 * Verifica se a transição de status é válida.
 */
export function isValidTransition(from: Status, to: Status): boolean {
  const allowed = TRANSITIONS[from];
  return allowed !== undefined && allowed.includes(to);
}

/**
 * Retorna os estados de destino válidos a partir do estado atual.
 */
export function getValidNextStates(from: Status): readonly Status[] {
  return TRANSITIONS[from] ?? [];
}
