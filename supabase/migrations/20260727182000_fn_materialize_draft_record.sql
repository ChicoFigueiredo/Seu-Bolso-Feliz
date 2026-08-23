-- P0-4: materialização atômica e idempotente de um draft aprovado.
--
-- Antes, materializar eram dois round-trips independentes do cliente:
--   1) INSERT em transactions
--   2) UPDATE draft_records SET status='posted', posted_record_id=...
-- Se (2) falhasse, restava uma transação órfã no ledger e um draft ainda
-- 'approved' que, ao ser reprocessado, lançava tudo de novo. PostgREST não
-- expõe transação multi-statement, então isso não era corrigível no cliente.
--
-- Divisão de responsabilidade escolhida: o TypeScript valida (Zod, em
-- @sbf/contracts, fonte única compartilhada com o gerador) e produz o payload;
-- esta função faz travar → conferir → inserir → marcar → auditar, tudo em uma
-- transação. Ela NÃO revalida regras de negócio — duplicar o contrato Zod em
-- plpgsql para 4 tipos de draft garantiria divergência. Ela confere apenas os
-- invariantes que o SQL detém de forma barata: posse, status, não-lançado e
-- unicidade.

CREATE OR REPLACE FUNCTION fn_materialize_draft_record(
  p_user_id        uuid,
  p_draft_id       uuid,
  p_target_table   text,
  p_insert_payload jsonb,
  p_actor          text DEFAULT 'web'
) RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  v_draft  draft_records%ROWTYPE;
  v_new_id uuid;
