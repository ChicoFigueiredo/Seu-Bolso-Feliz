-- ============================================================
-- Migration: Deduplicação por Content Hash (SHA-256)
-- ============================================================
-- Corrige bug de deduplicação do Gmail Scanner: o attachmentId
-- da API do Gmail é efêmero (muda a cada chamada), então o
-- origin_key antigo (gmail:msgId:attachmentId) nunca colide.
--
-- Nova estratégia:
--   Fase 1 (pré-download): gmail_message_id + filename
--   Fase 2 (pós-download): SHA-256 content hash
--   origin_key agora usa: gmail:messageId:contentHash
-- ============================================================

-- 1. Adicionar coluna content_hash à source_documents
ALTER TABLE source_documents ADD COLUMN content_hash TEXT;

-- 2. Índice parcial único para dedup pré-download (gmail)
--    Evita re-download do mesmo anexo do mesmo e-mail
CREATE UNIQUE INDEX idx_source_documents_gmail_msg_file_dedup
  ON source_documents (user_id, gmail_message_id, filename)
  WHERE origin_type = 'gmail' AND gmail_message_id IS NOT NULL;

-- 3. Índice para busca rápida por content_hash
CREATE INDEX idx_source_documents_content_hash
  ON source_documents (user_id, content_hash)
  WHERE content_hash IS NOT NULL;
