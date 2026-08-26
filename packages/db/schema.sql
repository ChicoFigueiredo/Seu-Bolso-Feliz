--
-- PostgreSQL database dump
--


-- Dumped from database version 17.6
-- Dumped by pg_dump version 17.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: public; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA public;


--
-- Name: SCHEMA public; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON SCHEMA public IS 'standard public schema';


--
-- Name: pg_trgm; Type: EXTENSION; Schema: public; Owner: -
--
-- NOTA: `pg_dump --schema=public` não inclui CREATE EXTENSION (extensões não
-- pertencem ao schema public mesmo quando seus operadores são registrados
-- nele) -- reinstalada manualmente aqui porque dois índices GIN
-- (idx_supplier_aliases_trgm, idx_suppliers_name_trgm) dependem do operator
-- class public.gin_trgm_ops. Confirmado disponível no Neon via
-- pg_available_extensions antes de adicionar esta linha.
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: auto_alias_on_rename(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.auto_alias_on_rename() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  IF OLD.name IS DISTINCT FROM NEW.name THEN
    INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type, valid_until)
    VALUES (NEW.user_id, NEW.id, OLD.name, 'former_name', NOW()::date);
  END IF;
  RETURN NEW;
END;
$$;


--
-- Name: check_alias_uniqueness(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_alias_uniqueness() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM supplier_aliases sa
    WHERE sa.user_id = NEW.user_id
      AND sa.id != COALESCE(NEW.id, '00000000-0000-0000-0000-000000000000'::uuid)
      AND LOWER(sa.alias_name) = LOWER(NEW.alias_name)
      AND sa.is_active = true
      AND (
        -- Períodos se sobrepõem
        (sa.valid_from IS NULL OR NEW.valid_until IS NULL OR sa.valid_from <= NEW.valid_until)
        AND
        (sa.valid_until IS NULL OR NEW.valid_from IS NULL OR sa.valid_until >= NEW.valid_from)
      )
  ) THEN
    RAISE EXCEPTION 'Alias "%" já existe para outro fornecedor no período informado', NEW.alias_name;
  END IF;
  RETURN NEW;
END;
$$;


