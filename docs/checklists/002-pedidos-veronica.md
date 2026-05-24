# 001 — Checklist Completo dos Pedidos da Verônica

Objetivo: consolidar, em formato auditável, tudo o que foi pedido pela Verônica e o status real de implantação no codebase.

Legendas:

- [x] Concluído
- [ ] Pendente
- [ ] Parcial (descrito no campo Observações)

## Fase A — Fundação de Arquitetura e Domínio (001-prompt.inicial)

### A1. Stack e arquitetura-base

- [x] Web com React + Next.js + Tailwind
  - Critério de aceite: apps/web com build funcional e rotas do App Router
  - Evidência: apps/web/package.json, apps/web/src/app/\*, bun run build OK
- [x] Mobile em estrutura React Native/Expo preparada
  - Critério de aceite: apps/mobile estruturado no monorepo
  - Evidência: apps/mobile/package.json, apps/mobile/src
- [x] Backend central em Supabase (Postgres/Auth/RLS/Storage/Functions)
  - Critério de aceite: supabase/config.toml + migrations + uso de client no web
  - Evidência: supabase/config.toml, supabase/migrations/_, apps/web/src/lib/supabase/_
- [x] Estratégia serverless-first com baixo acoplamento operacional
  - Critério de aceite: operações centrais no Supabase e pipeline CI/CD com CLI
  - Evidência: .gitlab-ci.yml, docs/adrs/ADR-004-arquitetura-operacional-repositorio-cicd.md

### A2. Princípios de modelagem financeira

- [x] Separação despesa vs transferência interna
  - Critério de aceite: regra explícita de deduplicação e tipos de evento
  - Evidência: packages/domain/src/deduplication/index.ts, docs/adrs/ADR-001-deduplicacao-transacao-item-fatura.md
- [x] Separação de composição da dívida (amortização/juros/encargos)
  - Critério de aceite: modelagem + cálculos e testes
  - Evidência: supabase/migrations/20260321170000_create_core_foundation_tables.sql, packages/domain/src/amortization/index.ts, **tests**/domain/amortization.test.ts
- [x] Ciclos financeiros personalizados
  - Critério de aceite: funções de domínio e tabela de períodos
  - Evidência: packages/domain/src/financial-cycle/index.ts, **tests**/domain/financial-cycle.test.ts
- [x] Categoria principal + múltiplas tags
  - Critério de aceite: tabelas categories/tags/junções e uso em transações
  - Evidência: supabase/migrations/20260321170000_create_core_foundation_tables.sql, apps/web/src/app/actions/transactions.ts
- [x] Priorização de pagamento
  - Critério de aceite: enum/campo + motor de priorização
  - Evidência: packages/domain/src/priority/index.ts, **tests**/domain/priority.test.ts

## Fase B — Dimensão Fornecedor (002-fornecedor)

### B1. Entidade e relações

- [x] Cadastro de fornecedores como entidade própria
  - Critério de aceite: tabela suppliers + ações CRUD
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql, apps/web/src/app/actions/suppliers.ts
- [x] Associação de fornecedor com lançamentos
  - Critério de aceite: supplier_id em transactions
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql
- [x] Associação de fornecedor com recorrências
  - Critério de aceite: supplier_id em recurring_templates
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql
- [x] Associação de fornecedor com documentos
  - Critério de aceite: supplier_id em documents
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql
- [x] Associação de fornecedor com itens de fatura
  - Critério de aceite: supplier_id em statement_items
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql

### B2. Aliases, histórico e contratos

- [x] Registro de aliases e nomes antigos
  - Critério de aceite: tabela supplier_aliases com vigência
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql
- [x] Governança de unicidade temporal de alias
  - Critério de aceite: trigger de validação
  - Evidência: supabase/migrations/20260321170400_create_triggers.sql
- [x] Auto-alias em renomeação
  - Critério de aceite: trigger de renomeação
  - Evidência: supabase/migrations/20260321170400_create_triggers.sql
- [x] Contratos e identificadores externos
  - Critério de aceite: tabela supplier_contracts
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql

