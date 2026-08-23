-- P0-9: checkpoints de varredura, para backfill retomável.
--
-- Antes não existia nenhum: uma varredura interrompida recomeçava do zero e
-- dependia inteiramente da deduplicação por content_hash para não duplicar —
-- o que funciona, mas re-lista e re-baixa tudo, tornando um backfill de seis
-- meses caro e sem progresso observável.

CREATE TABLE IF NOT EXISTS ingestion_checkpoints (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,

  source_type text NOT NULL CHECK (source_type IN ('gmail', 'local_file')),
  /** Identifica a varredura: label + query compõem o escopo. */
  scope_key text NOT NULL,

  -- NUANCE QUE ARRUINA O RESUME SE IGNORADA:
  -- um pageToken do Gmail não é durável entre sessões. Ele serve apenas
  -- DENTRO de uma execução. O cursor durável de backfill é `internal_date`:
  -- caminha-se do mais novo para o mais antigo, persiste-se o menor
  -- internalDate visto, e o resume semeia `before:<data>`. Usar page_token
  -- como cursor durável produz um resume que reinicia do topo em silêncio.
  cursor_kind text NOT NULL CHECK (cursor_kind IN ('page_token', 'internal_date', 'mtime')),
  cursor_value text,
  last_message_id text,

  messages_seen integer NOT NULL DEFAULT 0,
  documents_created integer NOT NULL DEFAULT 0,
  duplicates_skipped integer NOT NULL DEFAULT 0,
  errors integer NOT NULL DEFAULT 0,

  window_start date,
  window_end date,

  status text NOT NULL DEFAULT 'running'
    CHECK (status IN ('running', 'paused', 'completed', 'failed')),

  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),

  UNIQUE (user_id, source_type, scope_key)
);

CREATE INDEX IF NOT EXISTS ix_ingestion_checkpoints_status
  ON ingestion_checkpoints (user_id, status);

COMMENT ON COLUMN ingestion_checkpoints.cursor_kind IS
  'page_token vale apenas dentro de uma execução; internal_date é o cursor durável de backfill.';

ALTER TABLE ingestion_checkpoints ENABLE ROW LEVEL SECURITY;

CREATE POLICY "ingestion_checkpoints_user_policy"
  ON ingestion_checkpoints FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

CREATE POLICY "ingestion_checkpoints_service_role"
  ON ingestion_checkpoints FOR ALL TO service_role
  USING (true) WITH CHECK (true);

CREATE OR REPLACE FUNCTION update_ingestion_checkpoints_updated_at()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_ingestion_checkpoints_updated_at ON ingestion_checkpoints;
CREATE TRIGGER trg_ingestion_checkpoints_updated_at
  BEFORE UPDATE ON ingestion_checkpoints
  FOR EACH ROW EXECUTE FUNCTION update_ingestion_checkpoints_updated_at();

-- rollback:
--   DROP TABLE IF EXISTS ingestion_checkpoints;
--   DROP FUNCTION IF EXISTS update_ingestion_checkpoints_updated_at();