BEGIN
  -- auth.uid() é NULL sob service_role, então p_user_id precisa ser
  -- parâmetro — o que o tornaria forjável por qualquer 'authenticated'.
  -- Com JWT de usuário, fica preso ao chamador; com service_role (que já é
  -- onipotente), qualquer usuário é permitido.
  IF auth.uid() IS NOT NULL AND auth.uid() <> p_user_id THEN
    RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';
  END IF;

  IF p_target_table NOT IN
     ('transactions', 'recurring_templates', 'liabilities', 'consumption_metrics') THEN
    RAISE EXCEPTION 'target table not allowed: %', p_target_table
      USING ERRCODE = 'invalid_parameter_value';
  END IF;

  -- FOR UPDATE serializa aprovações concorrentes do mesmo draft.
  SELECT * INTO v_draft
    FROM draft_records
   WHERE id = p_draft_id AND user_id = p_user_id
     FOR UPDATE;

  IF NOT FOUND THEN
    RAISE EXCEPTION 'draft not found or not owned: %', p_draft_id
      USING ERRCODE = 'no_data_found';
  END IF;

  -- Replay idempotente, verificado DENTRO do lock: a segunda chamada
  -- concorrente chega aqui só depois que a primeira comitou.
  IF v_draft.posted_record_id IS NOT NULL THEN
    RETURN jsonb_build_object(
      'status', 'already_posted',
      'posted_record_id', v_draft.posted_record_id,
      'posted_record_type', v_draft.posted_record_type
    );
  END IF;

  -- Governança: pending_review nunca vira dinheiro. A camada TypeScript
  -- também filtra, mas é esta verificação que vale, porque nenhum cliente
  -- consegue contorná-la.
  IF v_draft.status NOT IN ('approved', 'corrected') THEN
    RAISE EXCEPTION 'draft % em status % não pode ser materializado', p_draft_id, v_draft.status
      USING ERRCODE = 'check_violation';
  END IF;

  -- Colunas explícitas por ramo. jsonb_populate_record seria mais curto, mas
  -- preenche TODAS as colunas: chaves ausentes virariam NULL explícito e
  -- sobrescreveriam DEFAULTs, incluindo id, created_at e
  -- origin_type NOT NULL DEFAULT 'manual'.
  IF p_target_table = 'transactions' THEN
    INSERT INTO transactions (
      user_id, financial_product_id, type, amount, description,
      event_date, competence_date, category_id, priority, notes,
      origin_type, is_confirmed, source_document_id, metadata
    ) VALUES (
      p_user_id,
      (p_insert_payload->>'financial_product_id')::uuid,
      p_insert_payload->>'type',
      (p_insert_payload->>'amount')::numeric,
      p_insert_payload->>'description',
      (p_insert_payload->>'event_date')::date,
      NULLIF(p_insert_payload->>'competence_date', '')::date,
      NULLIF(p_insert_payload->>'category_id', '')::uuid,
      NULLIF(p_insert_payload->>'priority', ''),
      p_insert_payload->>'notes',
      'import',
      COALESCE((p_insert_payload->>'is_confirmed')::boolean, true),
      v_draft.source_document_id,
      COALESCE(p_insert_payload->'metadata', '{}'::jsonb)
    )
    RETURNING id INTO v_new_id;

  ELSIF p_target_table = 'recurring_templates' THEN
    INSERT INTO recurring_templates (
      user_id, financial_product_id, name, type, amount, is_variable_amount,
      frequency, day_of_month, category_id, priority, starts_at, ends_at, notes
    ) VALUES (
      p_user_id,
      NULLIF(p_insert_payload->>'financial_product_id', '')::uuid,
      p_insert_payload->>'name',
      p_insert_payload->>'type',
      NULLIF(p_insert_payload->>'amount', '')::numeric,
      COALESCE((p_insert_payload->>'is_variable_amount')::boolean, false),
      p_insert_payload->>'frequency',
      NULLIF(p_insert_payload->>'day_of_month', '')::int,
      NULLIF(p_insert_payload->>'category_id', '')::uuid,
      NULLIF(p_insert_payload->>'priority', ''),
      NULLIF(p_insert_payload->>'starts_at', '')::date,
      NULLIF(p_insert_payload->>'ends_at', '')::date,
      p_insert_payload->>'notes'
    )
    RETURNING id INTO v_new_id;

  ELSIF p_target_table = 'liabilities' THEN
    INSERT INTO liabilities (
      user_id, financial_product_id, name, type, original_amount,
      outstanding_balance, interest_rate, rate_type, amortization_system,
      total_installments, start_date, end_date
    ) VALUES (
      p_user_id,
      (p_insert_payload->>'financial_product_id')::uuid,
      p_insert_payload->>'name',
      p_insert_payload->>'type',
      (p_insert_payload->>'original_amount')::numeric,
      (p_insert_payload->>'outstanding_balance')::numeric,
      NULLIF(p_insert_payload->>'interest_rate', '')::numeric,
      NULLIF(p_insert_payload->>'rate_type', ''),
      NULLIF(p_insert_payload->>'amortization_system', ''),
      NULLIF(p_insert_payload->>'total_installments', '')::int,
      NULLIF(p_insert_payload->>'start_date', '')::date,
      NULLIF(p_insert_payload->>'end_date', '')::date
    )
    RETURNING id INTO v_new_id;

  ELSIF p_target_table = 'consumption_metrics' THEN
    INSERT INTO consumption_metrics (
      user_id, supplier_id, supplier_contract_id, reference_period_start,
      reference_period_end, metric_name, metric_unit, quantity, unit_price,
      subtotal, metadata
    ) VALUES (
      p_user_id,
      (p_insert_payload->>'supplier_id')::uuid,
      NULLIF(p_insert_payload->>'supplier_contract_id', '')::uuid,
      (p_insert_payload->>'reference_period_start')::date,
      (p_insert_payload->>'reference_period_end')::date,
      NULLIF(p_insert_payload->>'metric_name', ''),
      NULLIF(p_insert_payload->>'metric_unit', ''),
      NULLIF(p_insert_payload->>'quantity', '')::numeric,
      NULLIF(p_insert_payload->>'unit_price', '')::numeric,
      NULLIF(p_insert_payload->>'subtotal', '')::numeric,
      COALESCE(p_insert_payload->'metadata', '{}'::jsonb)
    )
    RETURNING id INTO v_new_id;
  END IF;

  UPDATE draft_records
     SET status                = 'posted',
         posted_record_id      = v_new_id,
         posted_record_type    = p_target_table,
         posted_at             = now(),
         materialization_error = NULL,
         updated_at            = now()
   WHERE id = p_draft_id;

  -- A auditoria faz parte da MESMA transação, então não pode mais ser
  -- silenciosamente perdida. A versão anterior gravava em colunas
  -- inexistentes (target_id, details) e engolia o erro com .then(() => {}),
  -- de modo que nunca houve trilha de materialização.
  INSERT INTO audit_logs (user_id, action, entity_type, entity_id, new_values)
  VALUES (
    p_user_id,
    'draft_materialized',
    p_target_table,
    v_new_id,
    jsonb_build_object(
      'draft_id', p_draft_id,
      'draft_type', v_draft.draft_type,
      'actor', p_actor,
      'materialization_key', v_draft.materialization_key
    )
  );

  RETURN jsonb_build_object(
    'status', 'posted',
    'posted_record_id', v_new_id,
    'posted_record_type', p_target_table
  );
END;
$$;

REVOKE ALL ON FUNCTION fn_materialize_draft_record(uuid, uuid, text, jsonb, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fn_materialize_draft_record(uuid, uuid, text, jsonb, text)
  TO authenticated, service_role;

COMMENT ON FUNCTION fn_materialize_draft_record(uuid, uuid, text, jsonb, text) IS
  'Materializa um draft aprovado em uma transação única e idempotente. Recusa pending_review.';

-- rollback:
--   DROP FUNCTION IF EXISTS fn_materialize_draft_record(uuid, uuid, text, jsonb, text);
