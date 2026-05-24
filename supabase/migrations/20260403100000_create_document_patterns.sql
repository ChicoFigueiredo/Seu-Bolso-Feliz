-- Migration: create_document_patterns
-- ADR-006 — Padrões Documentais e Aprendizado
-- Tabelas: document_patterns, pattern_feedback
-- Trigger: auto-desativação quando feedback negativo > 3 e taxa de sucesso < 50%

-- ============================================================
-- 1. document_patterns
-- ============================================================

CREATE TABLE document_patterns (
  id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id              UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name                 TEXT NOT NULL,
  supplier_id          UUID REFERENCES suppliers(id) ON DELETE SET NULL,
  document_type        TEXT NOT NULL,
  institution_id       UUID REFERENCES institutions(id) ON DELETE SET NULL,

  -- Regras de extração: campo → regex/posição/instrução
  extraction_rules     JSONB NOT NULL DEFAULT '{}',

  -- Mapeamento: campo_extraído → campo_destino no draft
  field_mappings       JSONB NOT NULL DEFAULT '{}',

  -- Hashes SHA-256 de documentos-modelo usados para treinamento
  sample_fingerprints  TEXT[] NOT NULL DEFAULT '{}',

  -- Confiança mínima para auto-sugestão (0.00 a 1.00)
  confidence_threshold NUMERIC(3,2) NOT NULL DEFAULT 0.80
    CHECK (confidence_threshold BETWEEN 0.00 AND 1.00),

  -- Versionamento: versões antigas ficam com is_active=false para auditoria
  version              INT NOT NULL DEFAULT 1,

  is_active            BOOLEAN NOT NULL DEFAULT true,

  -- Contadores de feedback
  feedback_count       INT NOT NULL DEFAULT 0 CHECK (feedback_count >= 0),
  success_count        INT NOT NULL DEFAULT 0 CHECK (success_count >= 0),

  created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),

  -- Unicidade: mesmo usuário não pode ter dois padrões com mesmo nome e versão
  UNIQUE (user_id, name, version)
);

ALTER TABLE document_patterns ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users manage own patterns"
  ON document_patterns FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

-- updated_at automático
CREATE TRIGGER document_patterns_updated_at
  BEFORE UPDATE ON document_patterns
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Índices de busca frequente
CREATE INDEX idx_document_patterns_user_active
  ON document_patterns (user_id, is_active);

CREATE INDEX idx_document_patterns_supplier_type
  ON document_patterns (supplier_id, document_type)
  WHERE is_active = true;

CREATE INDEX idx_document_patterns_institution
  ON document_patterns (institution_id)
  WHERE is_active = true;

-- ============================================================
-- 2. pattern_feedback
-- ============================================================

CREATE TABLE pattern_feedback (
  id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id              UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  pattern_id           UUID NOT NULL REFERENCES document_patterns(id) ON DELETE CASCADE,
  source_document_id   UUID REFERENCES source_documents(id) ON DELETE SET NULL,

  -- Tipo de feedback
  feedback_type        TEXT NOT NULL
    CHECK (feedback_type IN ('correct', 'incorrect', 'partial', 'improved')),

  -- Correções aplicadas pelo usuário (campos → valores corrigidos)
  corrections          JSONB NOT NULL DEFAULT '{}',

  notes                TEXT,
  created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE pattern_feedback ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users manage own feedback"
  ON pattern_feedback FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

-- Índices de consulta frequente
CREATE INDEX idx_pattern_feedback_pattern
  ON pattern_feedback (pattern_id, created_at DESC);

CREATE INDEX idx_pattern_feedback_user
  ON pattern_feedback (user_id, created_at DESC);

CREATE INDEX idx_pattern_feedback_document
  ON pattern_feedback (source_document_id)
  WHERE source_document_id IS NOT NULL;

-- ============================================================
-- 3. Trigger: auto-desativação de padrões ruins
--
-- Regra (ADR-006 §4):
--   Se feedback_count > 3 (tipos 'incorrect' + 'improved')
--   E taxa de sucesso (success_count / (success_count + feedback_count)) < 0.50
--   → is_active = false
-- ============================================================

CREATE OR REPLACE FUNCTION check_pattern_auto_deactivation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  v_feedback_count INT;
  v_success_count  INT;
  v_total          INT;
BEGIN
  -- Só atua em feedback negativo
  IF NEW.feedback_type NOT IN ('incorrect', 'improved') THEN
    RETURN NEW;
  END IF;

  -- Busca contadores atuais do padrão
  SELECT feedback_count, success_count
    INTO v_feedback_count, v_success_count
    FROM document_patterns
   WHERE id = NEW.pattern_id;

  v_total := v_success_count + v_feedback_count;

  -- Aplica regra: feedback_count > 3 e taxa de sucesso < 50%
  IF v_feedback_count > 3 AND v_total > 0
     AND (v_success_count::NUMERIC / v_total) < 0.50 THEN
    UPDATE document_patterns
       SET is_active = false,
           updated_at = now()
     WHERE id = NEW.pattern_id
       AND is_active = true; -- idempotente
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER pattern_auto_deactivation
  AFTER INSERT ON pattern_feedback
  FOR EACH ROW EXECUTE FUNCTION check_pattern_auto_deactivation();

-- ============================================================
-- 4. RPC: register_pattern_feedback
--
-- Insere feedback e atualiza contadores em document_patterns
-- atomicamente. Chamada por server actions e AI tools.
-- ============================================================

CREATE OR REPLACE FUNCTION register_pattern_feedback(
  p_pattern_id        UUID,
  p_source_document_id UUID,  -- pode ser NULL
  p_feedback_type     TEXT,
  p_corrections       JSONB DEFAULT '{}'
)
RETURNS pattern_feedback
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  v_user_id UUID;
  v_row     pattern_feedback;
BEGIN
  -- Garante que o padrão pertence ao usuário autenticado
  SELECT user_id INTO v_user_id
    FROM document_patterns
   WHERE id = p_pattern_id
     AND user_id = auth.uid();

  IF NOT FOUND THEN
    RAISE EXCEPTION 'Pattern not found or access denied';
  END IF;

  -- Valida tipo de feedback
  IF p_feedback_type NOT IN ('correct', 'incorrect', 'partial', 'improved') THEN
    RAISE EXCEPTION 'Invalid feedback_type: %', p_feedback_type;
  END IF;

  -- Insere feedback
  INSERT INTO pattern_feedback (
    user_id,
    pattern_id,
    source_document_id,
    feedback_type,
    corrections
  ) VALUES (
    v_user_id,
    p_pattern_id,
    p_source_document_id,
    p_feedback_type,
    COALESCE(p_corrections, '{}')
  )
  RETURNING * INTO v_row;

  -- Atualiza contadores no padrão
  IF p_feedback_type = 'correct' THEN
    UPDATE document_patterns
       SET success_count = success_count + 1,
           updated_at    = now()
     WHERE id = p_pattern_id;
  ELSIF p_feedback_type IN ('incorrect', 'improved') THEN
    UPDATE document_patterns
       SET feedback_count = feedback_count + 1,
           updated_at     = now()
     WHERE id = p_pattern_id;
  END IF;
  -- 'partial' não incrementa nenhum contador (ambíguo)

  RETURN v_row;
END;
$$;

-- Permissão para usuários autenticados chamarem a RPC
GRANT EXECUTE ON FUNCTION register_pattern_feedback(UUID, UUID, TEXT, JSONB)
  TO authenticated;
