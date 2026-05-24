-- ============================================================
-- Migration 013: RPC atômica — confirm_supplier_associations
-- ============================================================
-- Substitui o loop de UPDATEs individuais na Edge Function
-- retroactive-supplier-association por uma transação atômica.
-- ============================================================

CREATE OR REPLACE FUNCTION confirm_supplier_associations(
  p_user_id uuid,
  p_confirmations jsonb  -- [{ "transaction_id": "...", "supplier_id": "..." }, ...]
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  v_updated int := 0;
  v_total int;
  v_conf jsonb;
  v_tx_id uuid;
  v_sup_id uuid;
BEGIN
  v_total := jsonb_array_length(p_confirmations);

  IF v_total = 0 THEN
    RAISE EXCEPTION 'No confirmations provided';
  END IF;

  -- Validar que todos os transaction_ids pertencem ao usuário
  FOR v_conf IN SELECT * FROM jsonb_array_elements(p_confirmations)
  LOOP
    v_tx_id := (v_conf ->> 'transaction_id')::uuid;
    v_sup_id := (v_conf ->> 'supplier_id')::uuid;

    -- Atualizar transação apenas se pertence ao usuário e supplier_id é NULL
    UPDATE transactions
    SET supplier_id = v_sup_id,
        updated_at = now()
    WHERE id = v_tx_id
      AND user_id = p_user_id
      AND supplier_id IS NULL;

    IF FOUND THEN
      v_updated := v_updated + 1;
    END IF;
  END LOOP;

  -- Log de auditoria
  INSERT INTO audit_logs (user_id, action, entity_type, details)
  VALUES (
    p_user_id,
    'retroactive_supplier_association_atomic',
    'transaction',
    jsonb_build_object(
      'total_requested', v_total,
      'total_updated', v_updated,
      'confirmations', p_confirmations
    )
  );

  RETURN jsonb_build_object(
    'updated', v_updated,
    'total_requested', v_total
  );
END;
$$;

GRANT EXECUTE ON FUNCTION confirm_supplier_associations(uuid, jsonb) TO authenticated;
