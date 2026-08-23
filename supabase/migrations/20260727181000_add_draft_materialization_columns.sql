-- P0-3: colunas e restrições que tornam a materialização rastreável e
-- estruturalmente única.
--
-- Contexto: draft_records tinha posted_record_id/posted_record_type sem FK,
-- sem índice único e sem timestamp. A única proteção contra lançar o mesmo
-- draft duas vezes era um SELECT seguido de um UPDATE no código da aplicação —
-- um TOCTOU clássico: duas aprovações concorrentes liam posted_record_id NULL
-- e ambas inseriam em transactions.

ALTER TABLE draft_records
  ADD COLUMN IF NOT EXISTS materialization_key text,
  ADD COLUMN IF NOT EXISTS posted_at timestamptz,
  ADD COLUMN IF NOT EXISTS materialization_error jsonb,
  ADD COLUMN IF NOT EXISTS obligation_id uuid
    REFERENCES financial_obligations(id) ON DELETE SET NULL;

COMMENT ON COLUMN draft_records.materialization_key IS
  'Chave de idempotência do lançamento. Única por usuário.';
COMMENT ON COLUMN draft_records.materialization_error IS
  'Último erro de materialização, para a tela de revisão explicar a falha sem consultar logs.';
COMMENT ON COLUMN draft_records.obligation_id IS
  'Obrigação financeira canônica que este draft materializa (P0-7).';

-- Backfill antes de tornar NOT NULL.
UPDATE draft_records
   SET materialization_key = 'draft:' || id::text
 WHERE materialization_key IS NULL;

-- A chave é derivada do id, então não pode ser um DEFAULT de coluna (que não
-- enxerga outras colunas da linha). Um trigger BEFORE INSERT garante o
-- invariante para qualquer escritor — worker, UI, MCP ou psql — em vez de
-- depender de cada um lembrar de preenchê-la.
CREATE OR REPLACE FUNCTION fn_draft_records_set_materialization_key()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  IF NEW.materialization_key IS NULL THEN
    NEW.materialization_key := 'draft:' || NEW.id::text;
  END IF;
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_draft_records_materialization_key ON draft_records;
CREATE TRIGGER trg_draft_records_materialization_key
  BEFORE INSERT ON draft_records
  FOR EACH ROW
  EXECUTE FUNCTION fn_draft_records_set_materialization_key();

ALTER TABLE draft_records ALTER COLUMN materialization_key SET NOT NULL;

-- Um draft é lançado no máximo uma vez.
CREATE UNIQUE INDEX IF NOT EXISTS uq_draft_records_materialization_key
  ON draft_records (user_id, materialization_key);

-- A verdadeira proteção contra duplo lançamento: um registro financeiro só
-- pode ser reivindicado por um draft. Isso vale mesmo que o código da
-- aplicação erre, porque é o banco que recusa.
CREATE UNIQUE INDEX IF NOT EXISTS uq_draft_records_posted_record
  ON draft_records (posted_record_type, posted_record_id)
  WHERE posted_record_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS ix_draft_records_obligation
  ON draft_records (obligation_id)
  WHERE obligation_id IS NOT NULL;

-- rollback:
--   DROP INDEX IF EXISTS ix_draft_records_obligation;
--   DROP INDEX IF EXISTS uq_draft_records_posted_record;
--   DROP INDEX IF EXISTS uq_draft_records_materialization_key;
--   ALTER TABLE draft_records
--     DROP COLUMN IF EXISTS obligation_id,
--     DROP COLUMN IF EXISTS materialization_error,
--     DROP COLUMN IF EXISTS posted_at,
--     DROP COLUMN IF EXISTS materialization_key;
