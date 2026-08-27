# ADR-009 Fase 4a-bis — Páginas do dashboard e rotas de reconciliation → Server Actions/Drizzle — Spec de Design

**Status:** Aprovada pelo CEO em 2026-08-27 (design em chat), com enumeração
verificada de forma independente (grep próprio batendo com a varredura da
Veronica-Bolso, mais 1 achado adicional — `api/chat/route.ts` via wrapper
`untyped.ts`, invisível a grep literal de `.from(`).

**ADR de origem:** `docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md`
(ADR-009). Nasce de um achado do próprio gate de fechamento da Fase 4a
(2026-08-27): o levantamento de impacto original só contou as 17 Server Actions
e os 3 componentes client-side já migrados — não capturou o resto do
`apps/web/src` que ainda acessa Supabase direto.

**Pré-requisito:** Fase 4a concluída — a maioria das tabelas aqui já tem Server
Action pronta (`getInstitutions`, `getTransactions`, `getFinancialProducts`
etc.); a tarefa típica desta fase é **trocar a chamada, não escrever query
nova**.

**Documento irmão:** Fase 4b (workers + MCP) — superfície disjunta, pode rodar
em paralelo.

## Contexto

Levantamento (grep + verificação manual, 2026-08-27):

```
grep -rn '\.from("\|\.rpc(\|functions\.invoke' apps/web/src --include='*.tsx' --include='*.ts' \
  | grep -v '\.test\.' | grep -v "/app/actions/" | grep -v "lib/ai" \
  | grep -v "ingestion-originals" | grep -v '\.storage\.from("user-files")' \
  | grep -v 'functions\.invoke("trigger-ingestion"'
```

74 ocorrências em 28 arquivos de página/componente, mais 1 arquivo
(`api/chat/route.ts`) que só aparece se procurar por `untypedFrom`/`untypedRpc`
— o wrapper genérico em `lib/supabase/untyped.ts` esconde 3 chamadas reais
(`ai_chat_messages`) de qualquer grep literal em `.from("`.

## Escopo

**Dentro — 4 categorias de trabalho:**

### 1. Páginas do dashboard (28 arquivos, reuso de Server Actions existentes)

A grande maioria já tem Server Action pronta da Fase 4a — a task é substituir
`supabase.from(...)` pela função já exportada, ajustando o shape de retorno se
necessário (mesmo cuidado de preservação de shape já seguido nas 3 migrações
de componente da Fase 4a).

| Página(s)                                                    | Tabela(s)                                                                                | Server Action existente                                                                                                                           |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `dashboard/page.tsx` (8)                                     | institutions, liabilities, recurring_instances, statement_cycles, transactions           | `institutions.ts`, `liabilities.ts`, `recurring.ts`, `statement-cycles.ts`, `transactions.ts`                                                     |
| `reports/page.tsx` (6)                                       | categories, suppliers, transactions, user_financial_preferences, v_expenses_deduplicated | `suppliers.ts`, `transactions.ts` (`getDeduplicatedExpenses` já existe); **categories precisa de action nova**                                    |
| `transactions/[id]`, `transactions/new`, `transactions/page` | transactions                                                                             | `transactions.ts`                                                                                                                                 |
| `recurring/[id]`, `recurring/new`, `recurring/page`          | recurring_templates/instances                                                            | `recurring.ts`                                                                                                                                    |
| `products/[id]`, `products/new`, `products/page`             | financial_products, institutions                                                         | `financial-products.ts`, `institutions.ts`                                                                                                        |
| `institutions/[id]`, `institutions/new`, `institutions/page` | institutions                                                                             | `institutions.ts`                                                                                                                                 |
| `liabilities/[id]`, `liabilities/new`, `liabilities/page`    | liabilities                                                                              | `liabilities.ts`                                                                                                                                  |
| `statements/[id]`, `statements/page`                         | statement_cycles                                                                         | `statement-cycles.ts`                                                                                                                             |
| `connections/[id]`, `connections/page`                       | provider_connections, financial_products                                                 | `pluggy-connections.ts`, `financial-products.ts` (achado: `connections/[id]` já era conhecido como pendente desde a revisão da Task 4 da Fase 4a) |
| `suppliers/page`, `suppliers/new`                            | suppliers                                                                                | `suppliers.ts`                                                                                                                                    |
| `settings/passwords/page`                                    | suppliers                                                                                | `suppliers.ts`                                                                                                                                    |
| `settings/page` (9)                                          | categories, tags, user_financial_preferences                                             | `user_financial_preferences` precisa de olhar (ver nota); **categories e tags precisam de action nova**                                           |

**Tabelas sem Server Action ainda: `categories`, `tags`.** Nenhuma das 21 tasks
da Fase 4a precisou escrever nelas diretamente (só validar posse como FK — ver
`assertOwnsTags`/`requireOwnedCategory` em vários arquivos). Esta fase cria
`apps/web/src/app/actions/categories.ts` e `apps/web/src/app/actions/tags.ts`
(CRUD simples, mesmo padrão `WHERE user_id` das demais).

**`user_financial_preferences`:** já é lida/escrita dentro de
`financial-periods.ts` (`upsertFinancialPreferences`, ver nota da Task 3 da
Fase 4a) — confirmar se essa função cobre o que `settings/page.tsx` precisa ou
se falta uma leitura dedicada.

### 2. Rotas de API de reconciliation (2 arquivos)

