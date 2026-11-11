-- Porta `private.get_crypto_key` para o Neon e semeia os dois usuários.
--
-- A Fase 1 da ADR-009 portou `encrypt_secret`/`decrypt_secret` mas deixou
-- explicitamente de fora a função que fornece a chave (ver a NOTA no topo de
-- packages/db/schema.sql). Resultado: guardar senha de fatura em PDF quebra
-- na primeira chamada. Isto fecha o buraco.

BEGIN;

REVOKE ALL ON SCHEMA private FROM PUBLIC;

CREATE TABLE IF NOT EXISTS private.crypto_keys (
    version    integer PRIMARY KEY,
    key        text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    retired_at timestamptz
);
REVOKE ALL ON TABLE private.crypto_keys FROM PUBLIC;

COMMENT ON TABLE private.crypto_keys IS
    'Chaves de criptografia dos segredos. A chave nasce dentro do banco e nunca sai dele — '
    'um dump que exclua o schema `private` torna todo segredo indecifrável para sempre.';

INSERT INTO private.crypto_keys (version, key)
SELECT 1, encode(gen_random_bytes(32), 'base64')
WHERE NOT EXISTS (SELECT 1 FROM private.crypto_keys WHERE version = 1);

CREATE OR REPLACE FUNCTION private.get_crypto_key(p_version integer DEFAULT NULL)
RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = private, public AS $$
  SELECT key FROM private.crypto_keys
   WHERE version = COALESCE(p_version,
                            (SELECT max(version) FROM private.crypto_keys WHERE retired_at IS NULL))
   LIMIT 1;
$$;
REVOKE ALL ON FUNCTION private.get_crypto_key(integer) FROM PUBLIC;

-- ─────────────────────────────────────────────────────────────────────────
-- Usuários. O dono nasce sem credencial: o `google_sub` é preenchido no
-- primeiro login pelo Google, casando por e-mail. Uma linha sem credencial
-- não consegue entrar — é inerte, não é um buraco.
-- ─────────────────────────────────────────────────────────────────────────
ALTER TABLE public.users DROP CONSTRAINT IF EXISTS users_tem_credencial;

INSERT INTO public.users (email, name, role)
SELECT 'fran.fig@gmail.com', 'Chico Figueiredo', 'owner'
WHERE NOT EXISTS (SELECT 1 FROM public.users WHERE email = 'fran.fig@gmail.com');

-- O admin de emergência opera sobre os dados do dono. A senha é definida no
-- primeiro boot a partir de ADMIN_INITIAL_PASSWORD e gravada aqui — assim
-- trocá-la depois não exige redeploy.
INSERT INTO public.users (email, name, role, owner_id)
SELECT 'admin@seubolsofeliz.com.br', 'Administrador', 'admin', o.id
  FROM public.users o
 WHERE o.email = 'fran.fig@gmail.com'
   AND NOT EXISTS (SELECT 1 FROM public.users WHERE email = 'admin@seubolsofeliz.com.br');

COMMIT;
