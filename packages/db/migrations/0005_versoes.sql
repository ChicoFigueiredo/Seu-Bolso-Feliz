-- Spec 08 (fatia 3 do desktop): versões de linha e registro de exclusões, para o delta
-- por cursor e para a guarda das alterações.
-- Idempotente: pode rodar duas vezes sem erro. Vai ao Neon só pelo portão humano
-- (ADR-007), junto com a implantação do `sbf servidor` v2.

BEGIN;

-- ─────────────────────────────────────────────────────────────────────────
-- 1. `updated_at` que se mantém sozinho onde faltava gatilho. A versão de base de uma
--    alteração é o `updated_at` da linha (spec 07, Q7): sem o gatilho, uma edição do
--    pipeline Bun não mudaria a versão e a estação não perceberia.
--    - source_documents, draft_batches, draft_records: tinham a coluna (anulável), sem
--      gatilho. O pipeline atualiza essas linhas o tempo todo.
--    - transaction_allocations: sem a coluna. Em vez de acrescentá-la, qualquer mudança
--      de rateio atualiza `transactions.updated_at` da movimentação (seção 3): a versão
--      da movimentação guarda também o conjunto de rateios dela.
-- ─────────────────────────────────────────────────────────────────────────
UPDATE public.source_documents SET updated_at = COALESCE(created_at, now()) WHERE updated_at IS NULL;
UPDATE public.draft_batches    SET updated_at = COALESCE(created_at, now()) WHERE updated_at IS NULL;
UPDATE public.draft_records    SET updated_at = COALESCE(created_at, now()) WHERE updated_at IS NULL;

ALTER TABLE public.draft_batches ALTER COLUMN updated_at SET NOT NULL;
ALTER TABLE public.draft_records ALTER COLUMN updated_at SET NOT NULL;

DROP TRIGGER IF EXISTS trg_source_documents_updated_at ON public.source_documents;
CREATE TRIGGER trg_source_documents_updated_at BEFORE UPDATE ON public.source_documents
    FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();

DROP TRIGGER IF EXISTS trg_draft_batches_updated_at ON public.draft_batches;
CREATE TRIGGER trg_draft_batches_updated_at BEFORE UPDATE ON public.draft_batches
    FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();

DROP TRIGGER IF EXISTS trg_draft_records_updated_at ON public.draft_records;
CREATE TRIGGER trg_draft_records_updated_at BEFORE UPDATE ON public.draft_records
    FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();

-- ─────────────────────────────────────────────────────────────────────────
-- 2. Exclusões. O delta precisa dizer à estação o que sumiu. Cada tabela do livro-caixa
--    (as 21 coleções do contrato v2) grava aqui, por gatilho, o id apagado, inclusive o
--    apagado em cascata. `seq` é a posição no registro: o cursor do delta carrega o maior
--    `seq` já visto.
-- ─────────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.exclusoes (
    seq         bigserial PRIMARY KEY,
    user_id     uuid NOT NULL,
    colecao     text NOT NULL,
    id          uuid NOT NULL,
    excluida_em timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS ix_exclusoes_dono ON public.exclusoes (user_id, colecao, seq);

CREATE OR REPLACE FUNCTION public.fn_registra_exclusao() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO public.exclusoes (user_id, colecao, id) VALUES (OLD.user_id, TG_ARGV[0], OLD.id);
    RETURN NULL;
END;
$$;

DO $$
DECLARE
    par text[];
    pares text[][] := ARRAY[
        ['institutions', 'instituicoes'],
        ['financial_products', 'contas'],
        ['cards', 'cartoes'],
        ['statement_cycles', 'faturas'],
        ['statement_items', 'itens-de-fatura'],
        ['transactions', 'movimentacoes'],
        ['transfers', 'transferencias'],
        ['transaction_allocations', 'rateios'],
        ['categories', 'categorias'],
        ['tags', 'tags'],
        ['financial_centers', 'centros'],
        ['suppliers', 'fornecedores'],
        ['supplier_aliases', 'apelidos'],
        ['supplier_contracts', 'contratos'],
        ['liabilities', 'dividas'],
        ['liability_installments', 'parcelas'],
        ['recurring_templates', 'recorrentes'],
        ['recurring_instances', 'ocorrencias'],
        ['source_documents', 'documentos'],
        ['draft_batches', 'lotes-de-rascunhos'],
        ['draft_records', 'rascunhos']
    ];
BEGIN
    FOREACH par SLICE 1 IN ARRAY pares LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS trg_exclusao ON public.%I', par[1]);
        EXECUTE format(
            'CREATE TRIGGER trg_exclusao AFTER DELETE ON public.%I '
            'FOR EACH ROW EXECUTE FUNCTION public.fn_registra_exclusao(%L)',
            par[1], par[2]);
    END LOOP;
END;
$$;

-- ─────────────────────────────────────────────────────────────────────────
-- 3. Rateio muda a versão da movimentação. Quem rateia (o desktop, o web, o pipeline)
--    mexe em `transaction_allocations`; a versão que a estação guarda é a da movimentação.
--    O gatilho não faz nada quando a movimentação inteira está sendo apagada.
-- ─────────────────────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.fn_rateio_muda_versao() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    UPDATE public.transactions
       SET updated_at = now()
     WHERE id = COALESCE(NEW.transaction_id, OLD.transaction_id);
    RETURN NULL;
END;
$$;

DROP TRIGGER IF EXISTS trg_rateio_muda_versao ON public.transaction_allocations;
CREATE TRIGGER trg_rateio_muda_versao AFTER INSERT OR UPDATE OR DELETE ON public.transaction_allocations
    FOR EACH ROW EXECUTE FUNCTION public.fn_rateio_muda_versao();

COMMIT;
