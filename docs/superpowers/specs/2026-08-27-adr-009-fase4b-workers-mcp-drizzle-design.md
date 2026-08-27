# ADR-009 Fase 4b — Workers e MCP server → Drizzle/Neon — Spec de Design

**Status:** Aprovada pelo CEO em 2026-08-27 (design em chat, incluindo a estratégia de
cutover e a separação código-vs-dados), com groundwork factual e enumeração
verificados de forma independente antes da escrita desta spec.

**ADR de origem:** `docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md` (ADR-009).
Esta é a segunda das três partes em que a antiga "Fase 4" foi decomposta (adendo de
2026-08-26): Fase 4a (Server Actions + componentes client-side, **concluída**
2026-08-27) → **Fase 4b (este documento)** → Cutover (apêndice desta spec).

**Pré-requisito:** ADR-009 Fase 1 (`@sbf/db`, schema Neon) e Fase 4a (padrão de
migração já validado em produção de código, 21 tasks, 16 IDORs fechados).

**Documento irmão:** Fase 4a-bis (páginas do dashboard + rotas de API de
reconciliação que a Fase 4a não cobriu) — spec separada, superfície disjunta
(`apps/web/src/app/dashboard/**`, `apps/web/src/app/api/reconciliation/**`), pode
rodar em paralelo a esta.

## Contexto

Levantamento factual (2026-08-27, `Explore` + verificação manual, números batem com
a medição original da ADR): 6 componentes usam `@supabase/supabase-js` com
`SUPABASE_SECRET_KEY` (chave de service role — bypassa RLS por completo, e não há
mais nenhuma `CREATE POLICY` no schema Neon de qualquer forma):

| Componente              | `.from()` reais | Tabelas              | `.rpc()` | Notas                                                                                                                                                                                    |
| ----------------------- | --------------- | -------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `workers/ingestion`     | 39              | 12                   | 4        | `fn_get_secrets`, `fn_mark_secret_used`, `fn_set_secret`, `fn_upsert_financial_obligation`                                                                                               |
| `workers/gmail-scanner` | 10              | 4                    | 0        |                                                                                                                                                                                          |
| `workers/local-scanner` | 5               | 3                    | 0        |                                                                                                                                                                                          |
| `workers/pluggy-sync`   | 13              | 6                    | 0        | persistência mora aqui, não em `packages/financial-connectors` (que é só cliente HTTP da API Pluggy, zero Supabase)                                                                      |
| `apps/mcp-server`       | 27              | 7                    | 2        | `fn_materialize_draft_record`, `generate_financial_periods`; **usa Storage também** (`scan-local-folder.ts`, upload em `ingestion-originals` — a ADR dizia "sem Storage", estava errada) |
| `workers/cli`           | 1               | 1 (`ingestion_jobs`) | 0        | **achado novo, não rastreado antes** — compõe `ingestion`/`local-scanner`/`gmail-scanner` num processo único, tem seu próprio client Supabase                                            |

**Confirmado: o app é single-user de verdade** (ADR-007) — todo componente lê
`LOCAL_USER_ID` de env var fixa, nenhum faz `SELECT DISTINCT user_id` nem itera por
usuário. A concorrência real é **multi-processo, mesmo usuário** (vários workers
escrevendo nas mesmas tabelas ao mesmo tempo), não multi-tenant — os padrões de
proteção existentes (`.eq("status", from)` como optimistic locking em
`processor.ts`, `UNIQUE(user_id, content_hash)` em `document_fingerprints`) já
partem dessa premissa e continuam válidos sem mudança.

**Tabelas de alto overlap entre componentes** (risco de split-brain se um
componente migrar e outro não, por isso o cutover único coordenado — ver apêndice):
`ingestion_jobs`/`source_documents` (5 componentes), `draft_records`/`draft_batches`
(3), `document_fingerprints` (3), `ingestion_runs` (4).

## Escopo

**Dentro:**

- Os 6 componentes (`ingestion`, `gmail-scanner`, `local-scanner`, `pluggy-sync`,
  `mcp-server`, `cli`) trocam a criação do client Supabase por `getDb()`
  (`@sbf/db`), toda leitura/escrita vira query Drizzle.
- Toda query mantém `WHERE user_id = $1` explícito, com `$1` vindo de
  `getUserId()`/`LOCAL_USER_ID` (não de input externo) — mesma disciplina da Fase
  4a, mesmo sem multi-tenancy real: defesa em profundidade e consistência de
  padrão valem mais que a economia de uma cláusula.
- **RPCs portáveis** (`fn_get_secrets`, `fn_mark_secret_used`,
  `fn_upsert_financial_obligation`, `fn_materialize_draft_record` — já existem no
  Neon, checagem interna de `auth.uid()` já removida no port da Fase 1): mantidas
  como chamada SQL crua via Drizzle (`db.execute(sql\`...\`)`), agora passando
`p_user_id`explícito de`LOCAL_USER_ID`em vez de depender de`auth.uid()` —
mesmo padrão já usado e revisado na Task 13 da Fase 4a
(`materialization.ts`).
- **RPCs bloqueadas** (`fn_set_secret` — depende de `private.get_crypto_key()`,
  schema `private` não portado; `generate_financial_periods` — depende de
  `auth.uid()` sem parâmetro, **confirmado quebrado em produção hoje** via
  reprodução direta: `SELECT generate_financial_periods(1,1)` sem contexto de auth
  lança `ERROR: Not authenticated`): reimplementadas em TypeScript dentro do
  worker/tool que as chama, não recriadas no Postgres. `fn_set_secret` precisa de
  uma decisão de onde a chave de criptografia mora agora (ver Apêndice A);
  `generate_financial_periods` pode reaproveitar lógica de
  `packages/domain` (módulo `financial-cycle` já existe) em vez de reescrever do
  zero — a task de implementação confirma isso ao chegar lá.
