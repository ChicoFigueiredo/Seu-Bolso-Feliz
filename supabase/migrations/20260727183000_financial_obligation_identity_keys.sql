-- P0-7: liga financial_obligations ao pipeline.
--
-- As tabelas financial_obligations e financial_obligation_evidences existiam
-- desde 2026-05-24 e NENHUMA linha de TypeScript jamais as leu ou escreveu.
-- Esta migration cria o que faltava para elas entrarem no fluxo.
--
-- Decisão de modelagem: obrigações ficam AO LADO de draft_records, a montante,
-- e não no lugar delas.
--   financial_obligations = "o que eu devo?"  (fato do mundo real, deduplicado
--                                              entre documentos)
--   draft_records         = "o que escrever no ledger?" (proposta por tabela
--                                              alvo, revisada por humano)
-- Uma obrigação tem N evidências e 1..N drafts. Colapsar as duas destruiria a
-- relação N:1 para a qual financial_obligation_evidences existe.

-- ── Aliases de identidade ───────────────────────────────────────────────────
-- financial_obligations.financial_identity_key guarda apenas a chave primária.
-- Como um documento pode emitir várias chaves computáveis (código de barras,
-- número do documento, fornecedor+mês+valor...), o casamento acontece contra
-- esta tabela: qualquer chave que bata identifica a mesma obrigação.

CREATE TABLE IF NOT EXISTS financial_obligation_identity_keys (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  obligation_id uuid NOT NULL REFERENCES financial_obligations(id) ON DELETE CASCADE,
  key text NOT NULL,
  key_kind text NOT NULL CHECK (key_kind IN (
    'barcode', 'docnum', 'card_cycle', 'supplier_period_amount', 'supplier_period'
  )),
  strength text NOT NULL CHECK (strength IN ('strong', 'medium', 'weak')),
  created_at timestamptz NOT NULL DEFAULT now()
);

-- Uma chave pertence a exatamente uma obrigação por usuário.
CREATE UNIQUE INDEX IF NOT EXISTS uq_fo_identity_keys
  ON financial_obligation_identity_keys (user_id, key);

CREATE INDEX IF NOT EXISTS ix_fo_identity_keys_obligation
  ON financial_obligation_identity_keys (obligation_id);

ALTER TABLE financial_obligation_identity_keys ENABLE ROW LEVEL SECURITY;

CREATE POLICY "fo_identity_keys_user_policy"
  ON financial_obligation_identity_keys
  FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

CREATE POLICY "fo_identity_keys_service_role"
  ON financial_obligation_identity_keys
  FOR ALL
  TO service_role
  USING (true)
  WITH CHECK (true);

-- ── Upsert atômico ──────────────────────────────────────────────────────────
-- Precisa ser uma RPC: procurar a chave, criar a obrigação, gravar os aliases e
-- anexar a evidência são quatro passos que, feitos do cliente, permitiriam que
-- dois workers drenando o mesmo lote criassem duas obrigações para a mesma conta.

CREATE OR REPLACE FUNCTION fn_upsert_financial_obligation(
  p_user_id  uuid,
  p_payload  jsonb,
  p_keys     jsonb,
  p_evidence jsonb
) RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
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
  IF auth.uid() IS NOT NULL AND auth.uid() <> p_user_id THEN
    RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';
  END IF;

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

REVOKE ALL ON FUNCTION fn_upsert_financial_obligation(uuid, jsonb, jsonb, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fn_upsert_financial_obligation(uuid, jsonb, jsonb, jsonb)
  TO authenticated, service_role;

COMMENT ON FUNCTION fn_upsert_financial_obligation(uuid, jsonb, jsonb, jsonb) IS
  'Encontra ou cria a obrigação canônica de uma evidência, gravando aliases de identidade e o vínculo da evidência atomicamente.';

-- rollback:
--   DROP FUNCTION IF EXISTS fn_upsert_financial_obligation(uuid, jsonb, jsonb, jsonb);
--   DROP TABLE IF EXISTS financial_obligation_identity_keys;
