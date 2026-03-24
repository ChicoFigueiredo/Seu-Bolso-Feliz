-- ============================================================
-- Migration 016: Índices de Performance — Tabelas de Ingestão
-- ============================================================
-- Índices otimizados para os padrões de consulta do worker,
-- scanner e interface de revisão.
-- ============================================================

-- ingestion_runs: filtro por user + status
CREATE INDEX idx_ingestion_runs_user_status
  ON ingestion_runs (user_id, status);

CREATE INDEX idx_ingestion_runs_user_source
  ON ingestion_runs (user_id, source_type);

-- ingestion_jobs: fila de processamento (poll pattern)
CREATE INDEX idx_ingestion_jobs_queue
  ON ingestion_jobs (status, created_at)
  WHERE status IN ('queued', 'discovered');

CREATE INDEX idx_ingestion_jobs_run
  ON ingestion_jobs (run_id, status);

CREATE INDEX idx_ingestion_jobs_user_status
  ON ingestion_jobs (user_id, status);

-- source_documents: busca por origin e gmail
CREATE INDEX idx_source_documents_user_origin
  ON source_documents (user_id, origin_type);

CREATE INDEX idx_source_documents_gmail_msg
  ON source_documents (user_id, gmail_message_id)
  WHERE gmail_message_id IS NOT NULL;

CREATE INDEX idx_source_documents_status
  ON source_documents (user_id, status);

-- document_fingerprints: busca por hash (deduplicação)
CREATE INDEX idx_document_fingerprints_canonical
  ON document_fingerprints (user_id, canonical_fingerprint)
  WHERE canonical_fingerprint IS NOT NULL;

-- parsed_document_versions: busca por doc
CREATE INDEX idx_parsed_versions_doc
  ON parsed_document_versions (source_document_id, version_number DESC);

-- extraction_results: busca por parsed version
CREATE INDEX idx_extraction_results_version
  ON extraction_results (parsed_version_id);

CREATE INDEX idx_extraction_results_supplier
  ON extraction_results (user_id, supplier_id)
  WHERE supplier_id IS NOT NULL;

-- draft_records: fila de revisão
CREATE INDEX idx_draft_records_review_queue
  ON draft_records (user_id, status, created_at)
  WHERE status = 'pending_review';

CREATE INDEX idx_draft_records_batch
  ON draft_records (batch_id)
  WHERE batch_id IS NOT NULL;

CREATE INDEX idx_draft_records_source_doc
  ON draft_records (source_document_id)
  WHERE source_document_id IS NOT NULL;

-- draft_batches: filtro por user e status
CREATE INDEX idx_draft_batches_user_status
  ON draft_batches (user_id, status);

-- ingestion_logs: busca por run/job e nível
CREATE INDEX idx_ingestion_logs_run
  ON ingestion_logs (run_id, created_at);

CREATE INDEX idx_ingestion_logs_job
  ON ingestion_logs (job_id, created_at)
  WHERE job_id IS NOT NULL;

CREATE INDEX idx_ingestion_logs_user_level
  ON ingestion_logs (user_id, level, created_at)
  WHERE level IN ('warn', 'error');
