/**
 * Enums do ciclo de ingestão — conforme máquina de estados do plano 005.
 */

export const IngestionRunStatus = {
  RUNNING: "running",
  COMPLETED: "completed",
  FAILED: "failed",
  CANCELLED: "cancelled",
} as const;
export type IngestionRunStatus = (typeof IngestionRunStatus)[keyof typeof IngestionRunStatus];

export const IngestionJobStatus = {
  DISCOVERED: "discovered",
  DOWNLOADED: "downloaded",
  HASHED: "hashed",
  QUEUED: "queued",
  PARSING: "parsing",
  PARSED: "parsed",
  CLASSIFIED: "classified",
  RECONCILED: "reconciled",
  DRAFTED: "drafted",
  PENDING_REVIEW: "pending_review",
  APPROVED: "approved",
  POSTED: "posted",
  FAILED: "failed",
} as const;
export type IngestionJobStatus = (typeof IngestionJobStatus)[keyof typeof IngestionJobStatus];

export const SourceDocumentOrigin = {
  GMAIL: "gmail",
  LOCAL_FILE: "local_file",
  MANUAL_UPLOAD: "manual_upload",
} as const;
export type SourceDocumentOrigin = (typeof SourceDocumentOrigin)[keyof typeof SourceDocumentOrigin];

export const DraftRecordType = {
  TRANSACTION: "transaction",
  RECURRING_TEMPLATE: "recurring_template",
  LIABILITY: "liability",
  CONSUMPTION_METRIC: "consumption_metric",
} as const;
export type DraftRecordType = (typeof DraftRecordType)[keyof typeof DraftRecordType];

export const DraftRecordStatus = {
  PENDING_REVIEW: "pending_review",
  APPROVED: "approved",
  POSTED: "posted",
  REJECTED: "rejected",
  CORRECTED: "corrected",
  ARCHIVED: "archived",
} as const;
export type DraftRecordStatus = (typeof DraftRecordStatus)[keyof typeof DraftRecordStatus];

export const DraftBatchStatus = {
  OPEN: "open",
  REVIEWING: "reviewing",
  APPROVED: "approved",
  PARTIAL: "partial",
  REJECTED: "rejected",
} as const;
export type DraftBatchStatus = (typeof DraftBatchStatus)[keyof typeof DraftBatchStatus];

export const IngestionLogLevel = {
  DEBUG: "debug",
  INFO: "info",
  WARN: "warn",
  ERROR: "error",
} as const;
export type IngestionLogLevel = (typeof IngestionLogLevel)[keyof typeof IngestionLogLevel];

export const ParserType = {
  LOCAL_TEXT: "local_text",
  LOCAL_REGEX: "local_regex",
  OPENAI_VISION: "openai_vision",
  OPENAI_TEXT: "openai_text",
} as const;
export type ParserType = (typeof ParserType)[keyof typeof ParserType];