### B3. Métricas de consumo (consumption_metrics)

- [x] Estrutura de métricas de consumo criada
  - Critério de aceite: tabela consumption_metrics com campos quantitativos
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql
- [x] Norma de uso formalizada
  - Critério de aceite: ADR publicada
  - Evidência: docs/adrs/ADR-002-norma-consumption-metrics.md
- [x] Constraint técnica de consistência implantada
  - Critério de aceite: validação em banco para métrica vs atributo
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql

### B4. Lacunas da dimensão fornecedor

- [x] Tela dedicada de fornecedores no dashboard
  - Fase alvo: Fase D
  - Critério de aceite: rotas dashboard/suppliers com CRUD completo e alias/contratos
  - Evidência: apps/web/src/app/dashboard/suppliers/{page.tsx, new/page.tsx, [id]/page.tsx}
- [x] Filtros compostos avançados com fornecedor em toda a experiência de relatório
  - Fase alvo: Fase E
  - Critério de aceite: filtros por fornecedor + categoria + tags + período + prioridade em relatórios e listagens
  - Evidência: apps/web/src/app/dashboard/reports/{page.tsx, filters.tsx} — filtro por fornecedor + card "Despesas por Fornecedor"

## Fase C — Parecer Formal (Aprovado com ajustes obrigatórios)

### C1. Ajuste obrigatório 1 — Deduplicação transação x item de fatura

- [x] Estratégia formalizada em ADR
  - Critério de aceite: ADR-001 publicada com regras de precedência/soma/exibição
  - Evidência: docs/adrs/ADR-001-deduplicacao-transacao-item-fatura.md
- [x] Estruturas de suporte implantadas
  - Critério de aceite: FK e view de deduplicação
  - Evidência: migrations 20260321170200 e 20260321170600
- [x] Regras cobertas por testes unitários
  - Critério de aceite: suíte dedup passando
  - Evidência: **tests**/domain/deduplication.test.ts

### C2. Ajuste obrigatório 2 — Norma de consumption_metrics

- [x] Norma formalizada e versionada
  - Critério de aceite: ADR-002 publicada
  - Evidência: docs/adrs/ADR-002-norma-consumption-metrics.md
- [x] Regras de dados implantadas no schema
  - Critério de aceite: campos + constraint coerentes
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql

### C3. Ajuste obrigatório 3 — Governança técnica de aliases

- [x] Governança formalizada e versionada
  - Critério de aceite: ADR-003 publicada
  - Evidência: docs/adrs/ADR-003-governanca-aliases-fornecedor.md
- [x] Regras técnicas básicas implantadas
  - Critério de aceite: triggers e colunas de vigência
  - Evidência: supabase/migrations/20260321170400_create_triggers.sql
- [x] Fluxo completo de merge/reversão de fornecedores implementado operacionalmente
  - Fase alvo: Fase F
  - Critério de aceite: operação atômica disponível e testada em função server-side
  - Evidência: supabase/functions/merge-suppliers/index.ts, supabase/functions/retroactive-supplier-association/index.ts, supabase/migrations/20260321170700_create_merge_suppliers_rpc.sql

## Fase D — Governança de Engenharia (GitLab + Supabase)

### D1. Padrão de repositório e branches

- [x] Monorepo com workspaces e estrutura modular
  - Critério de aceite: apps/_ + packages/_ + supabase/_ + docs/_
  - Evidência: package.json, árvore do repositório
- [x] Convenção de commit convencional aplicada
  - Critério de aceite: commitlint configurado
  - Evidência: commitlint.config.ts
- [x] Hook de bloqueio de push com qualidade mínima
  - Critério de aceite: pre-push roda lint/typecheck/test:unit
  - Evidência: .husky/pre-push

### D2. CI/CD e ambientes

- [x] Pipeline GitLab com estágios principais
  - Critério de aceite: validate/install/check/test/build/deploy
  - Evidência: .gitlab-ci.yml
- [x] Deploy de migrations/functions previsto por ambiente
  - Critério de aceite: jobs staging e production existentes
  - Evidência: .gitlab-ci.yml