--
-- Name: check_pattern_auto_deactivation(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.check_pattern_auto_deactivation() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
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


--
-- Name: confirm_supplier_associations(uuid, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.confirm_supplier_associations(p_user_id uuid, p_confirmations jsonb) RETURNS jsonb
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
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


--
-- Name: decrypt_secret(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.decrypt_secret(ciphertext text, p_version integer DEFAULT NULL::integer) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'private', 'extensions'
    AS $$
DECLARE k text;
BEGIN
  k := private.get_crypto_key(p_version);
  IF k IS NULL OR k = '' THEN
    RAISE EXCEPTION 'nenhuma chave de criptografia disponível' USING ERRCODE = 'configuration_error';
  END IF;
  RETURN pgp_sym_decrypt(decode(ciphertext, 'base64'), k);
EXCEPTION
  WHEN OTHERS THEN
    -- Mensagem deliberadamente sem o valor nem a chave.
    RAISE EXCEPTION 'falha ao decriptar segredo: chave incorreta ou dado corrompido'
      USING ERRCODE = 'data_exception';
END;
$$;


--
-- Name: encrypt_secret(text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.encrypt_secret(plaintext text, p_version integer DEFAULT NULL::integer) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'private', 'extensions'
    AS $$
DECLARE k text;
BEGIN
  k := private.get_crypto_key(p_version);
  IF k IS NULL OR k = '' THEN
    RAISE EXCEPTION 'nenhuma chave de criptografia disponível' USING ERRCODE = 'configuration_error';
  END IF;
  RETURN encode(pgp_sym_encrypt(plaintext, k), 'base64');
END;
$$;


--
-- Name: fn_draft_records_set_materialization_key(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_draft_records_set_materialization_key() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  IF NEW.materialization_key IS NULL THEN
    NEW.materialization_key := 'draft:' || NEW.id::text;
  END IF;
  RETURN NEW;
END;
$$;


--
-- Name: fn_get_secrets(uuid, text, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_get_secrets(p_user_id uuid, p_secret_type text, p_limit integer DEFAULT 20) RETURNS TABLE(secret_id uuid, value text, entity_type text, entity_id uuid, contract_identifier text, label text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public', 'private'
    AS $$
BEGIN
  -- Auditoria sem o valor. Nunca registrar o segredo em lugar algum.
  INSERT INTO audit_logs (user_id, action, entity_type, new_values)
  VALUES (p_user_id, 'secrets_accessed', 'user_secrets',
          jsonb_build_object('secret_type', p_secret_type, 'limit', p_limit));

  RETURN QUERY
  SELECT s.id,
         decrypt_secret(s.encrypted_value, s.encryption_version),
         s.entity_type,
         s.entity_id,
         s.contract_identifier,
         s.label
    FROM user_secrets s
   WHERE s.user_id = p_user_id
     AND s.secret_type = p_secret_type
   -- Mais usados primeiro: a próxima tentativa tende a acertar antes.
   ORDER BY s.last_used_at DESC NULLS LAST, s.success_count DESC
   LIMIT COALESCE(p_limit, 20);
END;
$$;


--
-- Name: fn_mark_secret_used(uuid, uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_mark_secret_used(p_user_id uuid, p_secret_id uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$
BEGIN
  UPDATE user_secrets
     SET last_used_at = now(), success_count = success_count + 1
   WHERE id = p_secret_id AND user_id = p_user_id;
END;
$$;


--
-- Name: fn_materialize_draft_record(uuid, uuid, text, jsonb, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_materialize_draft_record(p_user_id uuid, p_draft_id uuid, p_target_table text, p_insert_payload jsonb, p_actor text DEFAULT 'web'::text) RETURNS jsonb
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$
DECLARE
  v_draft  draft_records%ROWTYPE;
  v_new_id uuid;
BEGIN
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


--
-- Name: FUNCTION fn_materialize_draft_record(p_user_id uuid, p_draft_id uuid, p_target_table text, p_insert_payload jsonb, p_actor text); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.fn_materialize_draft_record(p_user_id uuid, p_draft_id uuid, p_target_table text, p_insert_payload jsonb, p_actor text) IS 'Materializa um draft aprovado em uma transação única e idempotente. Recusa pending_review.';


--
-- Name: fn_reconciliation_progress(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_reconciliation_progress(p_batch_id uuid) RETURNS TABLE(total_count bigint, reconciled_count bigint, progress_pct numeric)
    LANGUAGE sql STABLE SECURITY DEFINER
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


--
-- Name: FUNCTION fn_reconciliation_progress(p_batch_id uuid); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.fn_reconciliation_progress(p_batch_id uuid) IS 'Retorna progresso de reconciliação de um draft_batch: total_count, reconciled_count, progress_pct (0–100). Usado na Tela 14 (variant=statement) para exibir barra de progresso.';


--
-- Name: fn_set_updated_at(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_set_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;


--
-- Name: fn_upsert_financial_obligation(uuid, jsonb, jsonb, jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_upsert_financial_obligation(p_user_id uuid, p_payload jsonb, p_keys jsonb, p_evidence jsonb) RETURNS jsonb
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$
DECLARE
  v_obligation_id uuid;
  v_is_new        boolean := false;
  v_matched_by    text;
  v_key           jsonb;
  v_amount        numeric;
  v_found_amount  numeric;
  v_evidence_role text;
  v_count         int;
BEGIN
  v_amount := NULLIF(p_payload->>'amount', '')::numeric;

  -- Procura por chaves, das mais fortes para as mais fracas.
  FOR v_key IN
    SELECT * FROM jsonb_array_elements(p_keys)
    ORDER BY CASE jsonb_array_elements->>'strength'
               WHEN 'strong' THEN 1 WHEN 'medium' THEN 2 ELSE 3 END
  LOOP
    SELECT k.obligation_id, o.amount
      INTO v_obligation_id, v_found_amount
      FROM financial_obligation_identity_keys k
      JOIN financial_obligations o ON o.id = k.obligation_id
     WHERE k.user_id = p_user_id
       AND k.key = v_key->>'key'
     LIMIT 1;

    IF v_obligation_id IS NOT NULL THEN
      -- Uma chave fraca identifica apenas "alguma conta deste fornecedor neste
      -- mês". Só aceitamos o casamento se o valor for desconhecido de um dos
      -- lados ou coincidir; caso contrário, duas contas distintas do mesmo
      -- fornecedor no mesmo mês seriam fundidas numa só.
      IF (v_key->>'strength') = 'weak'
         AND v_amount IS NOT NULL
         AND v_found_amount IS NOT NULL
         AND v_amount <> v_found_amount THEN
        v_obligation_id := NULL;
        CONTINUE;
      END IF;

      v_matched_by := v_key->>'kind';
      EXIT;
    END IF;
  END LOOP;

  IF v_obligation_id IS NULL THEN
    INSERT INTO financial_obligations (
      user_id, obligation_type, status, supplier_id, supplier_name_raw,
      financial_product_id, amount, due_date, competence_date,
      cycle_start_date, cycle_end_date, document_number,
      barcode_digitable_line, financial_identity_key, confidence_score, metadata
    ) VALUES (
      p_user_id,
      COALESCE(p_payload->>'obligation_type', 'unknown'),
      'open',
      NULLIF(p_payload->>'supplier_id', '')::uuid,
      p_payload->>'supplier_name_raw',
      NULLIF(p_payload->>'financial_product_id', '')::uuid,
      v_amount,
      NULLIF(p_payload->>'due_date', '')::date,
      NULLIF(p_payload->>'competence_date', '')::date,
      NULLIF(p_payload->>'cycle_start_date', '')::date,
      NULLIF(p_payload->>'cycle_end_date', '')::date,
      NULLIF(p_payload->>'document_number', ''),
      NULLIF(p_payload->>'barcode_digitable_line', ''),
      NULLIF(p_payload->>'financial_identity_key', ''),
      NULLIF(p_payload->>'confidence_score', '')::numeric,
      COALESCE(p_payload->'metadata', '{}'::jsonb)
    )
    RETURNING id INTO v_obligation_id;

    v_is_new := true;
    v_matched_by := 'new';
  ELSE
    -- Preenche apenas lacunas. Um documento posterior pode acrescentar o que
    -- faltava, mas nunca apagar um dado já conhecido com um nulo.
    UPDATE financial_obligations o
       SET supplier_id           = COALESCE(o.supplier_id, NULLIF(p_payload->>'supplier_id', '')::uuid),
           supplier_name_raw     = COALESCE(o.supplier_name_raw, p_payload->>'supplier_name_raw'),
           financial_product_id  = COALESCE(o.financial_product_id, NULLIF(p_payload->>'financial_product_id', '')::uuid),
           amount                = COALESCE(o.amount, v_amount),
           due_date              = COALESCE(o.due_date, NULLIF(p_payload->>'due_date', '')::date),
           competence_date       = COALESCE(o.competence_date, NULLIF(p_payload->>'competence_date', '')::date),
           cycle_start_date      = COALESCE(o.cycle_start_date, NULLIF(p_payload->>'cycle_start_date', '')::date),
           cycle_end_date        = COALESCE(o.cycle_end_date, NULLIF(p_payload->>'cycle_end_date', '')::date),
           document_number       = COALESCE(o.document_number, NULLIF(p_payload->>'document_number', '')),
           barcode_digitable_line= COALESCE(o.barcode_digitable_line, NULLIF(p_payload->>'barcode_digitable_line', '')),
           financial_identity_key= COALESCE(o.financial_identity_key, NULLIF(p_payload->>'financial_identity_key', ''))
     WHERE o.id = v_obligation_id;
  END IF;

  -- Grava todos os aliases computáveis. ON CONFLICT DO NOTHING porque uma
  -- chave já reivindicada por outra obrigação não deve ser roubada.
  INSERT INTO financial_obligation_identity_keys (user_id, obligation_id, key, key_kind, strength)
  SELECT p_user_id, v_obligation_id, e->>'key', e->>'kind', e->>'strength'
    FROM jsonb_array_elements(p_keys) e
  ON CONFLICT (user_id, key) DO NOTHING;

  -- Anexa a evidência. A primeira vira 'primary'; as demais, 'supporting'.
  v_evidence_role := CASE WHEN v_is_new THEN 'primary' ELSE 'supporting' END;

  INSERT INTO financial_obligation_evidences (
    user_id, obligation_id, source_document_id, evidence_role, confidence_score, reasons
  ) VALUES (
    p_user_id,
    v_obligation_id,
    (p_evidence->>'source_document_id')::uuid,
    v_evidence_role,
    NULLIF(p_evidence->>'confidence_score', '')::numeric,
    COALESCE(p_evidence->'reasons', '[]'::jsonb)
  )
  ON CONFLICT (user_id, obligation_id, source_document_id) DO NOTHING;

  SELECT count(*) INTO v_count
    FROM financial_obligation_evidences
   WHERE obligation_id = v_obligation_id;

  RETURN jsonb_build_object(
    'obligation_id', v_obligation_id,
    'is_new', v_is_new,
    'matched_by', v_matched_by,
    'evidence_count', v_count
  );
END;
$$;


--
-- Name: FUNCTION fn_upsert_financial_obligation(p_user_id uuid, p_payload jsonb, p_keys jsonb, p_evidence jsonb); Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON FUNCTION public.fn_upsert_financial_obligation(p_user_id uuid, p_payload jsonb, p_keys jsonb, p_evidence jsonb) IS 'Encontra ou cria a obrigação canônica de uma evidência, gravando aliases de identidade e o vínculo da evidência atomicamente.';


--
-- Name: fn_validate_splits_sum(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.fn_validate_splits_sum() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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


--
-- Name: merge_suppliers(uuid, uuid, uuid, text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.merge_suppliers(p_user_id uuid, p_source_id uuid, p_target_id uuid, p_source_name text, p_target_name text) RETURNS jsonb
    LANGUAGE plpgsql SECURITY DEFINER
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


--
-- Name: prevent_audit_log_mutation(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.prevent_audit_log_mutation() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    AS $$
BEGIN
  RAISE EXCEPTION 'audit_logs are immutable — % operation not permitted on this table', TG_OP
    USING ERRCODE = 'restrict_violation';
END;
$$;


--
-- Name: refresh_mv_supplier_spending(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.refresh_mv_supplier_spending() RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$
BEGIN
  REFRESH MATERIALIZED VIEW CONCURRENTLY mv_supplier_spending;
END;
$$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: pattern_feedback; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pattern_feedback (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    pattern_id uuid NOT NULL,
    source_document_id uuid,
    feedback_type text NOT NULL,
    corrections jsonb DEFAULT '{}'::jsonb NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT pattern_feedback_feedback_type_check CHECK ((feedback_type = ANY (ARRAY['correct'::text, 'incorrect'::text, 'partial'::text, 'improved'::text])))
);


--
-- Name: update_financial_obligations_updated_at(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.update_financial_obligations_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;


--
-- Name: update_ingestion_checkpoints_updated_at(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.update_ingestion_checkpoints_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$;


--
-- Name: update_updated_at(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.update_updated_at() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$;


--
-- Name: ai_chat_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ai_chat_messages (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    session_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text NOT NULL,
    content text,
    tool_calls jsonb,
    tool_call_id text,
    tool_name text,
    tokens_used integer,
    latency_ms integer,
    model text,
    created_at timestamp with time zone DEFAULT now(),
    CONSTRAINT ai_chat_messages_role_check CHECK ((role = ANY (ARRAY['user'::text, 'assistant'::text, 'system'::text, 'tool'::text])))
);


--
-- Name: ai_chat_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ai_chat_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    title text,
    context_type text,
    context_id uuid,
    model text DEFAULT 'gpt-4o'::text NOT NULL,
    total_tokens_used integer DEFAULT 0,
    message_count integer DEFAULT 0,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now()
);


--
-- Name: audit_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.audit_logs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    action text NOT NULL,
    entity_type text,
    entity_id uuid,
    old_values jsonb,
    new_values jsonb,
    ip_address inet,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: TABLE audit_logs; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.audit_logs IS 'Log de auditoria imutável. INSERT permitido via RLS. UPDATE e DELETE bloqueados por trigger.';


--
-- Name: cards; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.cards (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    financial_product_id uuid NOT NULL,
    last_four_digits text,
    card_brand text,
    is_primary boolean DEFAULT true NOT NULL,
    holder_name text,
    credit_limit numeric(15,2),
    closing_day integer,
    due_day integer,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT cards_closing_day_check CHECK (((closing_day >= 1) AND (closing_day <= 31))),
    CONSTRAINT cards_due_day_check CHECK (((due_day >= 1) AND (due_day <= 31)))
);


--
-- Name: categories; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.categories (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    parent_id uuid,
    icon text,
    color text,
    display_order integer,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: consumption_metrics; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.consumption_metrics (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    supplier_id uuid NOT NULL,
    supplier_contract_id uuid,
    transaction_id uuid,
    document_id uuid,
    reference_period_start date NOT NULL,
    reference_period_end date NOT NULL,
    metric_name text,
    metric_unit text,
    quantity numeric(15,4),
    unit_price numeric(15,6),
    subtotal numeric(15,2),
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chk_metric_or_attribute CHECK ((((quantity IS NOT NULL) AND (metric_name IS NOT NULL) AND (metric_unit IS NOT NULL)) OR ((quantity IS NULL) AND (metadata IS NOT NULL) AND ((metadata ->> 'type'::text) = 'attribute'::text)))),
    CONSTRAINT chk_reference_period CHECK ((reference_period_end >= reference_period_start))
);


--
-- Name: document_fingerprints; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.document_fingerprints (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    source_document_id uuid NOT NULL,
    user_id uuid NOT NULL,
    content_hash text NOT NULL,
    canonical_fingerprint text,
    hash_algorithm text DEFAULT 'sha256'::text,
    created_at timestamp with time zone DEFAULT now()
);


--
-- Name: document_patterns; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.document_patterns (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    supplier_id uuid,
    document_type text NOT NULL,
    institution_id uuid,
    extraction_rules jsonb DEFAULT '{}'::jsonb NOT NULL,
    field_mappings jsonb DEFAULT '{}'::jsonb NOT NULL,
    sample_fingerprints text[] DEFAULT '{}'::text[] NOT NULL,
    confidence_threshold numeric(3,2) DEFAULT 0.80 NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    feedback_count integer DEFAULT 0 NOT NULL,
    success_count integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT document_patterns_confidence_threshold_check CHECK (((confidence_threshold >= 0.00) AND (confidence_threshold <= 1.00))),
    CONSTRAINT document_patterns_feedback_count_check CHECK ((feedback_count >= 0)),
    CONSTRAINT document_patterns_success_count_check CHECK ((success_count >= 0))
);


--
-- Name: document_splits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.document_splits (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    source_document_id uuid NOT NULL,
    user_id uuid NOT NULL,
    category_id uuid,
    tags uuid[] DEFAULT '{}'::uuid[] NOT NULL,
    amount numeric(14,2) NOT NULL,
    description text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT document_splits_amount_positive CHECK ((amount > (0)::numeric))
);


--
-- Name: TABLE document_splits; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.document_splits IS 'Rateio de documento entre categorias/tags. Conceito DocumentSplit vive em packages/domain. SUM(amount) validado server-side via trg_validate_splits_sum.';


--
-- Name: document_transactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.document_transactions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    source_document_id uuid NOT NULL,
    transaction_id uuid NOT NULL,
    user_id uuid NOT NULL,
    link_type text NOT NULL,
    confidence numeric(4,3),
    created_by text DEFAULT 'user'::text NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT document_transactions_confidence_range CHECK (((confidence IS NULL) OR ((confidence >= (0)::numeric) AND (confidence <= (1)::numeric)))),
    CONSTRAINT document_transactions_created_by_check CHECK ((created_by = ANY (ARRAY['user'::text, 'ai'::text, 'pattern'::text]))),
    CONSTRAINT document_transactions_link_type_check CHECK ((link_type = ANY (ARRAY['payment'::text, 'refund'::text, 'installment'::text, 'support'::text])))
);


--
-- Name: TABLE document_transactions; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.document_transactions IS 'Vínculo entre source_documents e transactions (ledger). Conceito DocumentTransactionLink vive em packages/domain. link_type: payment | refund | installment | support. created_by: user | ai | pattern.';


--
-- Name: documents_legacy; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.documents_legacy (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    description text,
    file_path text NOT NULL,
    file_type text,
    file_size bigint,
    document_type text,
    entity_type text,
    entity_id uuid,
    version integer DEFAULT 1 NOT NULL,
    is_password_protected boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    supplier_id uuid,
    CONSTRAINT documents_document_type_check CHECK (((document_type IS NULL) OR (document_type = ANY (ARRAY['receipt'::text, 'invoice'::text, 'statement'::text, 'contract'::text, 'proof'::text, 'other'::text]))))
);


--
-- Name: TABLE documents_legacy; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON TABLE public.documents_legacy IS 'Tabela legada renomeada em 2026-04-21 (S2-002). Substituída por source_documents. DROP agendado para após 30 dias de validação em staging pelo CEO. Não dropar sem confirmação explícita.';


--
-- Name: draft_batches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.draft_batches (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    run_id uuid,
    name text,
    status text DEFAULT 'open'::text NOT NULL,
    total_drafts integer DEFAULT 0,
    approved_count integer DEFAULT 0,
    rejected_count integer DEFAULT 0,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    source_document_id uuid,
    CONSTRAINT draft_batches_status_check CHECK ((status = ANY (ARRAY['open'::text, 'reviewing'::text, 'approved'::text, 'partial'::text, 'rejected'::text])))
);


--
-- Name: draft_records; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.draft_records (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    batch_id uuid,
    user_id uuid NOT NULL,
    source_document_id uuid,
    extraction_result_id uuid,
    draft_type text NOT NULL,
    status text DEFAULT 'pending_review'::text NOT NULL,
    draft_data jsonb NOT NULL,
    corrections jsonb,
    confidence_score numeric(3,2),
    approved_at timestamp with time zone,
    approved_by uuid,
    posted_record_id uuid,
    posted_record_type text,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    reconciliation_status text DEFAULT 'not_checked'::text NOT NULL,
    reconciled_transaction_id uuid,
    reconciled_template_id uuid,
    reconciliation_candidates jsonb DEFAULT '[]'::jsonb,
    reconciled_at timestamp with time zone,
    draft_schema_version smallint DEFAULT 0 NOT NULL,
    draft_data_legacy jsonb,
    materialization_key text NOT NULL,
    posted_at timestamp with time zone,
    materialization_error jsonb,
    obligation_id uuid,
    external_ref text,
    CONSTRAINT draft_records_draft_type_check CHECK ((draft_type = ANY (ARRAY['transaction'::text, 'recurring_template'::text, 'liability'::text, 'consumption_metric'::text]))),
    CONSTRAINT draft_records_reconciliation_status_check CHECK ((reconciliation_status = ANY (ARRAY['not_checked'::text, 'no_match'::text, 'match_exact'::text, 'match_fuzzy'::text, 'match_duplicate'::text, 'match_recurrence'::text, 'confirmed_new'::text, 'confirmed_duplicate'::text]))),
    CONSTRAINT draft_records_status_check CHECK ((status = ANY (ARRAY['pending_review'::text, 'approved'::text, 'posted'::text, 'rejected'::text, 'corrected'::text, 'archived'::text])))
);


--
-- Name: COLUMN draft_records.reconciliation_status; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.reconciliation_status IS 'Status da reconciliação: not_checked (padrão), no_match, match_exact, match_fuzzy, match_duplicate, match_recurrence, confirmed_new, confirmed_duplicate';


--
-- Name: COLUMN draft_records.reconciliation_candidates; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.reconciliation_candidates IS 'Array JSON com os candidatos encontrados pela engine de reconciliação (score, tipo, dados resumidos)';


--
-- Name: COLUMN draft_records.reconciled_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.reconciled_at IS 'Timestamp de quando a reconciliação foi executada pelo worker';


--
-- Name: COLUMN draft_records.draft_schema_version; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.draft_schema_version IS 'Versão do contrato em draft_data. 0 = formato legado pré-@sbf/contracts; 1 = DraftPayloadV1.';


--
-- Name: COLUMN draft_records.draft_data_legacy; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.draft_data_legacy IS 'Cópia intacta do draft_data v0, preservada pelo backfill para permitir auditoria e reversão.';


--
-- Name: COLUMN draft_records.materialization_key; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.materialization_key IS 'Chave de idempotência do lançamento. Única por usuário.';


--
-- Name: COLUMN draft_records.materialization_error; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.materialization_error IS 'Último erro de materialização, para a tela de revisão explicar a falha sem consultar logs.';


--
-- Name: COLUMN draft_records.obligation_id; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.draft_records.obligation_id IS 'Obrigação financeira canônica que este draft materializa (P0-7).';


--
-- Name: external_account_mappings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.external_account_mappings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    provider_connection_id uuid NOT NULL,
    external_account_id text NOT NULL,
    external_account_name text,
    external_account_type text,
    financial_product_id uuid,
    status text DEFAULT 'pending_mapping'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT external_account_mappings_status_check CHECK ((status = ANY (ARRAY['pending_mapping'::text, 'mapped'::text, 'ignored'::text])))
);


--
-- Name: extraction_results; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.extraction_results (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    parsed_version_id uuid NOT NULL,
    user_id uuid NOT NULL,
    supplier_name_raw text,
    supplier_id uuid,
    supplier_confidence numeric(3,2),
    competence_date date,
    due_date date,
    total_amount numeric(12,2),
    currency text DEFAULT 'BRL'::text,
    breakdown jsonb,
    document_number text,
    contract_identifier text,
    consumption_data jsonb,
    category_suggestion text,
    tags_suggestion text[],
    priority_suggestion text,
    financial_period_suggestion jsonb,
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    ai_enrichment_type text,
    ai_enrichment_at timestamp with time zone,
    confidence_per_field jsonb,
    reasoning text,
    financial_intent text,
    CONSTRAINT extraction_results_ai_enrichment_type_check CHECK (((ai_enrichment_type IS NULL) OR (ai_enrichment_type = ANY (ARRAY['lite'::text, 'full'::text])))),
    CONSTRAINT extraction_results_financial_intent_check CHECK (((financial_intent IS NULL) OR (financial_intent = ANY (ARRAY['transaction'::text, 'recurring_expense'::text, 'metric'::text, 'liability_payment'::text, 'unknown'::text]))))
);


--
-- Name: COLUMN extraction_results.ai_enrichment_type; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.extraction_results.ai_enrichment_type IS 'Nível de IA usado: null = só regex, lite = gpt-4o-mini, full = gpt-4o visão';


--
-- Name: COLUMN extraction_results.ai_enrichment_at; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.extraction_results.ai_enrichment_at IS 'Timestamp do enriquecimento de IA';


--
-- Name: COLUMN extraction_results.confidence_per_field; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.extraction_results.confidence_per_field IS 'Confiança por campo: { "total_amount": 0.95, "due_date": 0.82, ... }';


--
-- Name: COLUMN extraction_results.reasoning; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.extraction_results.reasoning IS 'Reasoning da IA explicando a classificação (curto)';


--
-- Name: COLUMN extraction_results.financial_intent; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.extraction_results.financial_intent IS 'Intenção financeira classificada: transaction | recurring_expense | metric | liability_payment | unknown';


--
-- Name: financial_obligation_evidences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.financial_obligation_evidences (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    obligation_id uuid NOT NULL,
    source_document_id uuid NOT NULL,
    evidence_role text DEFAULT 'supporting'::text NOT NULL,
    confidence_score numeric(3,2),
    reasons jsonb DEFAULT '[]'::jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT financial_obligation_evidences_evidence_role_check CHECK ((evidence_role = ANY (ARRAY['primary'::text, 'supporting'::text, 'duplicate'::text, 'conflicting'::text])))
);


--
-- Name: financial_obligation_identity_keys; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.financial_obligation_identity_keys (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    obligation_id uuid NOT NULL,
    key text NOT NULL,
    key_kind text NOT NULL,
    strength text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT financial_obligation_identity_keys_key_kind_check CHECK ((key_kind = ANY (ARRAY['barcode'::text, 'docnum'::text, 'card_cycle'::text, 'supplier_period_amount'::text, 'supplier_period'::text]))),
    CONSTRAINT financial_obligation_identity_keys_strength_check CHECK ((strength = ANY (ARRAY['strong'::text, 'medium'::text, 'weak'::text])))
);


--
-- Name: financial_obligations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.financial_obligations (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    obligation_type text NOT NULL,
    status text DEFAULT 'open'::text NOT NULL,
    supplier_id uuid,
    supplier_name_raw text,
    financial_product_id uuid,
    amount numeric(15,2),
    due_date date,
    competence_date date,
    cycle_start_date date,
    cycle_end_date date,
    document_number text,
    barcode_digitable_line text,
    financial_identity_key text,
    confidence_score numeric(3,2),
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT financial_obligations_obligation_type_check CHECK ((obligation_type = ANY (ARRAY['bill_to_pay'::text, 'bill_reminder'::text, 'invoice_statement'::text, 'recurring_charge'::text, 'liability_installment'::text, 'unknown'::text]))),
    CONSTRAINT financial_obligations_status_check CHECK ((status = ANY (ARRAY['open'::text, 'pending_review'::text, 'approved'::text, 'paid'::text, 'cancelled'::text, 'duplicate'::text, 'rejected'::text])))
);


--
-- Name: financial_periods; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.financial_periods (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    start_date date NOT NULL,
    end_date date NOT NULL,
    label text,
    is_current boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chk_period_dates CHECK ((end_date >= start_date))
);


--
-- Name: financial_products; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.financial_products (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    institution_id uuid NOT NULL,
    name text NOT NULL,
    type text NOT NULL,
    current_balance numeric(15,2) DEFAULT 0,
    credit_limit numeric(15,2),
    is_active boolean DEFAULT true NOT NULL,
    display_order integer,
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT financial_products_type_check CHECK ((type = ANY (ARRAY['checking_account'::text, 'savings_account'::text, 'credit_card'::text, 'overdraft'::text, 'personal_loan'::text, 'mortgage'::text, 'investment'::text, 'other'::text])))
);


--
-- Name: import_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.import_jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    source_type text NOT NULL,
    file_path text,
    status text DEFAULT 'pending'::text NOT NULL,
    total_rows integer,
    imported_rows integer DEFAULT 0 NOT NULL,
    skipped_rows integer DEFAULT 0 NOT NULL,
    error_rows integer DEFAULT 0 NOT NULL,
    error_details jsonb,
    started_at timestamp with time zone,
    completed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT import_jobs_source_type_check CHECK ((source_type = ANY (ARRAY['csv'::text, 'xlsx'::text, 'manual'::text, 'api'::text]))),
    CONSTRAINT import_jobs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'processing'::text, 'completed'::text, 'failed'::text, 'partial'::text])))
);


--
-- Name: ingestion_checkpoints; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ingestion_checkpoints (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    source_type text NOT NULL,
    scope_key text NOT NULL,
    cursor_kind text NOT NULL,
    cursor_value text,
    last_message_id text,
    messages_seen integer DEFAULT 0 NOT NULL,
    documents_created integer DEFAULT 0 NOT NULL,
    duplicates_skipped integer DEFAULT 0 NOT NULL,
    errors integer DEFAULT 0 NOT NULL,
    window_start date,
    window_end date,
    status text DEFAULT 'running'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT ingestion_checkpoints_cursor_kind_check CHECK ((cursor_kind = ANY (ARRAY['page_token'::text, 'internal_date'::text, 'mtime'::text, 'page_number'::text]))),
    CONSTRAINT ingestion_checkpoints_source_type_check CHECK ((source_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'pluggy'::text]))),
    CONSTRAINT ingestion_checkpoints_status_check CHECK ((status = ANY (ARRAY['running'::text, 'paused'::text, 'completed'::text, 'failed'::text])))
);


--
-- Name: COLUMN ingestion_checkpoints.cursor_kind; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.ingestion_checkpoints.cursor_kind IS 'page_token vale apenas dentro de uma execução; internal_date é o cursor durável de backfill.';


--
-- Name: ingestion_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ingestion_jobs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    run_id uuid NOT NULL,
    user_id uuid NOT NULL,
    source_document_id uuid,
    status text DEFAULT 'discovered'::text NOT NULL,
    error_message text,
    error_details jsonb,
    retry_count integer DEFAULT 0,
    max_retries integer DEFAULT 3,
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    needs_full_ai_review boolean DEFAULT false,
    CONSTRAINT ingestion_jobs_status_check CHECK ((status = ANY (ARRAY['discovered'::text, 'downloaded'::text, 'hashed'::text, 'queued'::text, 'parsing'::text, 'parsed'::text, 'classified'::text, 'reconciled'::text, 'drafted'::text, 'pending_review'::text, 'approved'::text, 'posted'::text, 'failed'::text])))
);


--
-- Name: COLUMN ingestion_jobs.needs_full_ai_review; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.ingestion_jobs.needs_full_ai_review IS 'Sinaliza que o documento precisou de análise full (visão) por não ter sido resolvido pelo modo lite';


--
-- Name: ingestion_logs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ingestion_logs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    run_id uuid,
    job_id uuid,
    level text DEFAULT 'info'::text NOT NULL,
    message text NOT NULL,
    details jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    CONSTRAINT ingestion_logs_level_check CHECK ((level = ANY (ARRAY['debug'::text, 'info'::text, 'warn'::text, 'error'::text])))
);


--
-- Name: ingestion_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.ingestion_runs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    source_type text NOT NULL,
    status text DEFAULT 'running'::text NOT NULL,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    completed_at timestamp with time zone,
    metadata jsonb DEFAULT '{}'::jsonb,
    stats jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT ingestion_runs_source_type_check CHECK ((source_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'manual_upload'::text]))),
    CONSTRAINT ingestion_runs_status_check CHECK ((status = ANY (ARRAY['running'::text, 'completed'::text, 'failed'::text, 'cancelled'::text])))
);


--
-- Name: institutions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.institutions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    type text DEFAULT 'bank'::text NOT NULL,
    icon_url text,
    color text,
    is_active boolean DEFAULT true NOT NULL,
    display_order integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT institutions_type_check CHECK ((type = ANY (ARRAY['bank'::text, 'fintech'::text, 'broker'::text, 'other'::text])))
);


--
-- Name: liabilities; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.liabilities (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    financial_product_id uuid NOT NULL,
    name text NOT NULL,
    type text NOT NULL,
    original_amount numeric(15,2) NOT NULL,
    outstanding_balance numeric(15,2) NOT NULL,
    interest_rate numeric(8,6),
    rate_type text,
    amortization_system text,
    total_installments integer,
    paid_installments integer DEFAULT 0 NOT NULL,
    start_date date,
    end_date date,
    status text DEFAULT 'active'::text NOT NULL,
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    supplier_id uuid,
    CONSTRAINT liabilities_amortization_system_check CHECK (((amortization_system IS NULL) OR (amortization_system = ANY (ARRAY['sac'::text, 'price'::text, 'mixed'::text, 'other'::text, 'none'::text])))),
    CONSTRAINT liabilities_rate_type_check CHECK (((rate_type IS NULL) OR (rate_type = ANY (ARRAY['monthly'::text, 'annual'::text])))),
    CONSTRAINT liabilities_status_check CHECK ((status = ANY (ARRAY['active'::text, 'paid_off'::text, 'renegotiated'::text, 'defaulted'::text]))),
    CONSTRAINT liabilities_type_check CHECK ((type = ANY (ARRAY['personal_loan'::text, 'mortgage'::text, 'overdraft'::text, 'installment_plan'::text, 'other'::text])))
);


--
-- Name: liability_installments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.liability_installments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    liability_id uuid NOT NULL,
    installment_number integer NOT NULL,
    due_date date NOT NULL,
    total_amount numeric(15,2) NOT NULL,
    principal_amount numeric(15,2),
    interest_amount numeric(15,2),
    insurance_amount numeric(15,2),
    fee_amount numeric(15,2),
    paid_amount numeric(15,2) DEFAULT 0 NOT NULL,
    paid_date date,
    status text DEFAULT 'pending'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT liability_installments_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'paid'::text, 'partial'::text, 'overdue'::text, 'waived'::text])))
);


