-- ─────────────────────────────────────────────────────────────────────────────
-- Migration: S3-009a — Função fn_reconciliation_progress
--
-- Retorna o progresso de reconciliação de um draft_batch:
--   total_count    — total de draft_records no batch
--   reconciled_count — registros aprovados (status = 'approved')
--   progress_pct   — percentual (0–100, arredondado 2 casas)
--
-- Chamada via supabase.rpc('fn_reconciliation_progress', { p_batch_id })
-- ─────────────────────────────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION fn_reconciliation_progress(p_batch_id UUID)
  RETURNS TABLE(
    total_count       BIGINT,
    reconciled_count  BIGINT,
    progress_pct      NUMERIC
  )
  LANGUAGE sql
  STABLE
  SECURITY DEFINER
AS $$
  SELECT
    COUNT(*)                                                           AS total_count,
    COUNT(*) FILTER (WHERE status = 'approved')                        AS reconciled_count,
    CASE WHEN COUNT(*) = 0 THEN 0::NUMERIC
         ELSE ROUND(
           COUNT(*) FILTER (WHERE status = 'approved')::NUMERIC
           / COUNT(*)::NUMERIC * 100,
           2
         )
    END                                                                AS progress_pct
  FROM draft_records
  WHERE batch_id = p_batch_id;
$$;

COMMENT ON FUNCTION fn_reconciliation_progress(UUID) IS
  'Retorna progresso de reconciliação de um draft_batch: '
  'total_count, reconciled_count, progress_pct (0–100). '
  'Usado na Tela 14 (variant=statement) para exibir barra de progresso.';