`api/reconciliation/[draftId]/route.ts` (9 chamadas: `draft_records`,
`recurring_instances`, `recurring_templates`, `source_documents`,
`transactions`) e `.../progress/route.ts` (RPC `fn_reconciliation_progress`).
São Route Handlers server-side — não precisam virar Server Action, podem usar
`getDb()` (`@sbf/db`) diretamente com o mesmo `requireUserId()` de sempre.

**Achado sobre `fn_reconciliation_progress` (correção à enumeração da
Veronica-Bolso — esta função é diferente das outras RPCs bloqueadas):**
já está portada no Neon (`packages/db/schema.sql:585`), e **não depende de
`auth.uid()`** — mas também **nunca verificou posse**: filtra só por
`batch_id`, sem checar se o `draft_batch` pertence ao usuário chamador, mesmo
antes desta migração (SECURITY DEFINER bypassa RLS, e o corpo da função não
faz `WHERE user_id`). Não é regressão desta fase, é um gap pré-existente do
lado Supabase — mas como esta fase já está tocando o arquivo, a lente de
posse-de-FK já estabelecida na Fase 4a se aplica: **a rota precisa validar que
o `batch_id` pertence ao usuário antes de chamar a RPC**, fechando o gap junto
com a migração, não deixando pra depois.

### 3. `api/chat/route.ts` (achado novo — `ai_chat_messages`)

3 operações via `untypedFrom(supabase, "ai_chat_messages")` (select, 2
inserts) — invisíveis a grep literal porque passam pelo wrapper genérico
`lib/supabase/untyped.ts`. Migram pra Drizzle direto na própria rota (ou uma
nova `actions/chat-messages.ts`, mesmo par de `chat-sessions.ts` da Task 20).
`untypedRpc` também existe no wrapper mas não tem nenhum consumidor
encontrado — remover junto se confirmado morto.

**`lib/supabase/untyped.ts` é removido inteiro** quando `api/chat/route.ts` for
o único consumidor migrado (verificar com grep antes de apagar).

### 4. Substituição do `trigger-ingestion` (movida da Fase 2, ruling da Veronica-Bolso)

O desenho já existe, pronto, na spec da Fase 2
(`docs/superpowers/specs/2026-08-26-adr-009-fase2-auth-minima-design.md`,
seção "Substituição do `trigger-ingestion`") — não depende de nenhuma mudança
de auth, só de `requireUser()` que já funciona hoje. Puxado pra cá porque
conserta um bug **já quebrado em produção** (upload-via-chat) sem esperar a
Fase 2:

- Novas rotas: `api/ingestion/upload-url/route.ts` (signed upload URL via
  service role) e `api/ingestion/trigger/route.ts` (registro do job com as
  colunas reais de `source_documents`/`ingestion_jobs`/`ingestion_runs`).
- 2 componentes migram a chamada: `ai-chat-drawer.tsx`, `upload-documents.tsx`
  (trocam `supabase.functions.invoke("trigger-ingestion", ...)` pelas 2 rotas
  novas em sequência).
- A Edge Function `trigger-ingestion` é removida só depois das rotas novas
  verificadas em produção — nunca antes, para não haver janela sem nenhuma das
  duas funcionando.
- Código completo (payloads, validação de `user_id` só da sessão, testes) já
  está na spec da Fase 2 — esta fase só executa o que já foi desenhado.

**Fora:**

- Workers, MCP server (Fase 4b).
- `dashboard/suppliers/page.tsx`/`suppliers/new/page.tsx`: **na verdade
  DENTRO** (linha da tabela acima) — mencionado aqui só para deixar explícito
  que não foram esquecidos por serem parecidos com `suppliers/[id]` (já
  migrado na Fase 4a).
- Storage em si (upload direto pro bucket) — Fase 3 decide o modelo definitivo;
  o signed-upload-URL do item 4 acima é ponte, não solução final.
- Auth em si (`requireUser()`/`supabase.auth.getUser()` continuam iguais) —
  Fase 2.
- **5 RPCs já bloqueadas e conhecidas do lado web** (`register_pattern_feedback`,
  `search_suppliers`, `get_financial_period_for_date`, `generate_financial_periods`,
  `fn_set_secret`) — continuam intencionalmente intocadas, já documentadas no
  apêndice da Fase 4a. As 2 últimas são **cross-surface** (achado da revisão
  técnica da Veronica-Bolso): `generate_financial_periods`
  (`actions/financial-periods.ts:120`) e `fn_set_secret`
  (`actions/secrets.ts:97`) também são chamadas do lado worker/MCP, e sua
  reimplementação em TypeScript é responsabilidade do Apêndice A da spec da
  Fase 4b (num pacote compartilhado, não duplicada aqui) — não desta fase.
  Sua resolução final entra no plano operacional de cutover (Apêndice B da
  spec da Fase 4b).

## Critério fixo de revisão (herdado da Fase 4a, não repetir aqui a cada task)

Toda FK recebida do cliente precisa de posse validada antes de escrita — o
mesmo critério que fechou 16 IDORs na Fase 4a. As páginas novas desta fase
majoritariamente **leem e chamam Server Actions já revisadas com esse
critério** — o risco novo real está nas 2 rotas de reconciliation (achado do
`fn_reconciliation_progress` acima) e nas 2 novas Server Actions
(`categories.ts`, `tags.ts`), que precisam da mesma disciplina desde o
primeiro commit, não como correção depois.

## Testes

Mesma técnica da Fase 4a: suíte de integração contra Postgres real
(`sbf_test_pg`), isolamento por `user_id` como teste obrigatório em toda
Server Action nova, `noUncheckedIndexedAccess` já ratificado em todo
`insert(...).returning()`.