--
-- Name: liability_tags; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.liability_tags (
    liability_id uuid NOT NULL,
    tag_id uuid NOT NULL
);


--
-- Name: suppliers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.suppliers (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    trade_name text,
    legal_name text,
    document_number text,
    type text DEFAULT 'company'::text NOT NULL,
    website text,
    contact_info jsonb,
    notes text,
    institution_id uuid,
    is_active boolean DEFAULT true NOT NULL,
    display_order integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT suppliers_type_check CHECK ((type = ANY (ARRAY['company'::text, 'individual'::text, 'government'::text, 'utility'::text, 'telecom'::text, 'saas'::text, 'platform'::text, 'other'::text])))
);


--
-- Name: transactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transactions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    financial_product_id uuid NOT NULL,
    type text NOT NULL,
    amount numeric(15,2) NOT NULL,
    description text,
    event_date date NOT NULL,
    competence_date date,
    financial_period_id uuid,
    statement_cycle_id uuid,
    liability_installment_id uuid,
    category_id uuid,
    priority text,
    is_confirmed boolean DEFAULT false NOT NULL,
    notes text,
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    origin_type text DEFAULT 'manual'::text NOT NULL,
    recurring_instance_id uuid,
    supplier_id uuid,
    source_document_id uuid,
    CONSTRAINT transactions_origin_type_check CHECK ((origin_type = ANY (ARRAY['manual'::text, 'import'::text, 'recurring'::text, 'statement_link'::text]))),
    CONSTRAINT transactions_priority_check CHECK (((priority IS NULL) OR (priority = ANY (ARRAY['essential'::text, 'high'::text, 'medium'::text, 'low'::text, 'optional'::text])))),
    CONSTRAINT transactions_type_check CHECK ((type = ANY (ARRAY['income'::text, 'expense'::text, 'refund'::text, 'adjustment'::text, 'interest_charge'::text, 'fee'::text, 'statement_payment'::text, 'liability_payment'::text])))
);


