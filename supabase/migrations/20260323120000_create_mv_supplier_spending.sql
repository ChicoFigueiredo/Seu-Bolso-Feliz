-- ============================================================
-- Migration 012: Materialized View — mv_supplier_spending
-- ============================================================
-- Agregação de gastos por fornecedor para painéis e relatórios.
-- Atualizada via Edge Function refresh-mv-supplier-spending.
-- ============================================================

CREATE MATERIALIZED VIEW mv_supplier_spending AS
SELECT
  s.user_id,
  s.id AS supplier_id,
  s.name AS supplier_name,
  s.type AS supplier_type,
  COUNT(DISTINCT t.id) AS transaction_count,
  COALESCE(SUM(t.amount), 0) AS total_spent,
  MIN(t.event_date) AS first_transaction_date,
  MAX(t.event_date) AS last_transaction_date,
  COUNT(DISTINCT t.financial_period_id) AS periods_active
FROM suppliers s
LEFT JOIN transactions t
  ON t.supplier_id = s.id
  AND t.user_id = s.user_id
  AND t.type IN ('expense', 'fee', 'interest_charge')
WHERE s.is_active = true
GROUP BY s.user_id, s.id, s.name, s.type;

-- Índices para performance em consultas filtradas por usuário
CREATE UNIQUE INDEX idx_mv_supplier_spending_pk
  ON mv_supplier_spending (user_id, supplier_id);

CREATE INDEX idx_mv_supplier_spending_total
  ON mv_supplier_spending (user_id, total_spent DESC);

-- ============================================================
-- RPC: Atualizar a materialized view
-- ============================================================
-- Usada pela Edge Function e pode ser chamada manualmente.
-- SECURITY DEFINER para bypass de RLS na view materializada.
-- ============================================================

CREATE OR REPLACE FUNCTION refresh_mv_supplier_spending()
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
BEGIN
  REFRESH MATERIALIZED VIEW CONCURRENTLY mv_supplier_spending;
END;
$$;

GRANT EXECUTE ON FUNCTION refresh_mv_supplier_spending() TO authenticated;
