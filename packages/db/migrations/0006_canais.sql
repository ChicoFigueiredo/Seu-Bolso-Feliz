-- ADR-015, ADR-017: canais de aviso (Telegram) e o registro do que saiu.
-- Idempotente: pode rodar duas vezes sem erro. Vai ao Neon só pelo portão humano
-- (ADR-007), junto com a implantação do `sbf canais`.

BEGIN;

-- 1. Destinatários vinculados (um chat do Telegram). `nivel_de_confianca` nasce
--    mascarado (ADR-015); só o dono sobe o nível, por `sbf canais nivel`.
CREATE TABLE IF NOT EXISTS public.canais_destinatarios (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            uuid NOT NULL REFERENCES public.users (id) ON DELETE CASCADE,
    canal              text NOT NULL,
    identificador      text NOT NULL,
    nivel_de_confianca text NOT NULL DEFAULT 'mascarado',
    vinculado_em       timestamptz NOT NULL DEFAULT now(),
    revogado_em        timestamptz,
    CONSTRAINT canais_destinatarios_canal CHECK (canal IN ('telegram')),
    CONSTRAINT canais_destinatarios_nivel CHECK (nivel_de_confianca IN ('mascarado', 'parcial', 'completo')),
    CONSTRAINT canais_destinatarios_identificador CHECK (length(btrim(identificador)) BETWEEN 1 AND 64)
);
CREATE UNIQUE INDEX IF NOT EXISTS ux_canais_destinatarios
    ON public.canais_destinatarios (canal, identificador);
CREATE INDEX IF NOT EXISTS ix_canais_destinatarios_dono
    ON public.canais_destinatarios (user_id);

-- 2. Códigos de vinculação de uso único. Só o SHA-256 do código fica aqui.
CREATE TABLE IF NOT EXISTS public.canais_codigos (
    codigo_sha256 bytea PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES public.users (id) ON DELETE CASCADE,
    canal         text NOT NULL,
    criado_em     timestamptz NOT NULL DEFAULT now(),
    expira_em     timestamptz NOT NULL,
    usado_em      timestamptz,
    CONSTRAINT canais_codigos_hash CHECK (octet_length(codigo_sha256) = 32)
);

-- 3. O último estado avisado de cada alerta por destinatário: um alerta não repete
--    até o estado mudar. A linha some quando o alerta deixa de valer.
CREATE TABLE IF NOT EXISTS public.canais_alertas_enviados (
    user_id       uuid NOT NULL REFERENCES public.users (id) ON DELETE CASCADE,
    canal         text NOT NULL,
    identificador text NOT NULL,
    chave         text NOT NULL,
    estado        text NOT NULL,
    enviado_em    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (canal, identificador, chave)
);

-- 4. O que saiu (ADR-015): para quem, de que tipo, quanto e quando. Nunca o texto.
CREATE TABLE IF NOT EXISTS public.canais_saidas (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            uuid NOT NULL REFERENCES public.users (id) ON DELETE CASCADE,
    canal              text NOT NULL,
    identificador      text NOT NULL,
    tipo               text NOT NULL,
    nivel_de_confianca text NOT NULL,
    tamanho_bytes      integer NOT NULL,
    entregue           boolean NOT NULL,
    enviado_em         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ix_canais_saidas_dono_data
    ON public.canais_saidas (user_id, enviado_em DESC);

-- 5. Pequeno estado do serviço (offset do getUpdates, última agenda rodada).
CREATE TABLE IF NOT EXISTS public.canais_estado (
    chave         text PRIMARY KEY,
    valor         text NOT NULL,
    atualizado_em timestamptz NOT NULL DEFAULT now()
);

COMMIT;
