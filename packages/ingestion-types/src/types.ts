/**
 * Tipos TypeScript para as 9 tabelas de ingestão — espelham o schema SQL.
 */

import type {
  IngestionRunStatus,
  IngestionJobStatus,
  SourceDocumentOrigin,
  DraftRecordType,
  DraftRecordStatus,
  DraftBatchStatus,
  IngestionLogLevel,
  ParserType,
} from "./enums";

export interface IngestionRun {
  id: string;
  user_id: string;
  source_type: SourceDocumentOrigin;
  status: IngestionRunStatus;
  started_at: string;
  completed_at: string | null;
  metadata: Record<string, unknown>;
  stats: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface IngestionJob {
  id: string;
  run_id: string;
  user_id: string;
  source_document_id: string | null;
  status: IngestionJobStatus;
  error_message: string | null;
  error_details: Record<string, unknown> | null;
  retry_count: number;
  max_retries: number;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface SourceDocument {
  id: string;
  user_id: string;
  origin_type: SourceDocumentOrigin;
  origin_key: string;
  gmail_message_id: string | null;
  gmail_thread_id: string | null;
  gmail_attachment_id: string | null;
  gmail_label: string | null;
  gmail_date: string | null;
  gmail_from: string | null;
  gmail_subject: string | null;
  local_filepath: string | null;
  local_mtime: string | null;
  filename: string;
  mime_type: string | null;
  file_size_bytes: number | null;
  storage_path: string | null;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface DocumentFingerprint {
  id: string;
  source_document_id: string;
  user_id: string;
  content_hash: string;
  canonical_fingerprint: string | null;
  hash_algorithm: string;
  created_at: string;
}

export interface ParsedDocumentVersion {
  id: string;
  source_document_id: string;
  user_id: string;
  version_number: number;
  parser_type: ParserType;
  parser_version: string | null;
  raw_text: string | null;
  structured_data: Record<string, unknown> | null;
  confidence_score: number | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface ExtractionResult {
  id: string;
  parsed_version_id: string;
  user_id: string;
  supplier_name_raw: string | null;
  supplier_id: string | null;
  supplier_confidence: number | null;
  competence_date: string | null;
  due_date: string | null;
  total_amount: number | null;
  currency: string;
  breakdown: Record<string, unknown> | null;
  document_number: string | null;
  contract_identifier: string | null;
  consumption_data: Record<string, unknown> | null;
  category_suggestion: string | null;
  tags_suggestion: string[] | null;
  priority_suggestion: string | null;
  financial_period_suggestion: Record<string, unknown> | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface DraftRecord {
  id: string;
  batch_id: string | null;
  user_id: string;
  source_document_id: string | null;
  extraction_result_id: string | null;
  draft_type: DraftRecordType;
  status: DraftRecordStatus;
  draft_data: Record<string, unknown>;
  corrections: Record<string, unknown> | null;
  confidence_score: number | null;
  approved_at: string | null;
  approved_by: string | null;
  posted_record_id: string | null;
  posted_record_type: string | null;
  created_at: string;
  updated_at: string;
}

export interface DraftBatch {
  id: string;
  user_id: string;
  run_id: string | null;
  name: string | null;
  status: DraftBatchStatus;
  total_drafts: number;
  approved_count: number;
  rejected_count: number;
  created_at: string;
  updated_at: string;
}

export interface IngestionLog {
  id: string;
  user_id: string;
  run_id: string | null;
  job_id: string | null;
  level: IngestionLogLevel;
  message: string;
  details: Record<string, unknown>;
  created_at: string;
}
