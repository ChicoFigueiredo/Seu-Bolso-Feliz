-- P0-1: versionamento do contrato de draft.
--
-- Contexto: o payload gravado em draft_records.draft_data seguia um formato
-- (v0) que o materializador nunca conseguiu validar — emitia type "despesa",
-- due_date sem event_date, base_amount em vez de amount. O pacote
-- @sbf/contracts define o formato v1; esta migration cria a coluna que permite
-- distinguir os dois e acompanhar o backfill.
--
-- A versão é uma coluna real, e não uma chave dentro do JSONB, justamente para
-- que o progresso do backfill seja consultável e indexável.

ALTER TABLE draft_records
  ADD COLUMN IF NOT EXISTS draft_schema_version smallint NOT NULL DEFAULT 0,
  ADD COLUMN IF NOT EXISTS draft_data_legacy jsonb;

COMMENT ON COLUMN draft_records.draft_schema_version IS
  'Versão do contrato em draft_data. 0 = formato legado pré-@sbf/contracts; 1 = DraftPayloadV1.';
COMMENT ON COLUMN draft_records.draft_data_legacy IS
  'Cópia intacta do draft_data v0, preservada pelo backfill para permitir auditoria e reversão.';

-- Índice parcial: só interessa localizar o que ainda falta migrar.
CREATE INDEX IF NOT EXISTS ix_draft_records_schema_version
  ON draft_records (user_id, draft_schema_version)
  WHERE draft_schema_version = 0;

-- rollback:
--   DROP INDEX IF EXISTS ix_draft_records_schema_version;
--   ALTER TABLE draft_records
--     DROP COLUMN IF EXISTS draft_schema_version,
--     DROP COLUMN IF EXISTS draft_data_legacy;