- Testes de integração novos por componente, contra o mesmo Postgres local de
  teste já usado na Fase 4a (`sbf_test_pg`) — sem necessidade de simular
  multi-tenant (não existe), mas simulando concorrência multi-processo onde já
  há proteção hoje (ex.: teste de optimistic locking em `processor.ts` continua
  válido).

**Fora:**

- **As 4 Edge Functions** (Deno, runtime Supabase): fora de escopo desta fase.
  `trigger-ingestion` (2 call sites vivos, `ai-chat-drawer.tsx` e
  `upload-documents.tsx`) já está marcada como quebrada contra o schema real em
  `supabase/config.toml` — tratamento fica com a Fase 4a-bis (mesma superfície de
  componentes web) ou item próprio, não aqui. As outras 3
  (`merge-suppliers`, `refresh-mv-supplier-spending`,
  `retroactive-supplier-association`, zero call sites encontrados) continuam
  adiadas para a Fase 6, conforme já registrado na spec da Fase 2.
- **`apps/mcp-server`'s uso de Storage** (`scan-local-folder.ts`): migra na Fase 3
  (Storage), não aqui — só o acesso a tabelas migra nesta fase.
- **Cutover de dados real e flip de produção:** ver Apêndice B — desenvolvido e
  testado nesta fase, mas a **execução** é bloqueada até autorização explícita
  separada.
- `@sbf/shared-types`/`database.types.ts`: seguem existindo até não terem mais
  nenhum consumidor (nenhum destes 6 componentes deveria precisar deles depois
  desta fase, mas a remoção do pacote em si é item de limpeza pós-cutover, não
  desta spec).

## Apêndice A — RPCs bloqueadas: decisões a tomar na implementação

| RPC                          | Chamada por                                          | Decisão a resolver na task                                                                                                                                                                                                                                                                                             |
| ---------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fn_set_secret`              | `workers/ingestion` (`parsers/secret-lookup.ts`)     | Onde mora a chave de criptografia agora? Opções: (a) portar `private.crypto_keys` + `pgcrypto` pro Neon como estava; (b) usar uma envvar/KMS externo e reimplementar `encrypt_secret`/`decrypt_secret` em TS; (c) aceitar perda de feature com aval do CEO (mesmo critério do apêndice de RPCs bloqueadas da Fase 4a). |
| `generate_financial_periods` | `apps/mcp-server` (`recompute-financial-periods.ts`) | Reimplementar em TS (candidato: reaproveitar `packages/domain`'s `financial-cycle`) ou aceitar que a tool fica indisponível até decisão futura — **já está quebrada hoje, então "não fazer nada" não piora o status quo**, mas resolver aqui é a oportunidade natural.                                                 |

**Nota de paridade (revisão da Veronica-Bolso):** `workers/pluggy-sync` tem uma
lacuna documentada e deliberada — checkpoint sem claim atômico (ver
`docs/integrations/pluggy-deploy.md`). A migração de código desta fase preserva
esse comportamento tal como está; locking atômico é decisão separada, já
registrada, não entra escondida junto da troca de driver.

## Apêndice B — Cutover de dados real (execução bloqueada, autorização própria)

O Neon hoje só tem **schema**, não dados reais — todo o histórico de documentos,
transações, drafts etc. vive no Postgres do Supabase. "Cutover" nesta ADR
significa: migrar esses dados reais pro Neon e virar o tráfego de produção (web +
6 componentes desta fase) de uma vez, coordenado — a opção que o CEO escolheu
sobre dual-write, evitando qualquer janela de split-brain nas tabelas de alto
overlap listadas acima.

Esboço do procedimento (a ser detalhado num plano operacional próprio, não nesta
spec de design):

1. Congelar escrita: parar os 4 workers + MCP + qualquer processo `cli`.
2. Exportar dados reais do Postgres do Supabase (schema `public`) e importar no
   Neon — schema já deve bater (Neon foi introspectado a partir do mesmo dump),
   mas exige verificação de paridade linha a linha antes de liberar tráfego.
3. Apontar `DATABASE_URL` de todos os componentes (web, 6 daqui, e o Vercel) pro
   Neon.
4. Manter o Postgres do Supabase **somente-leitura** por um período (a definir)
   como plano de rollback/auditoria antes de decomissionar de vez.
5. Reativar os workers e o MCP contra o Neon.

**Critério de aceite do flip (Veronica-Bolso): zero `supabase.*` em código
ativo, verificado por grep.** Isso cobre mais RPCs bloqueadas do que as 2 do
Apêndice A — as 4 do lado web que continuam como `supabase.rpc()` até então
(`register_pattern_feedback`, `search_suppliers`, `get_financial_period_for_date`,
`fn_reconciliation_progress`) também **morrem no flip** e precisam ter sua
decisão (recriar no Neon com `p_user_id` / reimplementar em TS / aceitar perda
com aval do CEO) resolvida no plano operacional do cutover, não deixadas pra
depois. O plano operacional (fora desta spec) precisa de uma tabela própria
listando as 6 RPCs bloqueadas do programa inteiro (2 daqui + 4 do lado web) e
a decisão tomada para cada uma.

**Esta execução não roda automaticamente ao final da implementação de código
desta fase.** É um passo com dados financeiros reais, efetivamente irreversível
na prática (mesmo com o Postgres antigo preservado por um tempo) — precisa de
autorização explícita e presente do CEO no momento de executar, não just-in-time
durante uma sessão autônoma.