--
-- Name: COLUMN transactions.source_document_id; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.transactions.source_document_id IS 'Documento de origem da transação (nota fiscal, fatura, comprovante)';


--
-- Name: mv_supplier_spending; Type: MATERIALIZED VIEW; Schema: public; Owner: -
--

CREATE MATERIALIZED VIEW public.mv_supplier_spending AS
 SELECT s.user_id,
    s.id AS supplier_id,
    s.name AS supplier_name,
    s.type AS supplier_type,
    count(DISTINCT t.id) AS transaction_count,
    COALESCE(sum(t.amount), (0)::numeric) AS total_spent,
    min(t.event_date) AS first_transaction_date,
    max(t.event_date) AS last_transaction_date,
    count(DISTINCT t.financial_period_id) AS periods_active
   FROM (public.suppliers s
     LEFT JOIN public.transactions t ON (((t.supplier_id = s.id) AND (t.user_id = s.user_id) AND (t.type = ANY (ARRAY['expense'::text, 'fee'::text, 'interest_charge'::text])))))
  WHERE (s.is_active = true)
  GROUP BY s.user_id, s.id, s.name, s.type
  WITH NO DATA;


--
-- Name: parsed_document_versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.parsed_document_versions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    source_document_id uuid NOT NULL,
    user_id uuid NOT NULL,
    version_number integer DEFAULT 1 NOT NULL,
    parser_type text NOT NULL,
    parser_version text,
    raw_text text,
    structured_data jsonb,
    confidence_score numeric(3,2),
    metadata jsonb DEFAULT '{}'::jsonb,
    created_at timestamp with time zone DEFAULT now(),
    CONSTRAINT parsed_document_versions_parser_type_check CHECK ((parser_type = ANY (ARRAY['local_text'::text, 'local_regex'::text, 'openai_vision'::text, 'openai_text'::text])))
);


--
-- Name: provider_connections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.provider_connections (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    provider text NOT NULL,
    external_item_id text NOT NULL,
    institution_name text,
    status text DEFAULT 'active'::text NOT NULL,
    last_synced_at timestamp with time zone,
    metadata jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT provider_connections_provider_check CHECK ((provider = 'pluggy'::text)),
    CONSTRAINT provider_connections_status_check CHECK ((status = ANY (ARRAY['active'::text, 'error'::text, 'revoked'::text, 'expired'::text])))
);


--
-- Name: recurring_instances; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recurring_instances (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    recurring_template_id uuid NOT NULL,
    expected_date date NOT NULL,
    expected_amount numeric(15,2),
    actual_amount numeric(15,2),
    status text DEFAULT 'pending'::text NOT NULL,
    paid_date date,
    transaction_id uuid,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT recurring_instances_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'paid'::text, 'partial'::text, 'skipped'::text, 'overdue'::text, 'cancelled'::text])))
);


--
-- Name: recurring_template_tags; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recurring_template_tags (
    recurring_template_id uuid NOT NULL,
    tag_id uuid NOT NULL
);


--
-- Name: recurring_templates; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recurring_templates (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    financial_product_id uuid,
    name text NOT NULL,
    type text NOT NULL,
    amount numeric(15,2),
    is_variable_amount boolean DEFAULT false NOT NULL,
    frequency text NOT NULL,
    day_of_month integer,
    custom_interval_days integer,
    category_id uuid,
    priority text,
    starts_at date,
    ends_at date,
    is_active boolean DEFAULT true NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    supplier_id uuid,
    CONSTRAINT recurring_templates_day_of_month_check CHECK (((day_of_month IS NULL) OR ((day_of_month >= 1) AND (day_of_month <= 31)))),
    CONSTRAINT recurring_templates_frequency_check CHECK ((frequency = ANY (ARRAY['monthly'::text, 'weekly'::text, 'biweekly'::text, 'quarterly'::text, 'annual'::text, 'custom'::text]))),
    CONSTRAINT recurring_templates_priority_check CHECK (((priority IS NULL) OR (priority = ANY (ARRAY['essential'::text, 'high'::text, 'medium'::text, 'low'::text, 'optional'::text])))),
    CONSTRAINT recurring_templates_type_check CHECK ((type = ANY (ARRAY['income'::text, 'expense'::text, 'liability_payment'::text, 'statement_payment'::text])))
);


--
-- Name: source_documents; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.source_documents (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    origin_type text NOT NULL,
    origin_key text NOT NULL,
    gmail_message_id text,
    gmail_thread_id text,
    gmail_attachment_id text,
    gmail_label text,
    gmail_date timestamp with time zone,
    gmail_from text,
    gmail_subject text,
    local_filepath text,
    local_mtime timestamp with time zone,
    filename text NOT NULL,
    mime_type text,
    file_size_bytes bigint,
    storage_path text,
    status text DEFAULT 'new'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    content_hash text,
    metadata jsonb,
    document_type text,
    supplier_id uuid,
    supplier_name_raw text,
    CONSTRAINT source_documents_origin_type_check CHECK ((origin_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'manual_upload'::text])))
);


--
-- Name: COLUMN source_documents.metadata; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.source_documents.metadata IS 'Metadados extraídos do documento (amount, date, due_date, total, cycle_start, cycle_end, etc.)';


--
-- Name: COLUMN source_documents.document_type; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.source_documents.document_type IS 'Tipo semântico do documento: invoice, receipt, credit_card_statement, bank_statement, etc.';


--
-- Name: COLUMN source_documents.supplier_id; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.source_documents.supplier_id IS 'Fornecedor/empresa vinculada ao documento';


--
-- Name: COLUMN source_documents.supplier_name_raw; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.source_documents.supplier_name_raw IS 'Nome bruto do fornecedor extraído do documento antes de normalização';


--
-- Name: statement_cycles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.statement_cycles (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    card_id uuid NOT NULL,
    reference_month date NOT NULL,
    cycle_start_date date NOT NULL,
    cycle_end_date date NOT NULL,
    due_date date NOT NULL,
    total_amount numeric(15,2) DEFAULT 0,
    paid_amount numeric(15,2) DEFAULT 0,
    status text DEFAULT 'open'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT statement_cycles_status_check CHECK ((status = ANY (ARRAY['open'::text, 'closed'::text, 'paid'::text, 'partial'::text, 'overdue'::text])))
);


--
-- Name: statement_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.statement_items (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    statement_cycle_id uuid NOT NULL,
    transaction_id uuid,
    description text,
    amount numeric(15,2) NOT NULL,
    transaction_date date,
    installment_number integer,
    total_installments integer,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    supplier_id uuid
);


--
-- Name: supplier_aliases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.supplier_aliases (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    supplier_id uuid NOT NULL,
    alias_name text NOT NULL,
    alias_type text DEFAULT 'other'::text NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    valid_from date,
    valid_until date,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT supplier_aliases_alias_type_check CHECK ((alias_type = ANY (ARRAY['former_name'::text, 'abbreviation'::text, 'trade_name'::text, 'billing_name'::text, 'other'::text])))
);


--
-- Name: supplier_contracts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.supplier_contracts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    supplier_id uuid NOT NULL,
    contract_type text NOT NULL,
    identifier text,
    label text,
    is_active boolean DEFAULT true NOT NULL,
    start_date date,
    end_date date,
    metadata jsonb,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT supplier_contracts_contract_type_check CHECK ((contract_type = ANY (ARRAY['service'::text, 'subscription'::text, 'utility'::text, 'loan'::text, 'insurance'::text, 'maintenance'::text, 'other'::text])))
);


