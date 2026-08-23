# Checklist Geral de Implementação — Seu Bolso Feliz

> Documento de acompanhamento de todas as etapas de implementação do projeto.
> Atualizado conforme o progresso real da equipe.

---

## Etapa 0 — Sprint 0: Monorepo, Tooling e CI/CD

### 0.1 Estrutura de Pastas

- [x] `apps/web/src/{app,components/ui,hooks,lib,styles}`
- [x] `apps/web/public`
- [x] `apps/mobile/src/{screens,components,hooks,navigation,lib}`
- [x] `apps/mobile/assets`
- [x] `packages/shared-types/src/domain`
- [x] `packages/validation/src`
- [x] `packages/domain/src/{financial-cycle,amortization,deduplication,priority}`
- [x] `packages/ui-tokens/src`
- [x] `packages/config/{eslint,tsconfig,vitest}`
- [x] `scripts/`
- [x] `__tests__/{domain,integration,e2e}`

### 0.2 Bun Workspaces

- [x] Root `package.json` com `"workspaces": ["apps/*", "packages/*"]`
- [x] Scripts padronizados: dev, build, lint, typecheck, test, format, generate-types, db:migrate, prepare

### 0.3 Migrar Código Web Existente

- [x] Mover `src/components/ui/` → `apps/web/src/components/ui/`
- [x] Mover `src/lib/` → `apps/web/src/lib/`
- [x] Mover `styles/globals.css` → `apps/web/src/styles/globals.css`
- [x] Criar `apps/web/package.json` (Next.js)
- [x] Criar `apps/web/next.config.ts`
- [x] Criar `apps/web/src/app/layout.tsx` (App Router)
- [x] Criar `apps/web/src/app/page.tsx` (página inicial)
- [x] Criar `apps/web/src/app/globals.css`
- [x] Criar `apps/web/postcss.config.mjs` (TailwindCSS v4 via @tailwindcss/postcss)
- [x] Criar `apps/web/tsconfig.json` (extends base)
- [x] Preservar componentes shadcn/ui
- [x] Atualizar `components.json` para paths do monorepo

### 0.4 Pacotes Internos

- [x] `packages/shared-types/package.json` (@sbf/shared-types)
- [x] `packages/shared-types/src/index.ts`
- [x] `packages/shared-types/tsconfig.json`
- [x] `packages/validation/package.json` (@sbf/validation)
- [x] `packages/validation/src/index.ts`
- [x] `packages/validation/tsconfig.json`
- [x] `packages/domain/package.json` (@sbf/domain)
- [x] `packages/domain/src/index.ts`
- [x] `packages/domain/tsconfig.json`
- [x] `packages/ui-tokens/package.json` (@sbf/ui-tokens)
- [x] `packages/ui-tokens/src/index.ts`
- [x] `packages/ui-tokens/tsconfig.json`
- [x] `packages/config/package.json` (@sbf/config)

### 0.5 TypeScript Base

- [x] `tsconfig.base.json` na raiz (composite, declaration, strict, noEmit)
- [x] `tsconfig.json` raiz (references para workspaces)
- [x] `apps/web/tsconfig.json` (extends base, Next.js compat — composite:false, verbatimModuleSyntax:false)
- [x] `apps/mobile/tsconfig.json` (extends base)
- [x] Cada pacote com `tsconfig.json` extends base (emitDeclarationOnly:true)

### 0.6 ESLint 9 + Prettier

- [x] Instalar: eslint@10.1.0, @eslint/js@10.0.1, typescript-eslint@8.57.1, prettier@3.8.1, eslint-config-prettier@10.1.8
- [x] `eslint.config.ts` (flat config ESLint 9)
- [x] `.prettierrc` (semi, doubleQuote, trailingComma:all, tabWidth:2, printWidth:100)
- [x] `.prettierignore`

### 0.7 Vitest

- [x] Instalar: vitest@4.1.0, @vitest/coverage-v8@4.1.0
- [x] `vitest.config.ts` (projetos unit + integration, coverage v8)
- [x] Teste placeholder para validar setup

### 0.8 Husky + lint-staged + commitlint

