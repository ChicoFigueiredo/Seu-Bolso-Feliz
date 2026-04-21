-- ─────────────────────────────────────────────────────────────────────────────
-- Migration: S2-002 — Renomear tabela documents → documents_legacy
--
-- ⚠️ NÃO dropar documents_legacy sem confirmação explícita do CEO após 30 dias
--    de validação em staging.
--
-- O rename garante que:
--   • Os dados históricos não se percam
--   • A UI legada não quebre de imediato (graceful degradation)
--   • O novo fluxo use exclusivamente source_documents
--
-- Após 30 dias em staging sem regressão, o CEO confirma e a tabela é dropada
-- em uma nova migration de limpeza.
-- ─────────────────────────────────────────────────────────────────────────────

ALTER TABLE documents RENAME TO documents_legacy;

-- Comentário técnico para consultas futuras
COMMENT ON TABLE documents_legacy IS
  'Tabela legada renomeada em 2026-04-21 (S2-002). '
  'Substituída por source_documents. '
  'DROP agendado para após 30 dias de validação em staging pelo CEO. '
  'Não dropar sem confirmação explícita.';
