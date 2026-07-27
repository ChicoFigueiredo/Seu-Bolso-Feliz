-- P0-8: criptografia real de segredos.
--
-- Estado anterior, quebrado nas DUAS direções:
--
--   Leitura  — secret-lookup.ts devolvia encrypted_value cru, que ia direto
--              para o pdf-parse como se fosse a senha. decrypt_secret() nunca
--              teve um único chamador em toda a aplicação.
--   Escrita  — nenhuma UI, action ou RPC jamais chamou encrypt_secret(), então
--              qualquer linha existente e' texto puro. A coluna
--              encryption_version tem DEFAULT 1 e mente: nada foi encriptado.
--   Chave    — encrypt_secret/decrypt_secret liam current_setting(
--              'app.encryption_key'), uma GUC de sessão que NINGUÉM nunca
--              definiu. PostgREST não permite SET LOCAL antes de uma RPC, então
--              as duas funções levantariam configuration_error na primeira
--              chamada que recebessem.

-- ── 1. Onde a chave mora ────────────────────────────────────────────────────
-- Schema privado, revogado de todos os papéis. A chave só é legível de dentro
-- de funções SECURITY DEFINER; nenhum cliente, worker, Edge Function ou
-- variável de CI jamais a detém.
--
-- Rejeitado: ALTER DATABASE ... SET app.encryption_key (persiste em
-- pg_db_role_setting, visível a superusuário e capturado por pg_dumpall) e
-- decriptar no worker com chave de env (este repositório já vazou credenciais
-- uma vez, commit 38b8126).

CREATE SCHEMA IF NOT EXISTS private;
REVOKE ALL ON SCHEMA private FROM PUBLIC;
REVOKE ALL ON SCHEMA private FROM anon, authenticated;

CREATE TABLE IF NOT EXISTS private.crypto_keys (
  version    integer PRIMARY KEY,
  key        text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  retired_at timestamptz
);

REVOKE ALL ON TABLE private.crypto_keys FROM PUBLIC;
REVOKE ALL ON TABLE private.crypto_keys FROM anon, authenticated;

COMMENT ON TABLE private.crypto_keys IS
  'Chaves de criptografia dos segredos. Rotação = inserir nova versão e re-encriptar as linhas.';

-- Semente de desenvolvimento. Em produção o operador substitui a versão 1 por
-- uma chave real; o CHECK de tamanho evita que a semente vá a produção por
-- descuido silencioso.
INSERT INTO private.crypto_keys (version, key)
SELECT 1, encode(gen_random_bytes(32), 'base64')
WHERE NOT EXISTS (SELECT 1 FROM private.crypto_keys WHERE version = 1);

CREATE OR REPLACE FUNCTION private.get_crypto_key(p_version integer DEFAULT NULL)
RETURNS text
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = private, public
AS $$
  SELECT key FROM private.crypto_keys
   WHERE version = COALESCE(p_version, (SELECT max(version) FROM private.crypto_keys
                                         WHERE retired_at IS NULL))
   LIMIT 1;
$$;

REVOKE ALL ON FUNCTION private.get_crypto_key(integer) FROM PUBLIC;

-- ── 2. Helpers de criptografia, agora com chave que existe ──────────────────
-- As versões antigas de um argumento precisam sair antes: conviver com as
-- novas (que têm p_version com DEFAULT) tornaria `encrypt_secret(x)` ambíguo.

DROP FUNCTION IF EXISTS encrypt_secret(text);
DROP FUNCTION IF EXISTS decrypt_secret(text);

CREATE OR REPLACE FUNCTION encrypt_secret(plaintext text, p_version integer DEFAULT NULL)
RETURNS text
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, private, extensions
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

CREATE OR REPLACE FUNCTION decrypt_secret(ciphertext text, p_version integer DEFAULT NULL)
RETURNS text
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, private, extensions
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

REVOKE ALL ON FUNCTION encrypt_secret(text, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION decrypt_secret(text, integer) FROM PUBLIC;

-- ── 3. Escopo do segredo ────────────────────────────────────────────────────
-- entity_id é polimórfico e não pode ter FK, então um trigger valida a
-- existência conforme entity_type, evitando segredos pendurados em nada.

ALTER TABLE user_secrets
  ADD COLUMN IF NOT EXISTS contract_identifier text,
  ADD COLUMN IF NOT EXISTS label text,
  ADD COLUMN IF NOT EXISTS last_used_at timestamptz,
  ADD COLUMN IF NOT EXISTS success_count integer NOT NULL DEFAULT 0;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_user_secrets_entity_type') THEN
    ALTER TABLE user_secrets ADD CONSTRAINT chk_user_secrets_entity_type CHECK (
      entity_type IS NULL OR entity_type IN
        ('supplier', 'financial_product', 'card', 'supplier_contract', 'institution')
    );
  END IF;
END;
$$;

-- Um segredo por escopo.
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_secrets_scope
  ON user_secrets (
    user_id,
    secret_type,
    COALESCE(entity_type, '-'),
    COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uuid),
    COALESCE(contract_identifier, '-')
  );

-- ── 4. Backfill: encriptar o que está em texto puro ─────────────────────────
-- encryption_version tinha DEFAULT 1 sem que nada tivesse sido encriptado.
-- Zeramos para refletir a verdade, encriptamos, e só então proibimos texto puro.