- [x] Instalar: husky@9.1.7, lint-staged@16.4.0, @commitlint/cli@20.5.0, @commitlint/config-conventional@20.5.0
- [x] `commitlint.config.ts` (Conventional Commits, scopes: web/mobile/domain/types/validation/supabase/ci/docs/config/tokens)
- [x] `.husky/commit-msg`
- [x] `.husky/pre-commit`
- [x] `.lintstagedrc.json`

### 0.9 Supabase Init

- [x] `supabase/config.toml` (via `npx supabase init` v2.83.0)
- [x] `supabase/seed.sql`
- [x] Pasta `supabase/migrations/` criada

### 0.10 Variáveis de Ambiente

- [x] `.env.example` com variáveis documentadas
- [x] `.gitignore` atualizado (supabase/.temp/, .env\*, .next/)
- [x] `scripts/generate-types.sh` criado

### 0.11 GitLab CI/CD

- [x] `.gitlab-ci.yml` com 6 stages: validate → install → check → test → build → deploy
- [x] Jobs condicionais por pasta alterada
- [x] Deploy staging automático (develop)
- [x] Deploy produção manual (main)

### 0.12 Validação Final

- [x] `bun install` sem erros (1083 installs, 1153 packages)
- [x] `bun run lint` passa (ESLint 10.1.0, 0 errors)
- [x] `bun run typecheck` passa (tsc -b, 0 errors)
- [x] `bun run test` passa (Vitest 4.1.0, 7 test files, 142 tests passed)
- [x] `bun run build` passa (Next.js 15.5.14, 5 routes, 102kB first load)
- [x] `bun run format:check` passa (Prettier 3.8.1, 0 issues)

---

## Etapa 1 — Fundação: Modelo de Dados + Auth + CRUD Inicial

### 1.1 Supabase Auth

- [x] Configurar Auth no Supabase (email + magic link)
- [x] Middleware de autenticação no Next.js

### 1.2 Migrations Fundacionais

- [x] Tabelas core: institutions, financial_products, cards, categories, tags, financial_periods, transactions, transfers, statement_cycles, statement_items, recurring_templates, recurring_instances, liabilities, liability_installments, documents, user_secrets, import_jobs, audit_logs, transaction_tags, recurring_template_tags, liability_tags, user_financial_preferences
- [x] RLS habilitado em todas as tabelas (27 tabelas + 3 junção)
- [x] Triggers de updated_at (18 tabelas)

### 1.3 Tabelas de Fornecedor (ADR-001, ADR-002, ADR-003)

- [x] Migration 001: suppliers, supplier_aliases, supplier_contracts, consumption_metrics, supplier_tags
- [x] Migration 002: ALTER TABLE com supplier_id + origin_type
- [x] Migration 003: Índices (trigram, FK, busca fuzzy)
- [x] Migration 004: Triggers (unicidade temporal, auto-alias)
- [x] Migration 005: RLS policies

### 1.4 CRUD Básico (Server Actions)

- [x] `apps/web/src/app/actions/institutions.ts` — CRUD instituições
- [x] `apps/web/src/app/actions/financial-products.ts` — CRUD produtos financeiros
- [x] `apps/web/src/app/actions/suppliers.ts` — CRUD fornecedores + aliases + contratos + busca RPC
- [x] `apps/web/src/app/actions/transactions.ts` — CRUD transações + tags + view deduplicada
- [x] `apps/web/src/app/actions/recurring.ts` — CRUD templates/instâncias recorrentes + geração automática
- [x] `apps/web/src/app/actions/financial-periods.ts` — CRUD períodos financeiros + RPCs + preferências
- [x] `apps/web/src/app/actions/statement-cycles.ts` — CRUD ciclos de fatura + itens
- [x] `apps/web/src/app/actions/liabilities.ts` — CRUD passivos + parcelas + tags

### 1.5 Types Generation

- [x] `scripts/generate-types.sh`
- [x] `packages/shared-types/src/database.types.ts` gerado (1678 linhas)
- [x] `packages/shared-types/src/domain/index.ts` — 20+ type aliases + enums + helpers
- [x] `packages/validation/src/enums.ts` — Zod enums (20+)
- [x] `packages/validation/src/schemas.ts` — Schemas CRUD completos

