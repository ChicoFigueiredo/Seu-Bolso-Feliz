-- Idempotência para drafts que não nascem de um documento (Pluggy e futuras
-- fontes não-documentais). `source_documents` já resolve isso pra origem
-- documental via UNIQUE(user_id, origin_type, origin_key); draft_records não
-- tinha equivalente porque, até aqui, todo draft vinha de um documento (cujo
-- hash já garantia idempotência antes de chegar aqui).
--
-- `external_ref` é opaco de propósito: "<provider>:<external_id>", ex.
-- "pluggy:txn_abc123". Sincronizar o mesmo período duas vezes não duplica
-- draft — a constraint rejeita o segundo insert, tratado como no-op pelo
-- worker (ver workers/pluggy-sync).

ALTER TABLE draft_records ADD COLUMN external_ref TEXT;

CREATE UNIQUE INDEX idx_draft_records_external_ref
  ON draft_records (user_id, external_ref)
  WHERE external_ref IS NOT NULL;

-- rollback:
--   DROP INDEX IF EXISTS idx_draft_records_external_ref;
--   ALTER TABLE draft_records DROP COLUMN IF EXISTS external_ref;