--
-- Name: supplier_tags; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.supplier_tags (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    supplier_id uuid NOT NULL,
    tag_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: tags; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.tags (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    name text NOT NULL,
    color text,
    influences_priority boolean DEFAULT false NOT NULL,
    suggested_priority text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT tags_suggested_priority_check CHECK (((suggested_priority IS NULL) OR (suggested_priority = ANY (ARRAY['essential'::text, 'high'::text, 'medium'::text, 'low'::text, 'optional'::text]))))
);


--
-- Name: transaction_tags; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transaction_tags (
    transaction_id uuid NOT NULL,
    tag_id uuid NOT NULL
);


--
-- Name: transfers; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transfers (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    source_product_id uuid NOT NULL,
    target_product_id uuid NOT NULL,
    amount numeric(15,2) NOT NULL,
    description text,
    event_date date NOT NULL,
    competence_date date,
    financial_period_id uuid,
    is_confirmed boolean DEFAULT false NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: user_financial_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_financial_preferences (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    financial_cycle_start_day integer,
    financial_cycle_anchor_date date,
    default_currency text DEFAULT 'BRL'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_financial_preferences_financial_cycle_start_day_check CHECK (((financial_cycle_start_day >= 1) AND (financial_cycle_start_day <= 31)))
);


--
-- Name: user_secrets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_secrets (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    secret_type text NOT NULL,
    entity_type text,
    entity_id uuid,
    encrypted_value text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    encryption_version integer DEFAULT 1 NOT NULL,
    contract_identifier text,
    label text,
    last_used_at timestamp with time zone,
    success_count integer DEFAULT 0 NOT NULL,
    CONSTRAINT chk_user_secrets_encrypted CHECK ((encryption_version >= 1)),
    CONSTRAINT chk_user_secrets_entity_type CHECK (((entity_type IS NULL) OR (entity_type = ANY (ARRAY['supplier'::text, 'financial_product'::text, 'card'::text, 'supplier_contract'::text, 'institution'::text]))))
);


--
-- Name: COLUMN user_secrets.encrypted_value; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.user_secrets.encrypted_value IS 'Valor encriptado com pgp_sym_encrypt(value, app.encryption_key).
   Use as funções encrypt_secret() e decrypt_secret() para manipular.
   Nunca armazene texto puro nesta coluna.';


--
-- Name: COLUMN user_secrets.encryption_version; Type: COMMENT; Schema: public; Owner: -
--

COMMENT ON COLUMN public.user_secrets.encryption_version IS 'Versão da chave de encriptação usada. Incrementar ao rotacionar a chave de encriptação.';


--
-- Name: v_expenses_deduplicated; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.v_expenses_deduplicated AS
 SELECT si.id AS canonical_id,
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
   FROM public.statement_items si
  WHERE (si.transaction_id IS NULL)
UNION ALL
 SELECT t.id AS canonical_id,
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
   FROM public.transactions t
  WHERE ((t.type = ANY (ARRAY['expense'::text, 'fee'::text, 'interest_charge'::text])) AND (t.type <> ALL (ARRAY['statement_payment'::text, 'refund'::text])));


--
-- Name: ai_chat_messages ai_chat_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_chat_messages
    ADD CONSTRAINT ai_chat_messages_pkey PRIMARY KEY (id);


--
-- Name: ai_chat_sessions ai_chat_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_chat_sessions
    ADD CONSTRAINT ai_chat_sessions_pkey PRIMARY KEY (id);


--
-- Name: audit_logs audit_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.audit_logs
    ADD CONSTRAINT audit_logs_pkey PRIMARY KEY (id);


--
-- Name: cards cards_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cards
    ADD CONSTRAINT cards_pkey PRIMARY KEY (id);


--
-- Name: categories categories_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.categories
    ADD CONSTRAINT categories_pkey PRIMARY KEY (id);


--
-- Name: consumption_metrics consumption_metrics_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.consumption_metrics
    ADD CONSTRAINT consumption_metrics_pkey PRIMARY KEY (id);


--
-- Name: document_fingerprints document_fingerprints_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_fingerprints
    ADD CONSTRAINT document_fingerprints_pkey PRIMARY KEY (id);


--
-- Name: document_fingerprints document_fingerprints_user_id_content_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_fingerprints
    ADD CONSTRAINT document_fingerprints_user_id_content_hash_key UNIQUE (user_id, content_hash);


--
-- Name: document_patterns document_patterns_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_patterns
    ADD CONSTRAINT document_patterns_pkey PRIMARY KEY (id);


--
-- Name: document_patterns document_patterns_user_id_name_version_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_patterns
    ADD CONSTRAINT document_patterns_user_id_name_version_key UNIQUE (user_id, name, version);


--
-- Name: document_splits document_splits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_splits
    ADD CONSTRAINT document_splits_pkey PRIMARY KEY (id);


--
-- Name: document_transactions document_transactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_transactions
    ADD CONSTRAINT document_transactions_pkey PRIMARY KEY (id);


--
-- Name: document_transactions document_transactions_source_document_id_transaction_id_lin_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_transactions
    ADD CONSTRAINT document_transactions_source_document_id_transaction_id_lin_key UNIQUE (source_document_id, transaction_id, link_type);


--
-- Name: documents_legacy documents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.documents_legacy
    ADD CONSTRAINT documents_pkey PRIMARY KEY (id);


--
-- Name: draft_batches draft_batches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_batches
    ADD CONSTRAINT draft_batches_pkey PRIMARY KEY (id);


--
-- Name: draft_records draft_records_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_pkey PRIMARY KEY (id);


--
-- Name: external_account_mappings external_account_mappings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.external_account_mappings
    ADD CONSTRAINT external_account_mappings_pkey PRIMARY KEY (id);


--
-- Name: external_account_mappings external_account_mappings_provider_connection_id_external_a_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.external_account_mappings
    ADD CONSTRAINT external_account_mappings_provider_connection_id_external_a_key UNIQUE (provider_connection_id, external_account_id);


--
-- Name: extraction_results extraction_results_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.extraction_results
    ADD CONSTRAINT extraction_results_pkey PRIMARY KEY (id);


--
-- Name: financial_obligation_evidences financial_obligation_evidence_user_id_obligation_id_source__key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligation_evidences
    ADD CONSTRAINT financial_obligation_evidence_user_id_obligation_id_source__key UNIQUE (user_id, obligation_id, source_document_id);


--
-- Name: financial_obligation_evidences financial_obligation_evidences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligation_evidences
    ADD CONSTRAINT financial_obligation_evidences_pkey PRIMARY KEY (id);


--
-- Name: financial_obligation_identity_keys financial_obligation_identity_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligation_identity_keys
    ADD CONSTRAINT financial_obligation_identity_keys_pkey PRIMARY KEY (id);


--
-- Name: financial_obligations financial_obligations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligations
    ADD CONSTRAINT financial_obligations_pkey PRIMARY KEY (id);


--
-- Name: financial_periods financial_periods_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_periods
    ADD CONSTRAINT financial_periods_pkey PRIMARY KEY (id);


--
-- Name: financial_products financial_products_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_products
    ADD CONSTRAINT financial_products_pkey PRIMARY KEY (id);


--
-- Name: import_jobs import_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_jobs
    ADD CONSTRAINT import_jobs_pkey PRIMARY KEY (id);


--
-- Name: ingestion_checkpoints ingestion_checkpoints_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_checkpoints
    ADD CONSTRAINT ingestion_checkpoints_pkey PRIMARY KEY (id);


--
-- Name: ingestion_checkpoints ingestion_checkpoints_user_id_source_type_scope_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_checkpoints
    ADD CONSTRAINT ingestion_checkpoints_user_id_source_type_scope_key_key UNIQUE (user_id, source_type, scope_key);


--
-- Name: ingestion_jobs ingestion_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_pkey PRIMARY KEY (id);


--
-- Name: ingestion_logs ingestion_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_logs
    ADD CONSTRAINT ingestion_logs_pkey PRIMARY KEY (id);


--
-- Name: ingestion_runs ingestion_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_runs
    ADD CONSTRAINT ingestion_runs_pkey PRIMARY KEY (id);


--
-- Name: institutions institutions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.institutions
    ADD CONSTRAINT institutions_pkey PRIMARY KEY (id);


--
-- Name: liabilities liabilities_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liabilities
    ADD CONSTRAINT liabilities_pkey PRIMARY KEY (id);


--
-- Name: liability_installments liability_installments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liability_installments
    ADD CONSTRAINT liability_installments_pkey PRIMARY KEY (id);


--
-- Name: liability_tags liability_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liability_tags
    ADD CONSTRAINT liability_tags_pkey PRIMARY KEY (liability_id, tag_id);


--
-- Name: parsed_document_versions parsed_document_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.parsed_document_versions
    ADD CONSTRAINT parsed_document_versions_pkey PRIMARY KEY (id);


--
-- Name: parsed_document_versions parsed_document_versions_source_document_id_version_number_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.parsed_document_versions
    ADD CONSTRAINT parsed_document_versions_source_document_id_version_number_key UNIQUE (source_document_id, version_number);


--
-- Name: pattern_feedback pattern_feedback_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pattern_feedback
    ADD CONSTRAINT pattern_feedback_pkey PRIMARY KEY (id);


--
-- Name: provider_connections provider_connections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_connections
    ADD CONSTRAINT provider_connections_pkey PRIMARY KEY (id);


--
-- Name: provider_connections provider_connections_user_id_provider_external_item_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.provider_connections
    ADD CONSTRAINT provider_connections_user_id_provider_external_item_id_key UNIQUE (user_id, provider, external_item_id);


--
-- Name: recurring_instances recurring_instances_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_instances
    ADD CONSTRAINT recurring_instances_pkey PRIMARY KEY (id);


--
-- Name: recurring_template_tags recurring_template_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_template_tags
    ADD CONSTRAINT recurring_template_tags_pkey PRIMARY KEY (recurring_template_id, tag_id);


--
-- Name: recurring_templates recurring_templates_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_templates
    ADD CONSTRAINT recurring_templates_pkey PRIMARY KEY (id);


--
-- Name: source_documents source_documents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.source_documents
    ADD CONSTRAINT source_documents_pkey PRIMARY KEY (id);


--
-- Name: source_documents source_documents_user_id_origin_type_origin_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.source_documents
    ADD CONSTRAINT source_documents_user_id_origin_type_origin_key_key UNIQUE (user_id, origin_type, origin_key);


--
-- Name: statement_cycles statement_cycles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statement_cycles
    ADD CONSTRAINT statement_cycles_pkey PRIMARY KEY (id);


--
-- Name: statement_items statement_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statement_items
    ADD CONSTRAINT statement_items_pkey PRIMARY KEY (id);


--
-- Name: supplier_aliases supplier_aliases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_aliases
    ADD CONSTRAINT supplier_aliases_pkey PRIMARY KEY (id);


--
-- Name: supplier_contracts supplier_contracts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_contracts
    ADD CONSTRAINT supplier_contracts_pkey PRIMARY KEY (id);


--
-- Name: supplier_tags supplier_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_tags
    ADD CONSTRAINT supplier_tags_pkey PRIMARY KEY (id);


--
-- Name: supplier_tags supplier_tags_supplier_id_tag_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_tags
    ADD CONSTRAINT supplier_tags_supplier_id_tag_id_key UNIQUE (supplier_id, tag_id);


--
-- Name: suppliers suppliers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.suppliers
    ADD CONSTRAINT suppliers_pkey PRIMARY KEY (id);


--
-- Name: tags tags_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.tags
    ADD CONSTRAINT tags_pkey PRIMARY KEY (id);


--
-- Name: transaction_tags transaction_tags_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transaction_tags
    ADD CONSTRAINT transaction_tags_pkey PRIMARY KEY (transaction_id, tag_id);


--
-- Name: transactions transactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_pkey PRIMARY KEY (id);


--
-- Name: transfers transfers_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfers
    ADD CONSTRAINT transfers_pkey PRIMARY KEY (id);


--
-- Name: user_financial_preferences user_financial_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_financial_preferences
    ADD CONSTRAINT user_financial_preferences_pkey PRIMARY KEY (id);


--
-- Name: user_financial_preferences user_financial_preferences_user_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_financial_preferences
    ADD CONSTRAINT user_financial_preferences_user_id_key UNIQUE (user_id);


--
-- Name: user_secrets user_secrets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_secrets
    ADD CONSTRAINT user_secrets_pkey PRIMARY KEY (id);


--
-- Name: idx_ai_chat_messages_session; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ai_chat_messages_session ON public.ai_chat_messages USING btree (session_id, created_at);


--
-- Name: idx_ai_chat_messages_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ai_chat_messages_user ON public.ai_chat_messages USING btree (user_id);


--
-- Name: idx_ai_chat_sessions_updated; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ai_chat_sessions_updated ON public.ai_chat_sessions USING btree (updated_at DESC);


--
-- Name: idx_ai_chat_sessions_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ai_chat_sessions_user ON public.ai_chat_sessions USING btree (user_id);


--
-- Name: idx_audit_logs_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_audit_logs_entity ON public.audit_logs USING btree (entity_type, entity_id);


--
-- Name: idx_audit_logs_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_audit_logs_user_id ON public.audit_logs USING btree (user_id);


--
-- Name: idx_cards_financial_product_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_cards_financial_product_id ON public.cards USING btree (financial_product_id);


--
-- Name: idx_cards_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_cards_user_id ON public.cards USING btree (user_id);


--
-- Name: idx_categories_parent_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_categories_parent_id ON public.categories USING btree (parent_id);


--
-- Name: idx_categories_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_categories_user_id ON public.categories USING btree (user_id);


--
-- Name: idx_consumption_metrics_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_consumption_metrics_supplier_id ON public.consumption_metrics USING btree (supplier_id);


--
-- Name: idx_consumption_metrics_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_consumption_metrics_user_id ON public.consumption_metrics USING btree (user_id);


--
-- Name: idx_document_fingerprints_canonical; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_fingerprints_canonical ON public.document_fingerprints USING btree (user_id, canonical_fingerprint) WHERE (canonical_fingerprint IS NOT NULL);


--
-- Name: idx_document_patterns_institution; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_patterns_institution ON public.document_patterns USING btree (institution_id) WHERE (is_active = true);


--
-- Name: idx_document_patterns_supplier_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_patterns_supplier_type ON public.document_patterns USING btree (supplier_id, document_type) WHERE (is_active = true);


--
-- Name: idx_document_patterns_user_active; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_patterns_user_active ON public.document_patterns USING btree (user_id, is_active);


--
-- Name: idx_document_splits_category; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_splits_category ON public.document_splits USING btree (category_id);


--
-- Name: idx_document_splits_source_document; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_splits_source_document ON public.document_splits USING btree (source_document_id);


--
-- Name: idx_document_splits_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_splits_user ON public.document_splits USING btree (user_id);


--
-- Name: idx_document_transactions_source; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_transactions_source ON public.document_transactions USING btree (source_document_id);


--
-- Name: idx_document_transactions_tx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_transactions_tx ON public.document_transactions USING btree (transaction_id);


--
-- Name: idx_document_transactions_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_document_transactions_user ON public.document_transactions USING btree (user_id);


--
-- Name: idx_documents_entity; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_documents_entity ON public.documents_legacy USING btree (entity_type, entity_id);


--
-- Name: idx_documents_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_documents_supplier_id ON public.documents_legacy USING btree (supplier_id);


--
-- Name: idx_documents_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_documents_user_id ON public.documents_legacy USING btree (user_id);


--
-- Name: idx_draft_batches_source_document; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_batches_source_document ON public.draft_batches USING btree (source_document_id) WHERE (source_document_id IS NOT NULL);


--
-- Name: idx_draft_batches_user_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_batches_user_status ON public.draft_batches USING btree (user_id, status);


--
-- Name: idx_draft_records_batch; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_records_batch ON public.draft_records USING btree (batch_id) WHERE (batch_id IS NOT NULL);


--
-- Name: idx_draft_records_external_ref; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_draft_records_external_ref ON public.draft_records USING btree (user_id, external_ref) WHERE (external_ref IS NOT NULL);


--
-- Name: idx_draft_records_reconciled_transaction; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_records_reconciled_transaction ON public.draft_records USING btree (reconciled_transaction_id) WHERE (reconciled_transaction_id IS NOT NULL);


--
-- Name: idx_draft_records_reconciliation_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_records_reconciliation_status ON public.draft_records USING btree (user_id, reconciliation_status) WHERE (reconciliation_status <> ALL (ARRAY['not_checked'::text, 'no_match'::text]));


--
-- Name: idx_draft_records_review_queue; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_records_review_queue ON public.draft_records USING btree (user_id, status, created_at) WHERE (status = 'pending_review'::text);


--
-- Name: idx_draft_records_source_doc; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_draft_records_source_doc ON public.draft_records USING btree (source_document_id) WHERE (source_document_id IS NOT NULL);


--
-- Name: idx_external_account_mappings_connection; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_external_account_mappings_connection ON public.external_account_mappings USING btree (provider_connection_id);


--
-- Name: idx_external_account_mappings_product; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_external_account_mappings_product ON public.external_account_mappings USING btree (financial_product_id);


--
-- Name: idx_external_account_mappings_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_external_account_mappings_user ON public.external_account_mappings USING btree (user_id);


--
-- Name: idx_extraction_results_ai_enrichment_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_extraction_results_ai_enrichment_type ON public.extraction_results USING btree (ai_enrichment_type) WHERE (ai_enrichment_type IS NOT NULL);


--
-- Name: idx_extraction_results_supplier; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_extraction_results_supplier ON public.extraction_results USING btree (user_id, supplier_id) WHERE (supplier_id IS NOT NULL);


--
-- Name: idx_extraction_results_version; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_extraction_results_version ON public.extraction_results USING btree (parsed_version_id);


--
-- Name: idx_financial_obligation_evidences_document; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_financial_obligation_evidences_document ON public.financial_obligation_evidences USING btree (source_document_id);


--
-- Name: idx_financial_obligation_evidences_obligation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_financial_obligation_evidences_obligation ON public.financial_obligation_evidences USING btree (obligation_id);


--
-- Name: idx_financial_obligations_identity; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_financial_obligations_identity ON public.financial_obligations USING btree (user_id, financial_identity_key) WHERE (financial_identity_key IS NOT NULL);


--
-- Name: idx_financial_obligations_user_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_financial_obligations_user_status ON public.financial_obligations USING btree (user_id, status);


--
-- Name: idx_financial_periods_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_financial_periods_user_id ON public.financial_periods USING btree (user_id);


--
-- Name: idx_financial_products_institution_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_financial_products_institution_id ON public.financial_products USING btree (institution_id);


--
-- Name: idx_financial_products_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_financial_products_user_id ON public.financial_products USING btree (user_id);


--
-- Name: idx_import_jobs_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_import_jobs_user_id ON public.import_jobs USING btree (user_id);


--
-- Name: idx_ingestion_jobs_needs_full_ai_review; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_jobs_needs_full_ai_review ON public.ingestion_jobs USING btree (needs_full_ai_review, user_id) WHERE (needs_full_ai_review = true);


--
-- Name: idx_ingestion_jobs_queue; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_jobs_queue ON public.ingestion_jobs USING btree (status, created_at) WHERE (status = ANY (ARRAY['queued'::text, 'discovered'::text]));


--
-- Name: idx_ingestion_jobs_run; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_jobs_run ON public.ingestion_jobs USING btree (run_id, status);


--
-- Name: idx_ingestion_jobs_user_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_jobs_user_status ON public.ingestion_jobs USING btree (user_id, status);


--
-- Name: idx_ingestion_logs_job; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_logs_job ON public.ingestion_logs USING btree (job_id, created_at) WHERE (job_id IS NOT NULL);


--
-- Name: idx_ingestion_logs_run; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_logs_run ON public.ingestion_logs USING btree (run_id, created_at);


--
-- Name: idx_ingestion_logs_user_level; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_logs_user_level ON public.ingestion_logs USING btree (user_id, level, created_at) WHERE (level = ANY (ARRAY['warn'::text, 'error'::text]));


--
-- Name: idx_ingestion_runs_user_source; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_runs_user_source ON public.ingestion_runs USING btree (user_id, source_type);


--
-- Name: idx_ingestion_runs_user_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_ingestion_runs_user_status ON public.ingestion_runs USING btree (user_id, status);


--
-- Name: idx_institutions_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_institutions_user_id ON public.institutions USING btree (user_id);


--
-- Name: idx_liabilities_financial_product_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_liabilities_financial_product_id ON public.liabilities USING btree (financial_product_id);


--
-- Name: idx_liabilities_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_liabilities_supplier_id ON public.liabilities USING btree (supplier_id);


--
-- Name: idx_liabilities_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_liabilities_user_id ON public.liabilities USING btree (user_id);


--
-- Name: idx_liability_installments_liability_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_liability_installments_liability_id ON public.liability_installments USING btree (liability_id);


--
-- Name: idx_liability_installments_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_liability_installments_user_id ON public.liability_installments USING btree (user_id);


--
-- Name: idx_mv_supplier_spending_pk; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_mv_supplier_spending_pk ON public.mv_supplier_spending USING btree (user_id, supplier_id);


--
-- Name: idx_mv_supplier_spending_total; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_mv_supplier_spending_total ON public.mv_supplier_spending USING btree (user_id, total_spent DESC);


--
-- Name: idx_parsed_versions_doc; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_parsed_versions_doc ON public.parsed_document_versions USING btree (source_document_id, version_number DESC);


--
-- Name: idx_pattern_feedback_document; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pattern_feedback_document ON public.pattern_feedback USING btree (source_document_id) WHERE (source_document_id IS NOT NULL);


--
-- Name: idx_pattern_feedback_pattern; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pattern_feedback_pattern ON public.pattern_feedback USING btree (pattern_id, created_at DESC);


--
-- Name: idx_pattern_feedback_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_pattern_feedback_user ON public.pattern_feedback USING btree (user_id, created_at DESC);


--
-- Name: idx_provider_connections_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_provider_connections_user ON public.provider_connections USING btree (user_id);


--
-- Name: idx_recurring_instances_template_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_recurring_instances_template_id ON public.recurring_instances USING btree (recurring_template_id);


--
-- Name: idx_recurring_instances_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_recurring_instances_user_id ON public.recurring_instances USING btree (user_id);


--
-- Name: idx_recurring_templates_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_recurring_templates_supplier_id ON public.recurring_templates USING btree (supplier_id);


--
-- Name: idx_recurring_templates_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_recurring_templates_user_id ON public.recurring_templates USING btree (user_id);


--
-- Name: idx_source_documents_content_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_source_documents_content_hash ON public.source_documents USING btree (user_id, content_hash) WHERE (content_hash IS NOT NULL);


--
-- Name: idx_source_documents_document_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_source_documents_document_type ON public.source_documents USING btree (user_id, document_type) WHERE (document_type IS NOT NULL);


--
-- Name: idx_source_documents_gmail_msg; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_source_documents_gmail_msg ON public.source_documents USING btree (user_id, gmail_message_id) WHERE (gmail_message_id IS NOT NULL);


--
-- Name: idx_source_documents_gmail_msg_file_dedup; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_source_documents_gmail_msg_file_dedup ON public.source_documents USING btree (user_id, gmail_message_id, filename) WHERE ((origin_type = 'gmail'::text) AND (gmail_message_id IS NOT NULL));


--
-- Name: idx_source_documents_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_source_documents_status ON public.source_documents USING btree (user_id, status);


--
-- Name: idx_source_documents_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_source_documents_supplier_id ON public.source_documents USING btree (supplier_id) WHERE (supplier_id IS NOT NULL);


--
-- Name: idx_source_documents_user_origin; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_source_documents_user_origin ON public.source_documents USING btree (user_id, origin_type);


--
-- Name: idx_statement_cycles_card_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_statement_cycles_card_id ON public.statement_cycles USING btree (card_id);


--
-- Name: idx_statement_cycles_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_statement_cycles_user_id ON public.statement_cycles USING btree (user_id);


--
-- Name: idx_statement_items_statement_cycle_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_statement_items_statement_cycle_id ON public.statement_items USING btree (statement_cycle_id);


--
-- Name: idx_statement_items_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_statement_items_supplier_id ON public.statement_items USING btree (supplier_id);


--
-- Name: idx_statement_items_transaction_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_statement_items_transaction_id ON public.statement_items USING btree (transaction_id) WHERE (transaction_id IS NOT NULL);


--
-- Name: idx_statement_items_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_statement_items_user_id ON public.statement_items USING btree (user_id);


--
-- Name: idx_supplier_aliases_resolve; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_aliases_resolve ON public.supplier_aliases USING btree (user_id, alias_name, valid_from, valid_until) WHERE (is_active = true);


--
-- Name: idx_supplier_aliases_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_aliases_supplier_id ON public.supplier_aliases USING btree (supplier_id);


--
-- Name: idx_supplier_aliases_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_aliases_trgm ON public.supplier_aliases USING gin (alias_name public.gin_trgm_ops);


--
-- Name: idx_supplier_aliases_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_aliases_user_id ON public.supplier_aliases USING btree (user_id);


--
-- Name: idx_supplier_contracts_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_contracts_supplier_id ON public.supplier_contracts USING btree (supplier_id);


--
-- Name: idx_supplier_contracts_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_contracts_user_id ON public.supplier_contracts USING btree (user_id);


--
-- Name: idx_supplier_tags_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_supplier_tags_supplier_id ON public.supplier_tags USING btree (supplier_id);


--
-- Name: idx_suppliers_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_suppliers_name_trgm ON public.suppliers USING gin (name public.gin_trgm_ops);


--
-- Name: idx_suppliers_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_suppliers_user_id ON public.suppliers USING btree (user_id);


--
-- Name: idx_tags_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_tags_user_id ON public.tags USING btree (user_id);


--
-- Name: idx_transactions_category_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_category_id ON public.transactions USING btree (category_id);


--
-- Name: idx_transactions_event_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_event_date ON public.transactions USING btree (user_id, event_date);


--
-- Name: idx_transactions_financial_period_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_financial_period_id ON public.transactions USING btree (financial_period_id);


--
-- Name: idx_transactions_financial_product_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_financial_product_id ON public.transactions USING btree (financial_product_id);


--
-- Name: idx_transactions_recurring_instance_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_recurring_instance_id ON public.transactions USING btree (recurring_instance_id);


--
-- Name: idx_transactions_source_document_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_source_document_id ON public.transactions USING btree (source_document_id) WHERE (source_document_id IS NOT NULL);


--
-- Name: idx_transactions_statement_cycle_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_statement_cycle_id ON public.transactions USING btree (statement_cycle_id);


--
-- Name: idx_transactions_supplier_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_supplier_id ON public.transactions USING btree (supplier_id);


--
-- Name: idx_transactions_type; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_type ON public.transactions USING btree (user_id, type);


--
-- Name: idx_transactions_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transactions_user_id ON public.transactions USING btree (user_id);


--
-- Name: idx_transfers_source_product_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfers_source_product_id ON public.transfers USING btree (source_product_id);


--
-- Name: idx_transfers_target_product_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfers_target_product_id ON public.transfers USING btree (target_product_id);


--
-- Name: idx_transfers_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_transfers_user_id ON public.transfers USING btree (user_id);


--
-- Name: idx_user_financial_preferences_user_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_user_financial_preferences_user_id ON public.user_financial_preferences USING btree (user_id);


--
-- Name: ix_draft_records_obligation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_draft_records_obligation ON public.draft_records USING btree (obligation_id) WHERE (obligation_id IS NOT NULL);


--
-- Name: ix_draft_records_schema_version; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_draft_records_schema_version ON public.draft_records USING btree (user_id, draft_schema_version) WHERE (draft_schema_version = 0);


--
-- Name: ix_fo_identity_keys_obligation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_fo_identity_keys_obligation ON public.financial_obligation_identity_keys USING btree (obligation_id);


--
-- Name: ix_ingestion_checkpoints_status; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX ix_ingestion_checkpoints_status ON public.ingestion_checkpoints USING btree (user_id, status);


--
-- Name: uq_draft_records_materialization_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_draft_records_materialization_key ON public.draft_records USING btree (user_id, materialization_key);


--
-- Name: uq_draft_records_posted_record; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_draft_records_posted_record ON public.draft_records USING btree (posted_record_type, posted_record_id) WHERE (posted_record_id IS NOT NULL);


--
-- Name: uq_fo_identity_keys; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_fo_identity_keys ON public.financial_obligation_identity_keys USING btree (user_id, key);


--
-- Name: uq_user_secrets_scope; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX uq_user_secrets_scope ON public.user_secrets USING btree (user_id, secret_type, COALESCE(entity_type, '-'::text), COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uuid), COALESCE(contract_identifier, '-'::text));


--
-- Name: document_patterns document_patterns_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER document_patterns_updated_at BEFORE UPDATE ON public.document_patterns FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: pattern_feedback pattern_auto_deactivation; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER pattern_auto_deactivation AFTER INSERT ON public.pattern_feedback FOR EACH ROW EXECUTE FUNCTION public.check_pattern_auto_deactivation();


--
-- Name: audit_logs trg_audit_logs_immutable; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_audit_logs_immutable BEFORE DELETE OR UPDATE ON public.audit_logs FOR EACH ROW EXECUTE FUNCTION public.prevent_audit_log_mutation();


--
-- Name: suppliers trg_auto_alias_on_rename; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_auto_alias_on_rename BEFORE UPDATE ON public.suppliers FOR EACH ROW EXECUTE FUNCTION public.auto_alias_on_rename();


--
-- Name: cards trg_cards_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_cards_updated_at BEFORE UPDATE ON public.cards FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: categories trg_categories_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_categories_updated_at BEFORE UPDATE ON public.categories FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: supplier_aliases trg_check_alias_uniqueness; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_check_alias_uniqueness BEFORE INSERT OR UPDATE ON public.supplier_aliases FOR EACH ROW EXECUTE FUNCTION public.check_alias_uniqueness();


--
-- Name: consumption_metrics trg_consumption_metrics_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_consumption_metrics_updated_at BEFORE UPDATE ON public.consumption_metrics FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: document_splits trg_document_splits_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_document_splits_updated_at BEFORE UPDATE ON public.document_splits FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();


--
-- Name: documents_legacy trg_documents_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_documents_updated_at BEFORE UPDATE ON public.documents_legacy FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: draft_records trg_draft_records_materialization_key; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_draft_records_materialization_key BEFORE INSERT ON public.draft_records FOR EACH ROW EXECUTE FUNCTION public.fn_draft_records_set_materialization_key();


--
-- Name: financial_obligations trg_financial_obligations_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_financial_obligations_updated_at BEFORE UPDATE ON public.financial_obligations FOR EACH ROW EXECUTE FUNCTION public.update_financial_obligations_updated_at();


--
-- Name: financial_periods trg_financial_periods_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_financial_periods_updated_at BEFORE UPDATE ON public.financial_periods FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: financial_products trg_financial_products_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_financial_products_updated_at BEFORE UPDATE ON public.financial_products FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: import_jobs trg_import_jobs_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_import_jobs_updated_at BEFORE UPDATE ON public.import_jobs FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: ingestion_checkpoints trg_ingestion_checkpoints_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_ingestion_checkpoints_updated_at BEFORE UPDATE ON public.ingestion_checkpoints FOR EACH ROW EXECUTE FUNCTION public.update_ingestion_checkpoints_updated_at();


--
-- Name: institutions trg_institutions_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_institutions_updated_at BEFORE UPDATE ON public.institutions FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: liabilities trg_liabilities_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_liabilities_updated_at BEFORE UPDATE ON public.liabilities FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: liability_installments trg_liability_installments_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_liability_installments_updated_at BEFORE UPDATE ON public.liability_installments FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: recurring_instances trg_recurring_instances_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_recurring_instances_updated_at BEFORE UPDATE ON public.recurring_instances FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: recurring_templates trg_recurring_templates_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_recurring_templates_updated_at BEFORE UPDATE ON public.recurring_templates FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: statement_cycles trg_statement_cycles_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_statement_cycles_updated_at BEFORE UPDATE ON public.statement_cycles FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: statement_items trg_statement_items_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_statement_items_updated_at BEFORE UPDATE ON public.statement_items FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: supplier_aliases trg_supplier_aliases_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_supplier_aliases_updated_at BEFORE UPDATE ON public.supplier_aliases FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: supplier_contracts trg_supplier_contracts_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_supplier_contracts_updated_at BEFORE UPDATE ON public.supplier_contracts FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: suppliers trg_suppliers_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_suppliers_updated_at BEFORE UPDATE ON public.suppliers FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: tags trg_tags_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_tags_updated_at BEFORE UPDATE ON public.tags FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: transactions trg_transactions_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_transactions_updated_at BEFORE UPDATE ON public.transactions FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: transfers trg_transfers_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_transfers_updated_at BEFORE UPDATE ON public.transfers FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: user_financial_preferences trg_user_financial_preferences_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_user_financial_preferences_updated_at BEFORE UPDATE ON public.user_financial_preferences FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: user_secrets trg_user_secrets_updated_at; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_user_secrets_updated_at BEFORE UPDATE ON public.user_secrets FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();


--
-- Name: document_splits trg_validate_splits_sum; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER trg_validate_splits_sum BEFORE INSERT OR UPDATE ON public.document_splits FOR EACH ROW EXECUTE FUNCTION public.fn_validate_splits_sum();


--
-- Name: ai_chat_messages ai_chat_messages_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ai_chat_messages
    ADD CONSTRAINT ai_chat_messages_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.ai_chat_sessions(id) ON DELETE CASCADE;


--
-- Name: cards cards_financial_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.cards
    ADD CONSTRAINT cards_financial_product_id_fkey FOREIGN KEY (financial_product_id) REFERENCES public.financial_products(id) ON DELETE CASCADE;


--
-- Name: categories categories_parent_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.categories
    ADD CONSTRAINT categories_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES public.categories(id) ON DELETE SET NULL;


--
-- Name: consumption_metrics consumption_metrics_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.consumption_metrics
    ADD CONSTRAINT consumption_metrics_document_id_fkey FOREIGN KEY (document_id) REFERENCES public.documents_legacy(id) ON DELETE SET NULL;


--
-- Name: consumption_metrics consumption_metrics_supplier_contract_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.consumption_metrics
    ADD CONSTRAINT consumption_metrics_supplier_contract_id_fkey FOREIGN KEY (supplier_contract_id) REFERENCES public.supplier_contracts(id) ON DELETE SET NULL;


--
-- Name: consumption_metrics consumption_metrics_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.consumption_metrics
    ADD CONSTRAINT consumption_metrics_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE CASCADE;


--
-- Name: consumption_metrics consumption_metrics_transaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.consumption_metrics
    ADD CONSTRAINT consumption_metrics_transaction_id_fkey FOREIGN KEY (transaction_id) REFERENCES public.transactions(id) ON DELETE SET NULL;


--
-- Name: document_fingerprints document_fingerprints_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_fingerprints
    ADD CONSTRAINT document_fingerprints_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id);


