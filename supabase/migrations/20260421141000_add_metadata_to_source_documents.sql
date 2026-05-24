-- Adiciona colunas de enriquecimento semântico em source_documents
-- Necessárias para Telas 13 e 14 (S3-007/008)

ALTER TABLE source_documents
  ADD COLUMN IF NOT EXISTS metadata       jsonb,
  ADD COLUMN IF NOT EXISTS document_type  text,
  ADD COLUMN IF NOT EXISTS supplier_id    uuid REFERENCES suppliers(id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS supplier_name_raw text;

-- Índice para filtro por tipo de documento
CREATE INDEX IF NOT EXISTS idx_source_documents_document_type
  ON source_documents(user_id, document_type)
  WHERE document_type IS NOT NULL;

-- Índice para filtro por fornecedor
CREATE INDEX IF NOT EXISTS idx_source_documents_supplier_id
  ON source_documents(supplier_id)
  WHERE supplier_id IS NOT NULL;

COMMENT ON COLUMN source_documents.metadata IS 'Metadados extraídos do documento (amount, date, due_date, total, cycle_start, cycle_end, etc.)';
COMMENT ON COLUMN source_documents.document_type IS 'Tipo semântico do documento: invoice, receipt, credit_card_statement, bank_statement, etc.';
COMMENT ON COLUMN source_documents.supplier_id IS 'Fornecedor/empresa vinculada ao documento';
COMMENT ON COLUMN source_documents.supplier_name_raw IS 'Nome bruto do fornecedor extraído do documento antes de normalização';
