-- ─────────────────────────────────────────────────────────────────────────────
-- Migration: S3-002 — Criar tabela document_transactions
--
-- Vincula um source_document a uma ou mais transactions (ledger).
-- Exemplo: fatura de cartão → lançamentos aprovados; nota fiscal → despesa paga.
--
-- Regras:
--   • link_type ∈ {payment, refund, installment, support}
--   • created_by ∈ {user, ai, pattern}
--   • Indexes em source_document_id e transaction_id
--   • RLS: usuário vê apenas seus vínculos
--   • Conceito DocumentTransactionLink vive em packages/domain
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE document_transactions (
  id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  source_document_id UUID        NOT NULL REFERENCES source_documents(id) ON DELETE CASCADE,
  transaction_id     UUID        NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
  user_id            UUID        NOT NULL REFERENCES auth.users(id),
  link_type          TEXT        NOT NULL
    CONSTRAINT document_transactions_link_type_check
    CHECK (link_type IN ('payment', 'refund', 'installment', 'support')),
  confidence         NUMERIC(4,3)
    CONSTRAINT document_transactions_confidence_range
    CHECK (confidence IS NULL OR (confidence >= 0 AND confidence <= 1)),
  created_by         TEXT        NOT NULL DEFAULT 'user'
    CONSTRAINT document_transactions_created_by_check
    CHECK (created_by IN ('user', 'ai', 'pattern')),
  notes              TEXT,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
  -- Unicidade: um documento pode vincular a uma transação apenas uma vez por tipo
  UNIQUE (source_document_id, transaction_id, link_type)
);

-- ── Indexes ───────────────────────────────────────────────────────────────────
CREATE INDEX idx_document_transactions_source ON document_transactions(source_document_id);
CREATE INDEX idx_document_transactions_tx     ON document_transactions(transaction_id);
CREATE INDEX idx_document_transactions_user   ON document_transactions(user_id);

-- ── RLS ───────────────────────────────────────────────────────────────────────
ALTER TABLE document_transactions ENABLE ROW LEVEL SECURITY;

CREATE POLICY "document_transactions: usuário vê seus vínculos"
  ON document_transactions FOR SELECT
  USING (user_id = auth.uid());

CREATE POLICY "document_transactions: usuário cria vínculos"
  ON document_transactions FOR INSERT
  WITH CHECK (user_id = auth.uid());

CREATE POLICY "document_transactions: usuário remove vínculos"
  ON document_transactions FOR DELETE
  USING (user_id = auth.uid());

CREATE POLICY "document_transactions: usuário atualiza vínculos"
  ON document_transactions FOR UPDATE
  USING (user_id = auth.uid());

COMMENT ON TABLE document_transactions IS
  'Vínculo entre source_documents e transactions (ledger). '
  'Conceito DocumentTransactionLink vive em packages/domain. '
  'link_type: payment | refund | installment | support. '
  'created_by: user | ai | pattern.';
