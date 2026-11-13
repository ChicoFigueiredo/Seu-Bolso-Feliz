-- Spec 07 (fatia 2 do desktop), ADR-013: as estações e as alterações.
-- Idempotente: pode rodar duas vezes sem erro. Vai ao Neon só pelo portão humano
-- (ADR-007), junto com a implantação do `sbf servidor`.

BEGIN;

-- ─────────────────────────────────────────────────────────────────────────
-- 1. Estações. Cada instalação do desktop tem uma credencial própria (grill Q3):
--    um token opaco de 256 bits que só a estação conhece; aqui fica só o
--    SHA-256 dele. Revogar apaga o hash e marca `revogada_em`: a credencial
--    nunca mais entra, e o servidor ainda sabe responder "estação revogada"
--    porque o token carrega o id da estação.
--    `user_id` é o DONO dos dados (o filtro de tudo); `pareada_por` é quem
--    fez o login Google (o dono ou o admin que opera sobre os dados dele).
-- ─────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.estacoes (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           uuid NOT NULL REFERENCES public.users (id) ON DELETE RESTRICT,
    pareada_por       uuid NOT NULL REFERENCES public.users (id) ON DELETE RESTRICT,
    nome              text NOT NULL,
    sistema           text NOT NULL,
    credencial_sha256 bytea,
    contrato          integer NOT NULL,
    pareada_em        timestamptz NOT NULL DEFAULT now(),
    ultimo_contato    timestamptz,
    revogada_em       timestamptz,
    CONSTRAINT estacoes_hash_de_256_bits CHECK (credencial_sha256 IS NULL OR octet_length(credencial_sha256) = 32),
    CONSTRAINT estacoes_revogada_sem_hash CHECK ((revogada_em IS NULL) = (credencial_sha256 IS NOT NULL)),
    CONSTRAINT estacoes_nome_preenchido CHECK (length(btrim(nome)) BETWEEN 1 AND 120),
    CONSTRAINT estacoes_sistema_preenchido CHECK (length(btrim(sistema)) BETWEEN 1 AND 60)
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_estacoes_credencial
    ON public.estacoes (credencial_sha256) WHERE credencial_sha256 IS NOT NULL;
CREATE INDEX IF NOT EXISTS ix_estacoes_dono ON public.estacoes (user_id);

-- ─────────────────────────────────────────────────────────────────────────
-- 2. Alterações recebidas (grill Q6/Q7). Uma linha por proposta, aceita ou
--    recusada, com a resposta dada: reenviar o mesmo `id` devolve a mesma
--    resposta (a estação repete depois de um corte de rede sem virar recusa
--    contra a própria versão). Só acréscimo.
-- ─────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.alteracoes (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES public.users (id) ON DELETE RESTRICT,
    estacao_id  uuid NOT NULL REFERENCES public.estacoes (id) ON DELETE RESTRICT,
    acao        text NOT NULL,
    resultado   text NOT NULL,
    proposta    jsonb NOT NULL,
    resposta    jsonb NOT NULL,
    recebida_em timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT alteracoes_resultado_check CHECK (resultado IN ('aceita', 'recusada'))
);

CREATE INDEX IF NOT EXISTS ix_alteracoes_dono ON public.alteracoes (user_id, recebida_em);
CREATE INDEX IF NOT EXISTS ix_alteracoes_estacao ON public.alteracoes (estacao_id, recebida_em);

COMMIT;
