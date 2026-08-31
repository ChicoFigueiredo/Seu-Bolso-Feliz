-- ADR-009 — saída do backend anterior: identidade, fila de jobs e Centros.
-- Idempotente: pode rodar duas vezes sem erro.

BEGIN;

-- ─────────────────────────────────────────────────────────────────────────
-- 1. pgcrypto — encrypt_secret/decrypt_secret já existem no Neon e chamam
--    pgp_sym_encrypt, mas a extensão nunca foi instalada. Sem isto, guardar
--    senha de fatura em PDF falha na primeira tentativa.
-- ─────────────────────────────────────────────────────────────────────────
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE SCHEMA IF NOT EXISTS private;

-- ─────────────────────────────────────────────────────────────────────────
-- 2. Identidade própria (substitui o auth.users do backend anterior).
--    Dois usuários: o dono (entra pelo Google) e o admin de emergência (entra
--    por e-mail/senha). O admin opera sobre os dados do dono — por isso
--    owner_id: quem você é e de quem são os dados são coisas separadas.
-- ─────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.users (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email              text NOT NULL UNIQUE,
    name               text,
    google_sub         text UNIQUE,
    password_hash      text,
    role               text NOT NULL DEFAULT 'owner',
    owner_id           uuid REFERENCES public.users (id) ON DELETE RESTRICT,
    failed_login_count integer NOT NULL DEFAULT 0,
    locked_until       timestamptz,
    last_login_at      timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_role_check CHECK (role IN ('owner', 'admin')),
    -- Um usuário precisa de pelo menos um meio de entrar.
    CONSTRAINT users_tem_credencial CHECK (google_sub IS NOT NULL OR password_hash IS NOT NULL),
    -- Admin sempre opera sobre os dados de alguém; dono opera sobre os seus.
    CONSTRAINT users_admin_tem_dono CHECK (role <> 'admin' OR owner_id IS NOT NULL)
);

DROP TRIGGER IF EXISTS users_set_updated_at ON public.users;
CREATE TRIGGER users_set_updated_at BEFORE UPDATE ON public.users
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

-- ─────────────────────────────────────────────────────────────────────────
-- 3. Fila de jobs. A Vercel grava aqui; os workers puxam com SKIP LOCKED e
--    acordam por NOTIFY. Nenhuma porta é exposta em lugar nenhum.
-- ─────────────────────────────────────────────────────────────────────────
ALTER TABLE public.ingestion_jobs
    ADD COLUMN IF NOT EXISTS locked_by    text,
    ADD COLUMN IF NOT EXISTS locked_at    timestamptz,
    ADD COLUMN IF NOT EXISTS requires     text[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS available_at timestamptz NOT NULL DEFAULT now();

-- Índice do caminho quente: "me dê o próximo job que eu consigo executar".
CREATE INDEX IF NOT EXISTS ix_ingestion_jobs_fila
    ON public.ingestion_jobs (available_at, created_at)
    WHERE locked_at IS NULL;

-- Reaper: achar job preso porque o worker morreu.
CREATE INDEX IF NOT EXISTS ix_ingestion_jobs_travados
    ON public.ingestion_jobs (locked_at)
    WHERE locked_at IS NOT NULL;

CREATE OR REPLACE FUNCTION public.fn_notifica_job_novo() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('sbf_jobs', NEW.id::text);
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS ingestion_jobs_notifica ON public.ingestion_jobs;
CREATE TRIGGER ingestion_jobs_notifica AFTER INSERT ON public.ingestion_jobs
    FOR EACH ROW EXECUTE FUNCTION public.fn_notifica_job_novo();

-- ─────────────────────────────────────────────────────────────────────────
-- 4. Centros e rateio. Um Centro é "para quem/para qual projeto o dinheiro
--    foi" — tem saldo, entradas e saídas. Uma movimentação real gera N
--    rateios analíticos SEM duplicar o lançamento.
-- ─────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.financial_centers (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL,
    name        text NOT NULL,
    type        text NOT NULL DEFAULT 'projeto',
    description text,
    status      text NOT NULL DEFAULT 'active',
    icon        text,
    color       text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT financial_centers_type_check   CHECK (type IN ('projeto', 'pessoa', 'negocio')),
    CONSTRAINT financial_centers_status_check CHECK (status IN ('active', 'archived')),
    CONSTRAINT financial_centers_nome_unico   UNIQUE (user_id, name)
);

DROP TRIGGER IF EXISTS financial_centers_set_updated_at ON public.financial_centers;
CREATE TRIGGER financial_centers_set_updated_at BEFORE UPDATE ON public.financial_centers
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TABLE IF NOT EXISTS public.transaction_allocations (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             uuid NOT NULL,
    transaction_id      uuid NOT NULL REFERENCES public.transactions (id) ON DELETE CASCADE,
    financial_center_id uuid NOT NULL REFERENCES public.financial_centers (id) ON DELETE RESTRICT,
    amount              numeric(14, 2) NOT NULL,
    note                text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT transaction_allocations_amount_positivo CHECK (amount > 0),
    CONSTRAINT transaction_allocations_um_por_centro   UNIQUE (transaction_id, financial_center_id)
);

CREATE INDEX IF NOT EXISTS ix_transaction_allocations_centro
    ON public.transaction_allocations (financial_center_id);
CREATE INDEX IF NOT EXISTS ix_transaction_allocations_transacao
    ON public.transaction_allocations (transaction_id);

-- ─────────────────────────────────────────────────────────────────────────
-- 5. Guarda de integridade do rateio. A regra "a soma dos rateios nunca
--    excede o valor da movimentação" é um invariante de dados, não uma
--    validação de formulário: se ela vazar, o gasto de um Centro fica
--    inflado e ninguém percebe por semanas. A validação em TypeScript
--    continua existindo (mensagem melhor, falha mais cedo) — isto aqui é a
--    rede embaixo dela.
--
--    Pagar fatura de cartão não rateia: as compras dentro da fatura já
--    foram rateadas, e ratear de novo dobraria o gasto do Centro.
-- ─────────────────────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.fn_valida_rateio() RETURNS trigger
    LANGUAGE plpgsql AS $$
DECLARE
    v_valor   numeric(14, 2);
    v_tipo    text;
    v_somado  numeric(14, 2);
    v_transacao uuid := COALESCE(NEW.transaction_id, OLD.transaction_id);
BEGIN
    SELECT abs(t.amount), t.type INTO v_valor, v_tipo
      FROM public.transactions t WHERE t.id = v_transacao;

    IF v_tipo = 'statement_payment' THEN
        RAISE EXCEPTION 'pagamento de fatura não aceita rateio: as compras da fatura já foram rateadas'
            USING ERRCODE = 'check_violation';
    END IF;

    SELECT COALESCE(sum(a.amount), 0) INTO v_somado
      FROM public.transaction_allocations a WHERE a.transaction_id = v_transacao;

    IF v_somado > v_valor THEN
        RAISE EXCEPTION 'a soma dos rateios (%) excede o valor da movimentação (%)', v_somado, v_valor
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS transaction_allocations_valida ON public.transaction_allocations;
CREATE CONSTRAINT TRIGGER transaction_allocations_valida
    AFTER INSERT OR UPDATE ON public.transaction_allocations
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION public.fn_valida_rateio();

COMMIT;
