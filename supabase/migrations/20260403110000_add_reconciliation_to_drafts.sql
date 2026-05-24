-- ──────────────────────────────────────────────────────────────────────────────
-- Migration: Adiciona suporte a reconciliação em draft_records
-- Marco 5 — Fase E (Reconciliação)
-- ──────────────────────────────────────────────────────────────────────────────

-- ── 1. Novas colunas em draft_records ──────────────────────────────────────

ALTER TABLE draft_records
  ADD COLUMN IF NOT EXISTS reconciliation_status TEXT
    NOT NULL DEFAULT 'not_checked'
    CHECK (reconciliation_status IN (
      'not_checked',
      'no_match',
      'match_exact',
      'match_fuzzy',
      'match_duplicate',
      'match_recurrence',
      'confirmed_new',
      'confirmed_duplicate'
    )),

  -- ID da transação com que foi reconciliado (qualquer tipo de match)
  ADD COLUMN IF NOT EXISTS reconciled_transaction_id UUID REFERENCES transactions(id) ON DELETE SET NULL,

  -- ID do template recorrente associado
  ADD COLUMN IF NOT EXISTS reconciled_template_id UUID REFERENCES recurring_templates(id) ON DELETE SET NULL,

  -- JSON com os candidatos encontrados e seus scores
  ADD COLUMN IF NOT EXISTS reconciliation_candidates JSONB DEFAULT '[]'::jsonb,

  -- Quando a reconciliação foi executada
  ADD COLUMN IF NOT EXISTS reconciled_at TIMESTAMPTZ;

-- ── 2. Índices ──────────────────────────────────────────────────────────────

-- Facilita filtros por status de reconciliação na UI e dashboards
CREATE INDEX IF NOT EXISTS idx_draft_records_reconciliation_status
  ON draft_records (user_id, reconciliation_status)
  WHERE reconciliation_status NOT IN ('not_checked', 'no_match');

-- Facilita busca de duplicatas confirmadas
CREATE INDEX IF NOT EXISTS idx_draft_records_reconciled_transaction
  ON draft_records (reconciled_transaction_id)
  WHERE reconciled_transaction_id IS NOT NULL;

-- ── 3. Comentários ─────────────────────────────────────────────────────────

COMMENT ON COLUMN draft_records.reconciliation_status IS
  'Status da reconciliação: not_checked (padrão), no_match, match_exact, match_fuzzy, match_duplicate, match_recurrence, confirmed_new, confirmed_duplicate';

COMMENT ON COLUMN draft_records.reconciliation_candidates IS
  'Array JSON com os candidatos encontrados pela engine de reconciliação (score, tipo, dados resumidos)';

COMMENT ON COLUMN draft_records.reconciled_at IS
  'Timestamp de quando a reconciliação foi executada pelo worker';