- [ ] Deploy web real automatizado (sem placeholder)
  - Fase alvo: Fase G
  - Critério de aceite: job deploy-web-\* executa integração real com provedor
  - Observações: hoje usa echo de placeholder
- [ ] Comprovação em ambiente remoto da política completa (proteções, approvals, variáveis)
  - Fase alvo: Fase G
  - Critério de aceite: checklist operacional validado no GitLab do projeto
  - Observações: depende de configuração manual na plataforma

## Fase E — Produto Web MVP (fluxo operacional)

### E1. Funcionalidades principais já disponíveis

- [x] Login/autenticação com Supabase
- [x] Dashboard operacional
- [x] CRUD de instituições
- [x] CRUD de produtos financeiros
- [x] CRUD de transações
- [x] CRUD de recorrências
- [x] CRUD de faturas
- [x] CRUD de dívidas
- [x] Upload/listagem de documentos
- [x] Importação CSV
- [x] Relatórios por período

Critério de aceite E1: rotas compilam no build e páginas acessíveis no app web.
Evidência: bun run build OK com rotas /dashboard/\*.

### E2. Lacunas funcionais ainda abertas no MVP

- [x] Gestão visual completa da dimensão fornecedor no dashboard
  - Critério de aceite: fluxo completo (listar, criar, editar, aliases, contratos, associação assistida)
  - Evidência: apps/web/src/app/dashboard/suppliers/{page.tsx, new/page.tsx, [id]/page.tsx} com alias CRUD inline e contratos inline
- [x] Relatórios avançados por fornecedor com filtros compostos completos
  - Critério de aceite: respostas operacionais para perguntas do prompt 002
  - Evidência: apps/web/src/app/dashboard/reports/page.tsx — 6 queries paralelas + card por fornecedor + filtro dropdown
- [ ] Auditoria histórica visível por fornecedor na interface
  - Critério de aceite: visão de histórico consolidado e variações
  - Observações: dados de auditoria são registrados no audit_log (via merge e associação retroativa), mas ainda não há tela dedicada de visualização

## Fase F — Testes e confiabilidade

- [x] Testes unitários de domínio/validação consolidados
  - Critério de aceite: 142 testes unitários passando
  - Evidência: bun run test:unit — 142 unit tests green
- [x] Testes de integração implementados
  - Critério de aceite: suíte em **tests**/integration com cenários críticos de banco+ações
  - Evidência: **tests**/integration/domain-flows.test.ts — 16 testes cobrindo ciclos + dedup + amortização + tags + prioridade + fluxo completo
- [x] Testes e2e implementados
  - Critério de aceite: suíte em **tests**/e2e para fluxos principais
  - Evidência: **tests**/e2e/user-journeys.test.ts — 10 testes cobrindo cadastro, transações, fornecedores, faturas, transferências, empréstimos, recorrências, priorização, períodos financeiros

## Fase G — Prontidão para entrega contínua total

- [ ] Fluxo develop -> feature -> MR -> pipeline -> staging validado ponta a ponta
  - Critério de aceite: evidência em execução real do GitLab
- [ ] Promotion controlada para main/produção com gates ativos
  - Critério de aceite: deploy de produção manual controlado e comprovado
- [ ] Checklist de operação contínua assinado pelo time
  - Critério de aceite: todos os itens críticos de segurança/qualidade/deploy fechados

## Fase H — Segurança e Hardening Supabase (pós-auditoria 2026-03-22)

> Gaps críticos e importantes identificados na auditoria pós-implementação das Edge Functions e configurações do Supabase.

### H1. Gaps Críticos (bloqueiam segurança real)

- [x] RLS de audit_logs: substituir FOR ALL por FOR SELECT + FOR INSERT + trigger de imutabilidade
  - Critério de aceite: migration criada; `DELETE` em audit_logs levanta exception
  - Evidência: supabase/migrations/20260322180000_fix_audit_logs_immutability.sql — trigger `trg_audit_logs_immutable` + policies SELECT/INSERT
