-- Migration: Encriptação de segredos com pgcrypto
-- Gap C3 identificado em auditoria 2026-03-22
-- encrypted_value na tabela user_secrets era coluna text puro (sem encriptação real).
-- Solução: helpers pgcrypto + coluna de versão para facilitar rotação de chave.

-- Garantir extensão pgcrypto disponível (deve estar instalada)
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Adicionar coluna de versão de encriptação (facilita rotação futura de chave)
ALTER TABLE user_secrets
  ADD COLUMN IF NOT EXISTS encryption_version integer NOT NULL DEFAULT 1;

-- Adicionar índice de verificação de que o user possui o segredo
COMMENT ON COLUMN user_secrets.encrypted_value IS
  'Valor encriptado com pgp_sym_encrypt(value, app.encryption_key).
   Use as funções encrypt_secret() e decrypt_secret() para manipular.
   Nunca armazene texto puro nesta coluna.';

COMMENT ON COLUMN user_secrets.encryption_version IS
  'Versão da chave de encriptação usada. Incrementar ao rotacionar a chave de encriptação.';

-- Função helper para encriptação (SECURITY DEFINER para acesso à config da app)
CREATE OR REPLACE FUNCTION encrypt_secret(plaintext text)
RETURNS text
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  encryption_key text;
BEGIN
  -- Obtém a chave da configuração da sessão. Deve ser definida pela Edge Function
  -- antes de chamar: SET LOCAL app.encryption_key = '<chave>';
  encryption_key := current_setting('app.encryption_key', true);

  IF encryption_key IS NULL OR encryption_key = '' THEN
    RAISE EXCEPTION 'app.encryption_key não configurada — não é possível encriptar segredos'
      USING ERRCODE = 'configuration_error';
  END IF;

  RETURN encode(
    pgp_sym_encrypt(plaintext, encryption_key),
    'base64'
  );
END;
$$;

-- Função helper para decriptação
CREATE OR REPLACE FUNCTION decrypt_secret(ciphertext text)
RETURNS text
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
  encryption_key text;
BEGIN
  encryption_key := current_setting('app.encryption_key', true);

  IF encryption_key IS NULL OR encryption_key = '' THEN
    RAISE EXCEPTION 'app.encryption_key não configurada — não é possível decriptar segredos'
      USING ERRCODE = 'configuration_error';
  END IF;

  RETURN pgp_sym_decrypt(
    decode(ciphertext, 'base64'),
    encryption_key
  );
EXCEPTION
  WHEN OTHERS THEN
    RAISE EXCEPTION 'Falha ao decriptar segredo: chave incorreta ou dado corrompido'
      USING ERRCODE = 'data_exception';
END;
$$;

-- RLS: garantir que apenas o próprio usuário acessa seus segredos
-- (já deve existir na migration 170500, esta é verificação defensiva)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_policies
    WHERE tablename = 'user_secrets' AND policyname = 'Users can manage own user_secrets'
  ) THEN
    CREATE POLICY "Users can manage own user_secrets"
      ON user_secrets FOR ALL
      USING (auth.uid() = user_id)
      WITH CHECK (auth.uid() = user_id);
  END IF;
END;
$$;