### 1.6 Testes da Etapa 1

- [x] T18-T24: Supplier CRUD (24 testes em `supplier-validation.test.ts`)
- [x] T25-T26 + T-METRIC: Consumption metrics (5 testes T-METRIC-01 a T-METRIC-05)
- [x] T27 + T-DEDUP: Deduplication (14 testes T-DEDUP-01 a T-DEDUP-04 + T27)
- [x] T-ALIAS: Alias governance (8 testes T-ALIAS-01 a T-ALIAS-08)
- [x] Schemas de validação Zod (34 testes em `validation-schemas.test.ts`)

### 1.7 Domain Logic

- [x] `packages/domain/src/financial-cycle/` — getCurrentPeriod, generatePeriods, findPeriodForDate, daysRemainingInPeriod (17 testes)
- [x] `packages/domain/src/deduplication/` — deduplicateExpenses, sumDeduplicatedExpenses, isExpenseType, isStatementPayment, getStatementComposition (14 testes)
- [x] `packages/domain/src/priority/` — deriveEffectivePriority, calculateSortScore, prioritizeItems, filterByPriority, groupByPriority (19 testes)
- [x] `packages/domain/src/amortization/` — SAC, Price, Misto, simulateEarlyPayoff, getOutstandingBalanceAfter, totalInterestPaid (33 testes)

### 1.8 Auth Pages

- [x] `apps/web/src/app/login/page.tsx` — Login com magic link
- [x] `apps/web/src/app/auth/callback/route.ts` — OAuth callback
- [x] `apps/web/src/app/auth/signout/route.ts` — Sign out
- [x] `apps/web/src/app/dashboard/page.tsx` — Dashboard autenticado

---

## Etapa 2 — Transações, Recorrências e Ciclos Financeiros

- [x] Tabelas: transactions, recurring_templates, recurring_instances (migration 001)
- [x] Ciclos financeiros personalizados (financial_periods, user_financial_preferences) — domain logic + Server Actions + RPCs
- [x] Statement cycles (faturas) — Server Actions CRUD
- [x] View v_expenses_deduplicated (ADR-001) — migration 006 + domain logic + testes
- [x] Testes de regras de negócio (142 testes cobrindo 17 regras críticas do copilot-instructions)

---

## Etapa 3 — Dívidas, Amortização e Empréstimos

- [x] Tabelas: liabilities, liability_installments (migration 001)
- [x] Lógica de amortização (SAC, Price, Misto) em @sbf/domain (33 testes)
- [x] Simulação de quitação antecipada (simulateEarlyPayoff)
- [x] Testes de cálculos financeiros (amortization.test.ts)
- [x] Server Actions: CRUD passivos + parcelas + tags (liabilities.ts)

---

## Etapa 4 — Interface Web (MVP)

### 4.1 Infraestrutura Frontend

- [x] Instalar 31 componentes shadcn/ui (button, card, dialog, input, select, table, tabs, badge, skeleton, toast, sidebar, etc.)
- [x] `apps/web/src/lib/supabase/client.ts` — createBrowserClient
- [x] `apps/web/src/lib/supabase/server.ts` — createServerClient (cookies)
- [x] `apps/web/src/lib/supabase/middleware.ts` — updateSession
- [x] `apps/web/src/middleware.ts` — Middleware de autenticação
- [x] `apps/web/src/lib/format.ts` — formatCurrency, formatDate, formatRelativeDate, isOverdue, isDueToday

### 4.2 App Shell

- [x] `apps/web/src/app/dashboard/layout.tsx` — Layout com SidebarProvider
- [x] `apps/web/src/components/app-sidebar.tsx` — Navegação: Dashboard, Instituições, Produtos, Transações, Recorrências, Faturas, Dívidas, Relatórios, Documentos, Importação, Configurações
- [x] `apps/web/src/components/dashboard-header.tsx` — Header com breadcrumbs, user menu, signout
- [x] `apps/web/src/components/providers.tsx` — ThemeProvider + Toaster

### 4.3 Dashboard (Primeira Tela — Ação e Decisão)