--
-- Name: document_patterns document_patterns_institution_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_patterns
    ADD CONSTRAINT document_patterns_institution_id_fkey FOREIGN KEY (institution_id) REFERENCES public.institutions(id) ON DELETE SET NULL;


--
-- Name: document_patterns document_patterns_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_patterns
    ADD CONSTRAINT document_patterns_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: document_splits document_splits_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_splits
    ADD CONSTRAINT document_splits_category_id_fkey FOREIGN KEY (category_id) REFERENCES public.categories(id) ON DELETE SET NULL;


--
-- Name: document_splits document_splits_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_splits
    ADD CONSTRAINT document_splits_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id) ON DELETE CASCADE;


--
-- Name: document_transactions document_transactions_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_transactions
    ADD CONSTRAINT document_transactions_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id) ON DELETE CASCADE;


--
-- Name: document_transactions document_transactions_transaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.document_transactions
    ADD CONSTRAINT document_transactions_transaction_id_fkey FOREIGN KEY (transaction_id) REFERENCES public.transactions(id) ON DELETE CASCADE;


--
-- Name: documents_legacy documents_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.documents_legacy
    ADD CONSTRAINT documents_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: draft_batches draft_batches_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_batches
    ADD CONSTRAINT draft_batches_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.ingestion_runs(id);