- [x] Storage: criar bucket `documents` e `imports` com policies de acesso por usuário
  - Critério de aceite: migration com `INSERT INTO storage.buckets` + policies de Storage RLS; bucket aparece no Studio
  - Evidência: supabase/migrations/20260322180100_create_storage_buckets.sql — buckets + 8 policies RLS de storage
- [x] user_secrets.encrypted_value: encriptar com pgcrypto em repouso
  - Critério de aceite: migration cria helpers pgp_sym_encrypt/pgp_sym_decrypt; segredos não são texto puro no banco
  - Evidência: supabase/migrations/20260322180200_encrypt_user_secrets.sql — funções encrypt_secret/decrypt_secret via pgcrypto

### H2. Gaps Importantes (antes de produção)

- [x] config.toml: `minimum_password_length` de 6 para 8 (dev) / 12 (prod)
  - Critério de aceite: valor alterado e documentado
  - Evidência: supabase/config.toml — `minimum_password_length = 8`
- [x] config.toml: `password_requirements = "lower_upper_letters_digits"`
  - Critério de aceite: valor configurado
  - Evidência: supabase/config.toml — `password_requirements = "lower_upper_letters_digits"`
- [x] config.toml: `enable_confirmations = true`
  - Critério de aceite: confirmação de e-mail ativa em dev/prod
  - Evidência: supabase/config.toml — `enable_confirmations = true`
- [x] config.toml: `secure_password_change = true`
  - Critério de aceite: reautenticação exigida para troca de senha
  - Evidência: supabase/config.toml — `secure_password_change = true`
- [ ] config.toml: Google OAuth configurado (`[auth.external.google]`)
  - Fase alvo: Sprint 3 (depende de CEO criar credenciais Google Cloud)
  - Critério de aceite: bloco provider Google presente; vars GOOGLE_CLIENT_ID/GOOGLE_SECRET no .env.example
  - Observações: CEO criou Gmail para este fim (ver TODO em prompts.md)