- [x] `apps/web/src/app/dashboard/page.tsx` — Tela orientada a ação: Receita/Despesas/Saldo/Dívida do mês, Fila de Prioridade (próximos 30 dias), Recorrências Próximas, Faturas em Aberto, Últimas Transações
- [x] Integração com `@sbf/domain/priority` (prioritizeItems)
- [x] Indicadores visuais: ATRASADO, HOJE, badges de prioridade (essencial/alta/média/baixa/opcional)

### 4.4 CRUD Instituições + Produtos

- [x] `apps/web/src/app/dashboard/institutions/page.tsx` — Listagem com tipo (banco/fintech/corretora/cooperativa/outro)
- [x] `apps/web/src/app/dashboard/institutions/new/page.tsx` — Formulário de criação
- [x] `apps/web/src/app/dashboard/institutions/[id]/page.tsx` — Edição + exclusão com confirmação
- [x] `apps/web/src/app/dashboard/products/page.tsx` — Listagem com badge de tipo
- [x] `apps/web/src/app/dashboard/products/new/page.tsx` — Formulário com seletor de instituição + 10 tipos de produto
- [x] `apps/web/src/app/dashboard/products/[id]/page.tsx` — Edição + exclusão

### 4.5 CRUD Transações

- [x] `apps/web/src/app/dashboard/transactions/page.tsx` — Listagem paginada com filtros
- [x] `apps/web/src/app/dashboard/transactions/filters.tsx` — Filtros: tipo, confirmado, data de/até
- [x] `apps/web/src/app/dashboard/transactions/new/page.tsx` — Formulário: tipo (8 opções), valor, datas, produto, prioridade, notas
- [x] `apps/web/src/app/dashboard/transactions/[id]/page.tsx` — Edição + exclusão

### 4.6 CRUD Recorrências

- [x] `apps/web/src/app/dashboard/recurring/page.tsx` — Listagem com frequência e próximo vencimento
- [x] `apps/web/src/app/dashboard/recurring/new/page.tsx` — Formulário de template recorrente
- [x] `apps/web/src/app/dashboard/recurring/[id]/page.tsx` — Edição + exclusão

### 4.7 Faturas (Statement Cycles)

- [x] `apps/web/src/app/dashboard/statements/page.tsx` — Listagem de faturas com card e status
- [x] `apps/web/src/app/dashboard/statements/[id]/page.tsx` — Detalhe da fatura com itens

### 4.8 Dívidas (Liabilities)

- [x] `apps/web/src/app/dashboard/liabilities/page.tsx` — Listagem de passivos ativos
- [x] `apps/web/src/app/dashboard/liabilities/new/page.tsx` — Formulário de criação
- [x] `apps/web/src/app/dashboard/liabilities/[id]/page.tsx` — Detalhe com parcelas

### 4.9 Configurações

- [x] `apps/web/src/app/dashboard/settings/page.tsx` — Ciclo financeiro, categorias (CRUD inline), tags (CRUD inline)

### 4.10 Validação

- [x] `bun run typecheck` — 0 erros
- [x] `bun run build` — 26 rotas compiladas (23 estáticas + 3 dinâmicas)
- [x] `bun run test` — 142 testes passando (0 regressões)

---

## Etapa 5 — Documentos, Importação e Polimento

### 5.1 Documentos

- [x] `apps/web/src/app/dashboard/documents/page.tsx` — Upload de documentos (Supabase Storage), listagem, download, exclusão

### 5.2 Importação

- [x] `apps/web/src/app/dashboard/import/page.tsx` — Importação de CSV com parsing, preview, detecção automática de colunas, normalização de datas, import_jobs tracking

### 5.3 Relatórios

- [x] `apps/web/src/app/dashboard/reports/page.tsx` — Relatórios por mês civil, período financeiro personalizado ou intervalo livre: receitas/despesas, por categoria, por tipo, maiores despesas
- [x] `apps/web/src/app/dashboard/reports/filters.tsx` — Componente de filtros com tabs de modo e date range

### 5.4 Validação

- [x] `bun run typecheck` — 0 erros
- [x] `bun run build` — 26 rotas compiladas
- [x] `bun run test` — 142 testes passando (0 regressões)

---

> **Legenda:** `- [ ]` = pendente | `- [x]` = concluído