--
-- Name: draft_batches draft_batches_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_batches
    ADD CONSTRAINT draft_batches_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id) ON DELETE SET NULL;


--
-- Name: draft_records draft_records_batch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_batch_id_fkey FOREIGN KEY (batch_id) REFERENCES public.draft_batches(id);


--
-- Name: draft_records draft_records_extraction_result_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_extraction_result_id_fkey FOREIGN KEY (extraction_result_id) REFERENCES public.extraction_results(id);


--
-- Name: draft_records draft_records_obligation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_obligation_id_fkey FOREIGN KEY (obligation_id) REFERENCES public.financial_obligations(id) ON DELETE SET NULL;


--
-- Name: draft_records draft_records_reconciled_template_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_reconciled_template_id_fkey FOREIGN KEY (reconciled_template_id) REFERENCES public.recurring_templates(id) ON DELETE SET NULL;


--
-- Name: draft_records draft_records_reconciled_transaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_reconciled_transaction_id_fkey FOREIGN KEY (reconciled_transaction_id) REFERENCES public.transactions(id) ON DELETE SET NULL;


--
-- Name: draft_records draft_records_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.draft_records
    ADD CONSTRAINT draft_records_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id);


--
-- Name: external_account_mappings external_account_mappings_financial_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.external_account_mappings
    ADD CONSTRAINT external_account_mappings_financial_product_id_fkey FOREIGN KEY (financial_product_id) REFERENCES public.financial_products(id);


