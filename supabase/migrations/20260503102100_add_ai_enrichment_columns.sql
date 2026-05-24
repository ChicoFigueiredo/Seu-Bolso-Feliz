-- Migration: Adiciona colunas de enriquecimento de IA
-- Necessário para arquitetura híbrida 3 camadas (determinístico → IA lite → IA full)
-- Refino: docs/refinos/2026-05/2026-05-03-10-21-refino-estrategia-parsing-ia-hibrida-boletos.md

-- ── extraction_results ──────────────────────────────────────────────────────

ALTER TABLE extraction_results
  ADD COLUMN IF NOT EXISTS ai_enrichment_type   text          DEFAULT NULL
    CHECK (ai_enrichment_type IS NULL OR ai_enrichment_type IN ('lite', 'full')),
  ADD COLUMN IF NOT EXISTS ai_enrichment_at     timestamptz   DEFAULT NULL,
  ADD COLUMN IF NOT EXISTS confidence_per_field jsonb         DEFAULT NULL,
  ADD COLUMN IF NOT EXISTS reasoning            text          DEFAULT NULL,
  ADD COLUMN IF NOT EXISTS financial_intent     text          DEFAULT NULL
    CHECK (financial_intent IS NULL OR financial_intent IN (
      'transaction', 'recurring_expense', 'metric', 'liability_payment', 'unknown'
    ));

COMMENT ON COLUMN extraction_results.ai_enrichment_type   IS 'Nível de IA usado: null = só regex, lite = gpt-4o-mini, full = gpt-4o visão';
COMMENT ON COLUMN extraction_results.ai_enrichment_at     IS 'Timestamp do enriquecimento de IA';
COMMENT ON COLUMN extraction_results.confidence_per_field IS 'Confiança por campo: { "total_amount": 0.95, "due_date": 0.82, ... }';
COMMENT ON COLUMN extraction_results.reasoning            IS 'Reasoning da IA explicando a classificação (curto)';
COMMENT ON COLUMN extraction_results.financial_intent     IS 'Intenção financeira classificada: transaction | recurring_expense | metric | liability_payment | unknown';

-- ── ingestion_jobs ──────────────────────────────────────────────────────────

ALTER TABLE ingestion_jobs
  ADD COLUMN IF NOT EXISTS needs_full_ai_review boolean DEFAULT false;

COMMENT ON COLUMN ingestion_jobs.needs_full_ai_review IS 'Sinaliza que o documento precisou de análise full (visão) por não ter sido resolvido pelo modo lite';

-- ── índice para consultas frequentes ────────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_extraction_results_ai_enrichment_type
  ON extraction_results (ai_enrichment_type)
  WHERE ai_enrichment_type IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_ingestion_jobs_needs_full_ai_review
  ON ingestion_jobs (needs_full_ai_review, user_id)
  WHERE needs_full_ai_review = true;
