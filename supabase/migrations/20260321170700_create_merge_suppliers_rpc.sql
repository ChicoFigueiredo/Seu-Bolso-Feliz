-- Migration: Função atômica de merge de fornecedores (ADR-003 §2.5)
-- Chamada pela Edge Function merge-suppliers via supabase.rpc()

CREATE OR REPLACE FUNCTION merge_suppliers(
  p_user_id uuid,
  p_source_id uuid,
  p_target_id uuid,
  p_source_name text,
  p_target_name text
) RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
  v_tx_count int;
  v_template_count int;
  v_stmt_count int;
  v_doc_count int;
  v_liability_count int;
  v_alias_count int;
  v_contract_count int;
  v_metric_count int;
  v_tag_count int;
BEGIN
  -- Etapa 1: Migrar transações
  UPDATE transactions
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_tx_count = ROW_COUNT;

  -- Etapa 1b: Migrar templates recorrentes
  UPDATE recurring_templates
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_template_count = ROW_COUNT;

  -- Etapa 1c: Migrar itens de fatura
  UPDATE statement_items
    SET supplier_id = p_target_id
    WHERE supplier_id = p_source_id
      AND statement_cycle_id IN (
        SELECT sc.id FROM statement_cycles sc
        JOIN cards c ON c.id = sc.card_id
        JOIN financial_products fp ON fp.id = c.financial_product_id
        WHERE fp.user_id = p_user_id
      );
  GET DIAGNOSTICS v_stmt_count = ROW_COUNT;

  -- Etapa 1d: Migrar documentos
  UPDATE documents
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_doc_count = ROW_COUNT;

  -- Etapa 1e: Migrar dívidas
  UPDATE liabilities
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_liability_count = ROW_COUNT;

  -- Etapa 2: Migrar aliases do source para target
  UPDATE supplier_aliases
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_alias_count = ROW_COUNT;

  -- Etapa 3: Criar alias com nome do fornecedor absorvido
  INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type, valid_until)
  VALUES (p_user_id, p_target_id, p_source_name, 'former_name', now());

  -- Etapa 4: Migrar contratos
  UPDATE supplier_contracts
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_contract_count = ROW_COUNT;

  -- Etapa 5: Migrar métricas de consumo
  UPDATE consumption_metrics
    SET supplier_id = p_target_id, updated_at = now()
    WHERE supplier_id = p_source_id AND user_id = p_user_id;
  GET DIAGNOSTICS v_metric_count = ROW_COUNT;

  -- Etapa 6: Migrar tags sem duplicatas
  INSERT INTO supplier_tags (supplier_id, tag_id)
  SELECT p_target_id, st.tag_id
  FROM supplier_tags st
  WHERE st.supplier_id = p_source_id
  ON CONFLICT DO NOTHING;
  GET DIAGNOSTICS v_tag_count = ROW_COUNT;

  -- Remover tags do source
  DELETE FROM supplier_tags WHERE supplier_id = p_source_id;

  -- Etapa 7: Desativar fornecedor absorvido
  UPDATE suppliers
    SET is_active = false,
        notes = COALESCE(notes, '') || ' [Merged into ' || p_target_name || ' at ' || now()::text || ']',
        updated_at = now()
    WHERE id = p_source_id AND user_id = p_user_id;

  -- Etapa 8: Registrar no audit_log
  INSERT INTO audit_logs (user_id, action, entity_type, entity_id, details)
  VALUES (
    p_user_id,
    'supplier_merge',
    'supplier',
    p_target_id,
    jsonb_build_object(
      'absorbed_id', p_source_id,
      'absorbed_name', p_source_name,
      'target_name', p_target_name,
      'migrated_counts', jsonb_build_object(
        'transactions', v_tx_count,
        'recurring_templates', v_template_count,
        'statement_items', v_stmt_count,
        'documents', v_doc_count,
        'liabilities', v_liability_count,
        'aliases', v_alias_count,
        'contracts', v_contract_count,
        'consumption_metrics', v_metric_count,
        'tags', v_tag_count
      )
    )
  );

  RETURN jsonb_build_object(
    'transactions', v_tx_count,
    'recurring_templates', v_template_count,
    'statement_items', v_stmt_count,
    'documents', v_doc_count,
    'liabilities', v_liability_count,
    'aliases', v_alias_count,
    'contracts', v_contract_count,
    'consumption_metrics', v_metric_count,
    'tags', v_tag_count
  );
END;
$$;
