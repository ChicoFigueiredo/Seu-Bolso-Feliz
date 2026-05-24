-- ============================================================
-- Migration 015: RLS Policies — Tabelas de Ingestão
-- ============================================================
-- Todas as tabelas de ingestão filtram por user_id.
-- Padrão: FOR ALL USING (auth.uid() = user_id)
--         WITH CHECK (auth.uid() = user_id)
-- ============================================================

ALTER TABLE ingestion_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE ingestion_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE source_documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE document_fingerprints ENABLE ROW LEVEL SECURITY;
ALTER TABLE parsed_document_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE extraction_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE draft_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE draft_batches ENABLE ROW LEVEL SECURITY;
ALTER TABLE ingestion_logs ENABLE ROW LEVEL SECURITY;

-- ── Policies ──

CREATE POLICY ingestion_runs_policy ON ingestion_runs
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY ingestion_jobs_policy ON ingestion_jobs
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY source_documents_policy ON source_documents
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY document_fingerprints_policy ON document_fingerprints
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY parsed_document_versions_policy ON parsed_document_versions
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY extraction_results_policy ON extraction_results
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY draft_records_policy ON draft_records
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY draft_batches_policy ON draft_batches
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY ingestion_logs_policy ON ingestion_logs
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);