- [x] config.toml: SMTP configurado (Mailpit local + SendGrid/SES prod)
  - Critério de aceite: emails de auth funcionam localmente via Mailpit (http://127.0.0.1:54324)
  - Evidência: supabase/config.toml — `[auth.email.smtp]` com host=inbucket + porta 54325
- [ ] View materializada `mv_supplier_spending` criada
  - Fase alvo: Sprint 1 (2026-03-23)
  - Critério de aceite: migration com a view + índice único para REFRESH CONCURRENTLY
  - Observações: planejada na Etapa 5.4 mas não implementada; necessária para relatórios de performance
- [ ] Edge Function `retroactive-supplier-association`: confirm com transação atômica
  - Fase alvo: Sprint 1 (2026-03-23)
  - Critério de aceite: `handleConfirm` usa RPC PL/pgSQL atômica; sem risco de estado inconsistente no meio do lote
- [x] Edge Functions: adicionar CORS headers em merge-suppliers e retroactive-supplier-association
  - Critério de aceite: headers `Access-Control-Allow-Origin` presentes; preflight OPTIONS respondido
  - Evidência: supabase/functions/merge-suppliers/index.ts + retroactive-supplier-association/index.ts — corsHeaders em todas as respostas
- [ ] Edge Function `refresh-mv-supplier-spending` criada (scheduled/cron)
  - Fase alvo: Sprint 1 (2026-03-23)
  - Critério de aceite: função que executa `REFRESH MATERIALIZED VIEW CONCURRENTLY mv_supplier_spending`
  - Observações: terceira Edge Function planejada, depende de mv_supplier_spending existir
- [ ] seed.sql populado com dados base de desenvolvimento
  - Fase alvo: Sprint 2 (2026-03-24)
  - Critério de aceite: categorias base, tags comuns, usuário de teste no seed.sql
  - Observações: seed.sql existe mas está vazio — banco funcional mas vazio após db reset

### H3. Gaps Opcionais (melhorias de qualidade)

- [ ] MFA TOTP habilitado (`[auth.mfa.totp] enroll_enabled = true`)
- [ ] Session timebox configurado (`timebox = "24h"`, `inactivity_timeout = "8h"`)
- [ ] `[db.vault]` habilitado para melhor governança de segredos
- [ ] `[db.ssl_enforcement]` habilitado em produção
- [ ] PgBouncer habilitado em produção (`[db.pooler] enabled = true`)
- [ ] Email templates customizados (confirmação, reset, convite)
- [ ] hCaptcha/Turnstile para signup (`[auth.captcha]`)
- [ ] `OPENAI_API_KEY` documentado no `.env.example`

### H4. Supabase Local — Ambiente de Desenvolvimento

- [ ] Edge Runtime iniciado e Edge Functions aparecendo no Studio
  - Critério de aceite: `npx supabase functions serve` documentado; Studio mostra as funções
  - Observações: `supabase_edge_runtime_seu.bolso.feliz` fica stopped quando não explicitamente iniciado — rodar `npx supabase functions serve` manualmente em desenvolvimento
- [x] MCP do Supabase local configurado no VS Code
  - Critério de aceite: `.vscode/mcp.json` com endpoint `http://127.0.0.1:54321/mcp`
  - Evidência: .vscode/mcp.json criado em 2026-03-22
- [x] Storage buckets criados via migration (documentado acima em H1)
  - Critério de aceite: migration 20260322180100 aplicada; Studio mostra buckets ao rodar `npx supabase db reset`
  - Evidência: supabase/migrations/20260322180100_create_storage_buckets.sql
- [ ] Verificação manual no Studio (tabelas, RLS, buckets visíveis)
  - Critério de aceite: verificação visual confirmada por CEO ou engenheir@ responsável
  - Observações: requer `npx supabase start` ativo + Studio em http://127.0.0.1:54323

---

## Conclusão do Checklist

**Última atualização:** 2026-03-22 21:27 (pós-segunda auditoria)

### Estado consolidado por fase:

| Fase                                | Status                       | Observações                                |
| ----------------------------------- | ---------------------------- | ------------------------------------------ |
| A — Fundação e Domínio              | ✅ Concluída                 | Stack, princípios, ciclos, prioridade      |
| B — Dimensão Fornecedor             | ✅ Concluída                 | Entidades, alias, contratos, métricas, UI  |
| C — Ajustes obrigatórios            | ✅ Concluída                 | Deduplicação, consumption_metrics, aliases |
| D — Governança de Engenharia        | ✅ Concluída (D2 parcial)    | Deploy web real ainda placeholder          |
| E — Produto Web MVP                 | ✅ Concluída (E2.3 pendente) | Tela de auditoria histórica de fornecedor  |
| F — Testes e confiabilidade         | ✅ Concluída                 | 168 testes passando                        |
| G — Prontidão para entrega contínua | ⏳ Pendente                  | Deploy real + validação staging            |
| H — Segurança e Hardening           | ⚠️ Parcialmente concluída    | H1 ✅ · H2 parcial · H3 pendente           |

### Próximos sprints:

- **Sprint 1 (2026-03-23):** mv_supplier_spending + refresh Edge Function + confirm atômico
- **Sprint 2 (2026-03-24):** seed.sql + tela de auditoria histórica de fornecedor (E2.3)
- **Sprint 3 (a definir):** Google OAuth (depende de credenciais do CEO)
- **Sprint G (após Sprint 3):** Deploy web real, validação staging, promoção para produção
- Testes (142 unit + 16 integration + 10 e2e = 168 total): PRONTOS
- Páginas CRUD de fornecedores e relatórios com filtro por fornecedor: PRONTOS
- Segurança e hardening Supabase (Fase H): PENDENTE — 3 gaps críticos, 11 importantes
- Camada de fechamento operacional e governança de entrega contínua remota: PENDENTE (deploy real, promoção controlada, checklist operacional)
- Auditoria visual por fornecedor na interface: PENDENTE

Este checklist deve ser usado como contrato de execução para o próximo ciclo.
Próxima prioridade: Corrigir gaps H1 (críticos) antes de qualquer promoção para produção.
