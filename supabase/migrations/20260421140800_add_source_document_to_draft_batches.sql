-- S3 — Adicionar vínculo source_document_id em draft_batches
-- Permite associar um batch de rascunhos a um documento de origem (ex: fatura importada)

ALTER TABLE draft_batches
  ADD COLUMN IF NOT EXISTS source_document_id UUID
    REFERENCES source_documents(id) ON DELETE SET NULL;

-- Índice para busca por documento
CREATE INDEX IF NOT EXISTS idx_draft_batches_source_document
  ON draft_batches (source_document_id)
  WHERE source_document_id IS NOT NULL;
