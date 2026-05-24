-- ─────────────────────────────────────────────────────────────────────────────
-- Migration: S3-001 — Criar tabela document_splits
--
-- Permite o rateio de um documento entre múltiplas categorias/tags/valores.
-- Exemplo: nota fiscal de R$ 500 rateada em R$ 300 (Alimentação) + R$ 200 (Trabalho).
--
-- Regras:
--   • amount > 0 (CHECK server-side)
--   • SUM(splits) ≤ source_document.amount — validado via trigger
--   • RLS: usuário vê apenas seus splits
--   • Conceito DocumentSplit vive em packages/domain
-- ─────────────────────────────────────────────────────────────────────────────

CREATE TABLE document_splits (
  id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  source_document_id UUID        NOT NULL REFERENCES source_documents(id) ON DELETE CASCADE,
  user_id            UUID        NOT NULL REFERENCES auth.users(id),
  category_id        UUID        REFERENCES categories(id) ON DELETE SET NULL,
  tags               UUID[]      NOT NULL DEFAULT '{}',
  amount             NUMERIC(14,2) NOT NULL,
  description        TEXT,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT document_splits_amount_positive CHECK (amount > 0)
);

-- ── Indexes ───────────────────────────────────────────────────────────────────
CREATE INDEX idx_document_splits_source_document ON document_splits(source_document_id);
CREATE INDEX idx_document_splits_user            ON document_splits(user_id);
CREATE INDEX idx_document_splits_category        ON document_splits(category_id);

-- ── RLS ───────────────────────────────────────────────────────────────────────
ALTER TABLE document_splits ENABLE ROW LEVEL SECURITY;

CREATE POLICY "document_splits: usuário vê seus próprios splits"
  ON document_splits FOR SELECT
  USING (user_id = auth.uid());

CREATE POLICY "document_splits: usuário insere seus splits"
  ON document_splits FOR INSERT
  WITH CHECK (user_id = auth.uid());

CREATE POLICY "document_splits: usuário atualiza seus splits"
  ON document_splits FOR UPDATE
  USING (user_id = auth.uid());

CREATE POLICY "document_splits: usuário remove seus splits"
  ON document_splits FOR DELETE
  USING (user_id = auth.uid());

-- ── Trigger updated_at ────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION fn_set_updated_at()
  RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;

-- Reutiliza a função se já existir (idempotente via OR REPLACE acima)
CREATE TRIGGER trg_document_splits_updated_at
  BEFORE UPDATE ON document_splits
  FOR EACH ROW EXECUTE FUNCTION fn_set_updated_at();

-- ── Trigger: valida que SUM(splits.amount) ≤ document.amount (quando definido) ─
CREATE OR REPLACE FUNCTION fn_validate_splits_sum()
  RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
  doc_amount    NUMERIC;
  current_sum   NUMERIC;
BEGIN
  -- Busca amount do documento (campo opcional em source_documents — via metadata JSONB)
  SELECT (metadata->>'amount')::NUMERIC
    INTO doc_amount
    FROM source_documents
   WHERE id = NEW.source_document_id;

  -- Se o documento não tem amount definido, não bloqueia
  IF doc_amount IS NULL THEN
    RETURN NEW;
  END IF;

  -- Soma splits existentes (excluindo o próprio registro em UPDATE)
  SELECT COALESCE(SUM(amount), 0)
    INTO current_sum
    FROM document_splits
   WHERE source_document_id = NEW.source_document_id
     AND id <> COALESCE(NEW.id, gen_random_uuid());

  IF (current_sum + NEW.amount) > doc_amount THEN
    RAISE EXCEPTION
      'Soma dos rateios (%.2f) excede o valor do documento (%.2f)',
      current_sum + NEW.amount, doc_amount;
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER trg_validate_splits_sum
  BEFORE INSERT OR UPDATE ON document_splits
  FOR EACH ROW EXECUTE FUNCTION fn_validate_splits_sum();

COMMENT ON TABLE document_splits IS
  'Rateio de documento entre categorias/tags. '
  'Conceito DocumentSplit vive em packages/domain. '
  'SUM(amount) validado server-side via trg_validate_splits_sum.';