--
-- Name: external_account_mappings external_account_mappings_provider_connection_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.external_account_mappings
    ADD CONSTRAINT external_account_mappings_provider_connection_id_fkey FOREIGN KEY (provider_connection_id) REFERENCES public.provider_connections(id) ON DELETE CASCADE;


--
-- Name: extraction_results extraction_results_parsed_version_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.extraction_results
    ADD CONSTRAINT extraction_results_parsed_version_id_fkey FOREIGN KEY (parsed_version_id) REFERENCES public.parsed_document_versions(id);


--
-- Name: extraction_results extraction_results_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.extraction_results
    ADD CONSTRAINT extraction_results_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id);


--
-- Name: financial_obligation_evidences financial_obligation_evidences_obligation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligation_evidences
    ADD CONSTRAINT financial_obligation_evidences_obligation_id_fkey FOREIGN KEY (obligation_id) REFERENCES public.financial_obligations(id) ON DELETE CASCADE;


--
-- Name: financial_obligation_evidences financial_obligation_evidences_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligation_evidences
    ADD CONSTRAINT financial_obligation_evidences_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id) ON DELETE CASCADE;


--
-- Name: financial_obligation_identity_keys financial_obligation_identity_keys_obligation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligation_identity_keys
    ADD CONSTRAINT financial_obligation_identity_keys_obligation_id_fkey FOREIGN KEY (obligation_id) REFERENCES public.financial_obligations(id) ON DELETE CASCADE;


--
-- Name: financial_obligations financial_obligations_financial_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligations
    ADD CONSTRAINT financial_obligations_financial_product_id_fkey FOREIGN KEY (financial_product_id) REFERENCES public.financial_products(id) ON DELETE SET NULL;


--
-- Name: financial_obligations financial_obligations_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_obligations
    ADD CONSTRAINT financial_obligations_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: financial_products financial_products_institution_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.financial_products
    ADD CONSTRAINT financial_products_institution_id_fkey FOREIGN KEY (institution_id) REFERENCES public.institutions(id) ON DELETE CASCADE;


--
-- Name: ingestion_jobs ingestion_jobs_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.ingestion_runs(id);


--
-- Name: ingestion_jobs ingestion_jobs_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id);


--
-- Name: ingestion_logs ingestion_logs_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_logs
    ADD CONSTRAINT ingestion_logs_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.ingestion_jobs(id);


--
-- Name: ingestion_logs ingestion_logs_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.ingestion_logs
    ADD CONSTRAINT ingestion_logs_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.ingestion_runs(id);


--
-- Name: liabilities liabilities_financial_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liabilities
    ADD CONSTRAINT liabilities_financial_product_id_fkey FOREIGN KEY (financial_product_id) REFERENCES public.financial_products(id) ON DELETE CASCADE;


--
-- Name: liabilities liabilities_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liabilities
    ADD CONSTRAINT liabilities_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: liability_installments liability_installments_liability_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liability_installments
    ADD CONSTRAINT liability_installments_liability_id_fkey FOREIGN KEY (liability_id) REFERENCES public.liabilities(id) ON DELETE CASCADE;


--
-- Name: liability_tags liability_tags_liability_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liability_tags
    ADD CONSTRAINT liability_tags_liability_id_fkey FOREIGN KEY (liability_id) REFERENCES public.liabilities(id) ON DELETE CASCADE;


--
-- Name: liability_tags liability_tags_tag_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.liability_tags
    ADD CONSTRAINT liability_tags_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES public.tags(id) ON DELETE CASCADE;


--
-- Name: parsed_document_versions parsed_document_versions_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.parsed_document_versions
    ADD CONSTRAINT parsed_document_versions_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id);


--
-- Name: pattern_feedback pattern_feedback_pattern_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pattern_feedback
    ADD CONSTRAINT pattern_feedback_pattern_id_fkey FOREIGN KEY (pattern_id) REFERENCES public.document_patterns(id) ON DELETE CASCADE;


--
-- Name: pattern_feedback pattern_feedback_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pattern_feedback
    ADD CONSTRAINT pattern_feedback_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id) ON DELETE SET NULL;


--
-- Name: recurring_instances recurring_instances_recurring_template_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_instances
    ADD CONSTRAINT recurring_instances_recurring_template_id_fkey FOREIGN KEY (recurring_template_id) REFERENCES public.recurring_templates(id) ON DELETE CASCADE;


--
-- Name: recurring_instances recurring_instances_transaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_instances
    ADD CONSTRAINT recurring_instances_transaction_id_fkey FOREIGN KEY (transaction_id) REFERENCES public.transactions(id) ON DELETE SET NULL;


--
-- Name: recurring_template_tags recurring_template_tags_recurring_template_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_template_tags
    ADD CONSTRAINT recurring_template_tags_recurring_template_id_fkey FOREIGN KEY (recurring_template_id) REFERENCES public.recurring_templates(id) ON DELETE CASCADE;


--
-- Name: recurring_template_tags recurring_template_tags_tag_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_template_tags
    ADD CONSTRAINT recurring_template_tags_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES public.tags(id) ON DELETE CASCADE;


--
-- Name: recurring_templates recurring_templates_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_templates
    ADD CONSTRAINT recurring_templates_category_id_fkey FOREIGN KEY (category_id) REFERENCES public.categories(id) ON DELETE SET NULL;


--
-- Name: recurring_templates recurring_templates_financial_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_templates
    ADD CONSTRAINT recurring_templates_financial_product_id_fkey FOREIGN KEY (financial_product_id) REFERENCES public.financial_products(id) ON DELETE SET NULL;


--
-- Name: recurring_templates recurring_templates_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recurring_templates
    ADD CONSTRAINT recurring_templates_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: source_documents source_documents_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.source_documents
    ADD CONSTRAINT source_documents_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: statement_cycles statement_cycles_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statement_cycles
    ADD CONSTRAINT statement_cycles_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.cards(id) ON DELETE CASCADE;


--
-- Name: statement_items statement_items_statement_cycle_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statement_items
    ADD CONSTRAINT statement_items_statement_cycle_id_fkey FOREIGN KEY (statement_cycle_id) REFERENCES public.statement_cycles(id) ON DELETE CASCADE;


--
-- Name: statement_items statement_items_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statement_items
    ADD CONSTRAINT statement_items_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: statement_items statement_items_transaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.statement_items
    ADD CONSTRAINT statement_items_transaction_id_fkey FOREIGN KEY (transaction_id) REFERENCES public.transactions(id) ON DELETE SET NULL;


--
-- Name: supplier_aliases supplier_aliases_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_aliases
    ADD CONSTRAINT supplier_aliases_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE CASCADE;


--
-- Name: supplier_contracts supplier_contracts_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_contracts
    ADD CONSTRAINT supplier_contracts_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE CASCADE;


--
-- Name: supplier_tags supplier_tags_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_tags
    ADD CONSTRAINT supplier_tags_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE CASCADE;


--
-- Name: supplier_tags supplier_tags_tag_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.supplier_tags
    ADD CONSTRAINT supplier_tags_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES public.tags(id) ON DELETE CASCADE;


--
-- Name: suppliers suppliers_institution_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.suppliers
    ADD CONSTRAINT suppliers_institution_id_fkey FOREIGN KEY (institution_id) REFERENCES public.institutions(id) ON DELETE SET NULL;


--
-- Name: transaction_tags transaction_tags_tag_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transaction_tags
    ADD CONSTRAINT transaction_tags_tag_id_fkey FOREIGN KEY (tag_id) REFERENCES public.tags(id) ON DELETE CASCADE;


--
-- Name: transaction_tags transaction_tags_transaction_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transaction_tags
    ADD CONSTRAINT transaction_tags_transaction_id_fkey FOREIGN KEY (transaction_id) REFERENCES public.transactions(id) ON DELETE CASCADE;


--
-- Name: transactions transactions_category_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_category_id_fkey FOREIGN KEY (category_id) REFERENCES public.categories(id) ON DELETE SET NULL;


--
-- Name: transactions transactions_financial_period_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_financial_period_id_fkey FOREIGN KEY (financial_period_id) REFERENCES public.financial_periods(id) ON DELETE SET NULL;


--
-- Name: transactions transactions_financial_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_financial_product_id_fkey FOREIGN KEY (financial_product_id) REFERENCES public.financial_products(id) ON DELETE CASCADE;


--
-- Name: transactions transactions_liability_installment_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_liability_installment_id_fkey FOREIGN KEY (liability_installment_id) REFERENCES public.liability_installments(id) ON DELETE SET NULL;


--
-- Name: transactions transactions_recurring_instance_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_recurring_instance_id_fkey FOREIGN KEY (recurring_instance_id) REFERENCES public.recurring_instances(id) ON DELETE SET NULL;


--
-- Name: transactions transactions_source_document_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_source_document_id_fkey FOREIGN KEY (source_document_id) REFERENCES public.source_documents(id) ON DELETE SET NULL;


--
-- Name: transactions transactions_statement_cycle_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_statement_cycle_id_fkey FOREIGN KEY (statement_cycle_id) REFERENCES public.statement_cycles(id) ON DELETE SET NULL;


--
-- Name: transactions transactions_supplier_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_supplier_id_fkey FOREIGN KEY (supplier_id) REFERENCES public.suppliers(id) ON DELETE SET NULL;


--
-- Name: transfers transfers_financial_period_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfers
    ADD CONSTRAINT transfers_financial_period_id_fkey FOREIGN KEY (financial_period_id) REFERENCES public.financial_periods(id) ON DELETE SET NULL;


--
-- Name: transfers transfers_source_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfers
    ADD CONSTRAINT transfers_source_product_id_fkey FOREIGN KEY (source_product_id) REFERENCES public.financial_products(id) ON DELETE CASCADE;


--
-- Name: transfers transfers_target_product_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfers
    ADD CONSTRAINT transfers_target_product_id_fkey FOREIGN KEY (target_product_id) REFERENCES public.financial_products(id) ON DELETE CASCADE;