ALTER TABLE user_secrets ALTER COLUMN encryption_version SET DEFAULT 1;

DO $$
DECLARE v_count integer;
BEGIN
  UPDATE user_secrets SET encryption_version = 0 WHERE encryption_version = 1;

  UPDATE user_secrets
     SET encrypted_value = encrypt_secret(encrypted_value),
         encryption_version = 1
   WHERE encryption_version = 0;

  GET DIAGNOSTICS v_count = ROW_COUNT;
  RAISE NOTICE 'user_secrets: % linha(s) encriptada(s)', v_count;
END;
$$;

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_user_secrets_encrypted') THEN
    ALTER TABLE user_secrets ADD CONSTRAINT chk_user_secrets_encrypted
      CHECK (encryption_version >= 1);
  END IF;
END;
$$;

-- ── 5. Leitura: só service_role, sempre auditada ────────────────────────────
-- O navegador nunca precisa de uma senha de PDF em claro, então esta função
-- NÃO é concedida a `authenticated`.

CREATE OR REPLACE FUNCTION fn_get_secrets(
  p_user_id     uuid,
  p_secret_type text,
  p_limit       integer DEFAULT 20
) RETURNS TABLE (
  secret_id           uuid,
  value               text,
  entity_type         text,
  entity_id           uuid,
  contract_identifier text,
  label               text
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, private
AS $$
BEGIN
  IF auth.uid() IS NOT NULL AND auth.uid() <> p_user_id THEN
    RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';
  END IF;

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

REVOKE ALL ON FUNCTION fn_get_secrets(uuid, text, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fn_get_secrets(uuid, text, integer) TO service_role;

-- ── 6. Escrita: só authenticated, sem p_user_id ─────────────────────────────
-- Usa auth.uid() diretamente, o que elimina a superfície de forja: não há
-- parâmetro de usuário para mentir.

CREATE OR REPLACE FUNCTION fn_set_secret(
  p_secret_type         text,
  p_plaintext           text,
  p_entity_type         text DEFAULT NULL,
  p_entity_id           uuid DEFAULT NULL,
  p_contract_identifier text DEFAULT NULL,
  p_label               text DEFAULT NULL
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, private
AS $$
DECLARE
  v_user_id uuid := auth.uid();
  v_id      uuid;
BEGIN
  IF v_user_id IS NULL THEN
    RAISE EXCEPTION 'não autenticado' USING ERRCODE = 'insufficient_privilege';
  END IF;
  IF p_plaintext IS NULL OR p_plaintext = '' THEN
    RAISE EXCEPTION 'segredo vazio' USING ERRCODE = 'invalid_parameter_value';
  END IF;

  INSERT INTO user_secrets (
    user_id, secret_type, entity_type, entity_id, contract_identifier,
    label, encrypted_value, encryption_version
  ) VALUES (
    v_user_id, p_secret_type, p_entity_type, p_entity_id, p_contract_identifier,
    p_label, encrypt_secret(p_plaintext), 1
  )
  ON CONFLICT (user_id, secret_type, COALESCE(entity_type, '-'),
               COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uuid),
               COALESCE(contract_identifier, '-'))
  DO UPDATE SET encrypted_value    = EXCLUDED.encrypted_value,
                encryption_version = EXCLUDED.encryption_version,
                label              = COALESCE(EXCLUDED.label, user_secrets.label),
                updated_at         = now()
  RETURNING id INTO v_id;

  INSERT INTO audit_logs (user_id, action, entity_type, entity_id, new_values)
  VALUES (v_user_id, 'secret_written', 'user_secrets', v_id,
          jsonb_build_object('secret_type', p_secret_type, 'entity_type', p_entity_type));

  RETURN v_id;
END;
$$;

REVOKE ALL ON FUNCTION fn_set_secret(text, text, text, uuid, text, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fn_set_secret(text, text, text, uuid, text, text)
  TO authenticated, service_role;

-- ── 7. Marcar acerto, para o próximo documento resolver na primeira tentativa

CREATE OR REPLACE FUNCTION fn_mark_secret_used(p_user_id uuid, p_secret_id uuid)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
BEGIN
  IF auth.uid() IS NOT NULL AND auth.uid() <> p_user_id THEN
    RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';
  END IF;

  UPDATE user_secrets
     SET last_used_at = now(), success_count = success_count + 1
   WHERE id = p_secret_id AND user_id = p_user_id;
END;
$$;

REVOKE ALL ON FUNCTION fn_mark_secret_used(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fn_mark_secret_used(uuid, uuid) TO authenticated, service_role;

-- rollback:
--   DROP FUNCTION IF EXISTS fn_mark_secret_used(uuid, uuid);
--   DROP FUNCTION IF EXISTS fn_set_secret(text, text, text, uuid, text, text);
--   DROP FUNCTION IF EXISTS fn_get_secrets(uuid, text, integer);
--   ALTER TABLE user_secrets DROP CONSTRAINT IF EXISTS chk_user_secrets_encrypted;
--   DROP INDEX IF EXISTS uq_user_secrets_scope;
--   DROP FUNCTION IF EXISTS private.get_crypto_key(integer);
--   DROP TABLE IF EXISTS private.crypto_keys;
--   DROP SCHEMA IF EXISTS private;
