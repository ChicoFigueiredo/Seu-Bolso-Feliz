-- ============================================================
-- Migration 003: ALTER tabelas existentes — supplier_id
-- ============================================================
-- Adiciona supplier_id nas tabelas core que podem ter
-- vínculo com fornecedor, conforme ADR-001/003.
-- ============================================================

ALTER TABLE transactions ADD COLUMN supplier_id uuid
  REFERENCES suppliers(id) ON DELETE SET NULL;

ALTER TABLE statement_items ADD COLUMN supplier_id uuid
  REFERENCES suppliers(id) ON DELETE SET NULL;

ALTER TABLE recurring_templates ADD COLUMN supplier_id uuid
  REFERENCES suppliers(id) ON DELETE SET NULL;

ALTER TABLE documents ADD COLUMN supplier_id uuid
  REFERENCES suppliers(id) ON DELETE SET NULL;

ALTER TABLE liabilities ADD COLUMN supplier_id uuid
  REFERENCES suppliers(id) ON DELETE SET NULL;
