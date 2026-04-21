-- Adiciona source_document_id em transactions para rastrear origem documental
-- Necessário para S3-011 (coluna "Documento" na tela de transações)

ALTER TABLE transactions
  ADD COLUMN IF NOT EXISTS source_document_id uuid REFERENCES source_documents(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_transactions_source_document_id
  ON transactions(source_document_id)
  WHERE source_document_id IS NOT NULL;

COMMENT ON COLUMN transactions.source_document_id IS 'Documento de origem da transação (nota fiscal, fatura, comprovante)';
