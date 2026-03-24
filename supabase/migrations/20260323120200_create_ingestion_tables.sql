-- ============================================================
-- Migration 014: Tabelas de Ingestão Documental
-- ============================================================
-- 9 tabelas para o ciclo de ingestão, parsing e drafts.
-- Conforme planejamento 005, seção 2.1.
-- ============================================================

-- ── 1. Execuções do worker ──
CREATE TABLE ingestion_runs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_type TEXT NOT NULL CHECK (source_type IN ('gmail', 'local_file', 'manual_upload')),
  status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ,
  metadata JSONB DEFAULT '{}',
  stats JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- ── 2. Lotes de drafts (antes de draft_records por FK) ──
CREATE TABLE draft_batches (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  run_id UUID REFERENCES ingestion_runs(id),
  name TEXT,
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'reviewing', 'approved', 'partial', 'rejected')),
  total_drafts INT DEFAULT 0,
  approved_count INT DEFAULT 0,
  rejected_count INT DEFAULT 0,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- ── 3. Documentos canônicos ingeridos ──
CREATE TABLE source_documents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  origin_type TEXT NOT NULL CHECK (origin_type IN ('gmail', 'local_file', 'manual_upload')),
  origin_key TEXT NOT NULL,
  -- Gmail fields
  gmail_message_id TEXT,
  gmail_thread_id TEXT,
  gmail_attachment_id TEXT,
  gmail_label TEXT,
  gmail_date TIMESTAMPTZ,
  gmail_from TEXT,
  gmail_subject TEXT,
  -- Local file fields
  local_filepath TEXT,
  local_mtime TIMESTAMPTZ,
  -- Common fields
  filename TEXT NOT NULL,
  mime_type TEXT,
  file_size_bytes BIGINT,
  storage_path TEXT,
  status TEXT NOT NULL DEFAULT 'new',
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(user_id, origin_type, origin_key)
);

-- ── 4. Jobs individuais de processamento ──
CREATE TABLE ingestion_jobs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  run_id UUID REFERENCES ingestion_runs(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_document_id UUID REFERENCES source_documents(id),
  status TEXT NOT NULL DEFAULT 'discovered' CHECK (status IN (
    'discovered', 'downloaded', 'hashed', 'queued',
    'parsing', 'parsed', 'classified', 'reconciled',
    'drafted', 'pending_review', 'approved', 'posted', 'failed'
  )),
  error_message TEXT,
  error_details JSONB,
  retry_count INT DEFAULT 0,
  max_retries INT DEFAULT 3,
  metadata JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- ── 5. Fingerprints de documentos ──
CREATE TABLE document_fingerprints (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  source_document_id UUID REFERENCES source_documents(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  content_hash TEXT NOT NULL,
  canonical_fingerprint TEXT,
  hash_algorithm TEXT DEFAULT 'sha256',
  created_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(user_id, content_hash)
);

-- ── 6. Versões de parsing/extração ──
CREATE TABLE parsed_document_versions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  source_document_id UUID REFERENCES source_documents(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  version_number INT NOT NULL DEFAULT 1,
  parser_type TEXT NOT NULL CHECK (parser_type IN ('local_text', 'local_regex', 'openai_vision', 'openai_text')),
  parser_version TEXT,
  raw_text TEXT,
  structured_data JSONB,
  confidence_score NUMERIC(3,2),
  metadata JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(source_document_id, version_number)
);

-- ── 7. Resultados de extração estruturados ──
CREATE TABLE extraction_results (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  parsed_version_id UUID REFERENCES parsed_document_versions(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  supplier_name_raw TEXT,
  supplier_id UUID REFERENCES suppliers(id),
  supplier_confidence NUMERIC(3,2),
  competence_date DATE,
  due_date DATE,
  total_amount NUMERIC(12,2),
  currency TEXT DEFAULT 'BRL',
  breakdown JSONB,
  document_number TEXT,
  contract_identifier TEXT,
  consumption_data JSONB,
  category_suggestion TEXT,
  tags_suggestion TEXT[],
  priority_suggestion TEXT,
  financial_period_suggestion JSONB,
  metadata JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now()
);

-- ── 8. Drafts de registros financeiros ──
CREATE TABLE draft_records (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  batch_id UUID REFERENCES draft_batches(id),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_document_id UUID REFERENCES source_documents(id),
  extraction_result_id UUID REFERENCES extraction_results(id),
  draft_type TEXT NOT NULL CHECK (draft_type IN ('transaction', 'recurring_template', 'liability', 'consumption_metric')),
  status TEXT NOT NULL DEFAULT 'pending_review' CHECK (status IN (
    'pending_review', 'approved', 'posted', 'rejected', 'corrected', 'archived'
  )),
  draft_data JSONB NOT NULL,
  corrections JSONB,
  confidence_score NUMERIC(3,2),
  approved_at TIMESTAMPTZ,
  approved_by UUID REFERENCES auth.users(id),
  posted_record_id UUID,
  posted_record_type TEXT,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- ── 9. Logs de ingestão ──
CREATE TABLE ingestion_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  run_id UUID REFERENCES ingestion_runs(id),
  job_id UUID REFERENCES ingestion_jobs(id),
  level TEXT NOT NULL DEFAULT 'info' CHECK (level IN ('debug', 'info', 'warn', 'error')),
  message TEXT NOT NULL,
  details JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now()
);
