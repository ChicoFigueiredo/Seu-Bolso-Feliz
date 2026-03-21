-- ============================================================
-- Migration 007: Views e RPCs — Seu Bolso Feliz
-- ============================================================
-- v_expenses_deduplicated (ADR-001)
-- search_suppliers RPC (trigram fuzzy search)
-- ============================================================

-- ============================================================
-- 1. View de despesas deduplicadas (ADR-001)
-- ============================================================
-- Regras de deduplicação:
-- - Se statement_item tem transaction_id → conta APENAS a transaction
-- - Se statement_item NÃO tem transaction_id → conta o statement_item
-- - statement_payment NUNCA entra na soma de despesas
-- - transfers NUNCA entram na soma de despesas
-- - Exceção: relatório "Composição da Fatura" conta todos os statement_items
-- ============================================================

CREATE OR REPLACE VIEW v_expenses_deduplicated AS

-- Caso 1: statement_items SEM transação vinculada
SELECT
  si.id AS canonical_id,
  'statement_item'::text AS source_type,
  si.user_id,
  si.amount,
  si.description,
  si.supplier_id,
  NULL::uuid AS category_id,
  NULL::text AS priority,
  si.transaction_date AS event_date,
  NULL::date AS competence_date,
  NULL::uuid AS financial_period_id,
  si.statement_cycle_id
FROM statement_items si
WHERE si.transaction_id IS NULL

UNION ALL

-- Caso 2: transactions de despesa (incluindo vinculadas a statement_items)
SELECT
  t.id AS canonical_id,
  'transaction'::text AS source_type,
  t.user_id,
  t.amount,
  t.description,
  t.supplier_id,
  t.category_id,
  t.priority,
  t.event_date,
  t.competence_date,
  t.financial_period_id,
  t.statement_cycle_id
FROM transactions t
WHERE t.type IN ('expense', 'fee', 'interest_charge')
  AND t.type NOT IN ('statement_payment', 'refund');

-- RLS: a view herda as RLS policies das tabelas base (statement_items e transactions).
-- Como ambas têm RLS com user_id = auth.uid(), a view automaticamente filtra por usuário.

-- ============================================================
-- 2. RPC: Busca fuzzy de fornecedores (trigram)
-- ============================================================
-- Busca por nome do fornecedor OU por alias ativo,
-- retornando resultados ordenados por similaridade.
-- ============================================================

