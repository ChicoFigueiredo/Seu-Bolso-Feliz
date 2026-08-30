-- As telas de cadastro oferecem opções que o CHECK do banco rejeita: escolher
-- "Cooperativa de Crédito" ou "Corretora" fazia o save falhar com erro de
-- constraint. Duas correções em direções opostas, porque as causas são
-- diferentes:
--
--   Instituição: cooperativa de crédito e corretora são tipos reais e
--   legítimos de instituição financeira. Quem estava errado era o banco.
--   Corrigido aqui.
--
--   Produto financeiro: cartão de débito, empréstimo e seguro NÃO são
--   produtos separados neste modelo -- débito é meio de acesso a uma conta
--   corrente, "empréstimo" duplica personal_loan, e seguro é despesa
--   recorrente, não produto. Quem estava errado era a tela; corrigido lá.

BEGIN;

ALTER TABLE public.institutions DROP CONSTRAINT IF EXISTS institutions_type_check;
ALTER TABLE public.institutions ADD CONSTRAINT institutions_type_check
    CHECK (type = ANY (ARRAY['bank', 'credit_union', 'brokerage', 'fintech', 'broker', 'other']));

COMMIT;
