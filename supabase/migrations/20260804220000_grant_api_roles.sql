-- Privilégios de tabela para os papéis da API. Sem isto, o schema sobe mudo.
--
-- DESCOBERTO POR EXECUÇÃO, não por leitura: depois de um `supabase db reset`
-- limpo, NENHUMA das 46 tabelas de `public` era acessível por `service_role`
-- nem por `authenticated`. O worker recebia `permission denied for table
-- ingestion_runs`, a web não lia nada, e 29 dos 60 testes de integração
-- falhavam. A chave legacy `service_role` (JWT) falhava exatamente igual à
-- `sb_secret_...` — ou seja, não era problema de formato de chave.
--
-- CAUSA: as migrations criam objetos como papel `postgres` e dependiam de um
-- `ALTER DEFAULT PRIVILEGES` implícito do Supabase. Mas o default ACL do
-- schema `public` existe só para `supabase_admin`:
--
--   supabase_admin | public | r | {postgres=…,anon=…,authenticated=…,service_role=…}
--      (nenhuma linha para  postgres | public)
--
-- Objeto criado por `postgres` não herda nada dessa entrada. O privilégio
-- nunca foi concedido — apenas presumido.
--
-- POR QUE ISTO NÃO ABRE O BANCO: `GRANT` dá acesso à *tabela*; são as políticas
-- de RLS que filtram *linha* por linha, e elas continuam intactas. Sem o
-- `GRANT`, o RLS nem chega a ser avaliado — o Postgres barra antes. Este é o
-- modelo padrão do Supabase, e é o que o restante do schema já pressupõe:
-- toda tabela de `public` tem `ENABLE ROW LEVEL SECURITY` com política por
-- `user_id`.

-- ── 1. Uso do schema ──────────────────────────────────────────────────────
GRANT USAGE ON SCHEMA public TO anon, authenticated, service_role;

-- ── 2. Objetos que já existem ─────────────────────────────────────────────
GRANT ALL ON ALL TABLES IN SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO anon, authenticated, service_role;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO anon, authenticated, service_role;

-- ── 3. Objetos futuros ────────────────────────────────────────────────────
--
-- Sem este bloco, a próxima migration que criar uma tabela reintroduz o mesmo
-- bug em silêncio — e ele só aparece em runtime, num `permission denied` que
-- parece problema de chave. A entrada é registrada PARA O PAPEL `postgres`,
-- que é quem o CLI usa para aplicar migrations, tanto local quanto no
-- `db push` contra o projeto hospedado.
ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public
  GRANT ALL ON TABLES TO anon, authenticated, service_role;
ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public
  GRANT ALL ON SEQUENCES TO anon, authenticated, service_role;
ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public
  GRANT EXECUTE ON FUNCTIONS TO anon, authenticated, service_role;

-- ── 4. `private` continua fechado ─────────────────────────────────────────
--
-- Declarado explicitamente para que a intenção fique registrada: o schema
-- `private` guarda `crypto_keys`, e o acesso a segredos passa exclusivamente
-- pelas funções `SECURITY DEFINER` (`fn_get_secrets`, `fn_set_secret`), que
-- têm o seu próprio GRANT EXECUTE. Nenhum papel da API toca a tabela.
REVOKE ALL ON SCHEMA private FROM anon, authenticated, service_role;

-- rollback:
--   REVOKE ALL ON ALL TABLES IN SCHEMA public FROM anon, authenticated, service_role;
--   REVOKE ALL ON ALL SEQUENCES IN SCHEMA public FROM anon, authenticated, service_role;
--   REVOKE EXECUTE ON ALL FUNCTIONS IN SCHEMA public FROM anon, authenticated, service_role;
--   ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public
--     REVOKE ALL ON TABLES FROM anon, authenticated, service_role;
--   ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public
--     REVOKE ALL ON SEQUENCES FROM anon, authenticated, service_role;
--   ALTER DEFAULT PRIVILEGES FOR ROLE postgres IN SCHEMA public
--     REVOKE EXECUTE ON FUNCTIONS FROM anon, authenticated, service_role;
--
--   ATENÇÃO: o rollback derruba a aplicação inteira. Só faz sentido se este
--   arquivo for revertido junto com o schema que ele acompanha.