CREATE OR REPLACE FUNCTION search_suppliers(
  p_query text,
  p_limit int DEFAULT 10
)
RETURNS TABLE (
  id uuid,
  name text,
  type text,
  document_number text,
  is_active boolean,
  matched_alias text,
  similarity_score real
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
BEGIN
  RETURN QUERY
  WITH direct_matches AS (
    SELECT
      s.id,
      s.name,
      s.type,
      s.document_number,
      s.is_active,
      NULL::text AS matched_alias,
      similarity(s.name, p_query) AS similarity_score
    FROM suppliers s
    WHERE s.user_id = auth.uid()
      AND s.is_active = true
      AND (
        s.name % p_query
        OR s.name ILIKE '%' || p_query || '%'
      )
  ),
  alias_matches AS (
    SELECT
      s.id,
      s.name,
      s.type,
      s.document_number,
      s.is_active,
      sa.alias_name AS matched_alias,
      similarity(sa.alias_name, p_query) AS similarity_score
    FROM supplier_aliases sa
    JOIN suppliers s ON s.id = sa.supplier_id
    WHERE s.user_id = auth.uid()
      AND sa.is_active = true
      AND s.is_active = true
      AND (
        sa.alias_name % p_query
        OR sa.alias_name ILIKE '%' || p_query || '%'
      )
      AND (sa.valid_from IS NULL OR sa.valid_from <= CURRENT_DATE)
      AND (sa.valid_until IS NULL OR sa.valid_until >= CURRENT_DATE)
  ),
  combined AS (
    SELECT * FROM direct_matches
    UNION ALL
    SELECT * FROM alias_matches
  )
  SELECT sub.id, sub.name, sub.type, sub.document_number, sub.is_active, sub.matched_alias, sub.similarity_score
  FROM (
    SELECT DISTINCT ON (combined.id)
      combined.id,
      combined.name,
      combined.type,
      combined.document_number,
      combined.is_active,
      combined.matched_alias,
      combined.similarity_score
    FROM combined
    ORDER BY combined.id, combined.similarity_score DESC
  ) sub
  ORDER BY sub.similarity_score DESC
  LIMIT p_limit;
END;
$$;

-- Grant execute to authenticated users
GRANT EXECUTE ON FUNCTION search_suppliers(text, int) TO authenticated;

-- ============================================================
-- 3. RPC: Calcular período financeiro para uma data
-- ============================================================
-- Dado uma data e as preferências do usuário, retorna o
-- período financeiro correspondente.
-- ============================================================

CREATE OR REPLACE FUNCTION get_financial_period_for_date(
  p_date date
)
RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  v_period_id uuid;
BEGIN
  SELECT id INTO v_period_id
  FROM financial_periods
  WHERE user_id = auth.uid()
    AND p_date >= start_date
    AND p_date <= end_date
  ORDER BY start_date DESC
  LIMIT 1;

  RETURN v_period_id;
END;
$$;

GRANT EXECUTE ON FUNCTION get_financial_period_for_date(date) TO authenticated;

-- ============================================================
-- 4. RPC: Gerar períodos financeiros para um intervalo
-- ============================================================
-- A partir do dia de início do ciclo e um intervalo de meses,
-- gera os períodos financeiros automaticamente.
-- ============================================================

CREATE OR REPLACE FUNCTION generate_financial_periods(
  p_start_day int,
  p_months_ahead int DEFAULT 12,
  p_anchor_date date DEFAULT CURRENT_DATE
)
RETURNS int
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  v_user_id uuid;
  v_current_start date;
  v_current_end date;
  v_count int := 0;
  v_base_year int;
  v_base_month int;
BEGIN
  v_user_id := auth.uid();
  IF v_user_id IS NULL THEN
    RAISE EXCEPTION 'Not authenticated';
  END IF;

  -- Calcular o mês base a partir da anchor_date
  v_base_year := EXTRACT(YEAR FROM p_anchor_date)::int;
  v_base_month := EXTRACT(MONTH FROM p_anchor_date)::int;

  -- Se a anchor_date é antes do start_day deste mês, recuar 1 mês
  IF EXTRACT(DAY FROM p_anchor_date)::int < p_start_day THEN
    IF v_base_month = 1 THEN
      v_base_year := v_base_year - 1;
      v_base_month := 12;
    ELSE
      v_base_month := v_base_month - 1;
    END IF;
  END IF;

  FOR v_month_offset IN 0..p_months_ahead - 1 LOOP
    -- Calcular start_date: dia p_start_day do mês (base + offset)
    v_current_start := make_date(
      v_base_year + ((v_base_month + v_month_offset - 1) / 12),
      ((v_base_month + v_month_offset - 1) % 12) + 1,
      LEAST(p_start_day, date_part('day',
        (make_date(
          v_base_year + ((v_base_month + v_month_offset - 1) / 12),
          ((v_base_month + v_month_offset - 1) % 12) + 1,
          1
        ) + interval '1 month - 1 day')::date
      )::int)
    );

    -- Calcular end_date: dia anterior ao start do próximo ciclo
    v_current_end := make_date(
      v_base_year + ((v_base_month + v_month_offset) / 12),
      ((v_base_month + v_month_offset) % 12) + 1,
      LEAST(p_start_day, date_part('day',
        (make_date(
          v_base_year + ((v_base_month + v_month_offset) / 12),
          ((v_base_month + v_month_offset) % 12) + 1,
          1
        ) + interval '1 month - 1 day')::date
      )::int)
    ) - interval '1 day';

    -- Inserir apenas se não existir período sobreposto
    INSERT INTO financial_periods (user_id, start_date, end_date, label, is_current)
    SELECT
      v_user_id,
      v_current_start,
      v_current_end::date,
      to_char(v_current_start, 'DD/MM/YYYY') || ' a ' || to_char(v_current_end, 'DD/MM/YYYY'),
      (CURRENT_DATE >= v_current_start AND CURRENT_DATE <= v_current_end::date)
    WHERE NOT EXISTS (
      SELECT 1 FROM financial_periods fp
      WHERE fp.user_id = v_user_id
        AND fp.start_date = v_current_start
        AND fp.end_date = v_current_end::date
    );

    IF FOUND THEN
      v_count := v_count + 1;
    END IF;
  END LOOP;

  -- Atualizar is_current em todos os períodos do usuário
  UPDATE financial_periods
  SET is_current = (CURRENT_DATE >= start_date AND CURRENT_DATE <= end_date)
  WHERE user_id = v_user_id;

  RETURN v_count;
END;
$$;

GRANT EXECUTE ON FUNCTION generate_financial_periods(int, int, date) TO authenticated;
