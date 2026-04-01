---
Título da Reunião: Raio-X Completo — Estado do Projeto Seu Bolso Feliz
Data e Hora: 2026-03-31 17:29
Participantes:
  - Chico (CEO) — facilitador, decisão final
  - Ana Silva (Arquiteta de Software) — visão estrutural, coerência de stack
  - Carlos Mendes (Designer de Software) — design system, consistência visual
  - João Pereira (Backend Sênior — Node/Bun) — workers, ingestion pipeline
  - Maria Oliveira (Backend Sênior — Node/Bun) — segurança, testes, MCP
  - Roberto Lima (Frontend Sênior — React/Next) — Web App, componentes
  - Sofia Almeida (Frontend Sênior — React/Next) — páginas, server actions
  - Lucas Ferreira (Mobile Sênior — React Native/Expo) — app mobile
  - Fernando Gomes (DevOps Sênior) — CI/CD, deploy, ambientes
  - André Santos (DBA Sênior — PostgreSQL) — schema, migrations, performance
  - Ricardo Monteiro (Economista) — modelagem financeira, cálculos
  - Camila Duarte (Consultora de Finanças Pessoais) — validação de domínio, UX financeiro
  - Renata Silva (QA Visual/A11y) — qualidade, acessibilidade
  - Thiago Martins (Front Engineer) — componentização, tipagem, padrões
  - Isabella Torres (UI Designer) — hierarquia visual, micro-interações
  - Helena Vargas (UX/UI Specialist) — pesquisa de usuário, prototipagem
Pauta:
  - Raio-X completo e preciso do estado de cada módulo do projeto
  - O que foi feito em cada módulo
  - O que falta em cada módulo
  - Quais eram os próximos passos planejados
  - O que é prioridade agora
  - Documento para consultoria externa — máximo detalhamento
---

# Raio-X Completo — Estado do Projeto Seu Bolso Feliz

> **Objetivo**: Produzir o documento mais detalhado e preciso possível sobre o estado atual de cada módulo, código e intenção do projeto. Será usado como referência para consultoria e para clareza total da equipe sobre o que existe, o que falta e o que priorizar.

---

## Discussão

### Ana Silva (Arquiteta de Software)

Fiz uma varredura completa de todo o repositório. A arquitetura foi implementada exatamente como planejada nos refinos iniciais: monorepo com workspaces Bun, separação clara entre `packages/` (lógica pura), `apps/` (interfaces), `workers/` (processamento) e `supabase/` (infraestrutura). A coerência estrutural está excelente.

O que me preocupa é que temos **módulos em estágios de maturidade muito diferentes**. O backend/domínio está em 90-100%, mas o frontend está em estados variados — o web tem a casca mas falta carne, e o mobile basicamente não existe. Preciso que o documento reflita isso com precisão cirúrgica.

### André Santos (DBA Sênior)

Do lado do banco de dados, estamos muito bem. São **19 migrations** aplicadas, **30+ tabelas**, RLS em todas, indexes otimizados, views materializadas, RPCs úteis. O schema cobre 100% do domínio planejado incluindo o pipeline de ingestão completo. O seed.sql tem dados realistas para desenvolvimento. Não temos débito técnico no banco.

### Ricardo Monteiro (Economista)

Toda a modelagem financeira que planejamos está implementada e testada: SAC, Price, Misto, quitação antecipada, ciclos financeiros personalizados, priorização multi-fator, deduplicação transaction × statement_item. Os 33 testes de cálculos financeiros cobrem os cenários que definimos. A base está sólida para qualquer evolução futura.

### Camila Duarte (Consultora de Finanças Pessoais)

Do ponto de vista do usuário real — o CEO quer controlar suas finanças para zerar dívidas até o fim do ano. **Hoje ele não consegue usar o sistema para isso** porque a UI de aprovação de documentos não existe. Os dados entram pelo pipeline (Gmail, scanner local), viram rascunhos, mas ficam parados na fila sem interface para revisar e aprovar. Isso é o gargalo número 1. O dashboard existe mas precisa de dados aprovados para funcionar.

### Fernando Gomes (DevOps Sênior)

Ambientes estão criados: Supabase staging (`dcljzgjgnkmxdvhybvpt`) e produção (`opwelsgdhksuuewdbefk`), migrations aplicadas em ambos, Edge Functions deployadas. GitLab CI/CD tem 6 stages configurados. **Porém**: o deploy web para Vercel ainda é um **placeholder** — não faz deploy real. E o Google OAuth não está configurado em staging/produção, só funciona em localhost. Esses dois itens bloqueiam a validação real do sistema.

### João Pereira (Backend Sênior)

O pipeline de ingestão é o módulo mais complexo do projeto e está substancialmente pronto. O Gmail Scanner funciona: autentica via OAuth, baixa anexos, faz upload pro Storage, cria jobs. O worker de ingestão processa: baixa o blob, calcula hash, extrai texto de PDF (inclusive protegidos com senha), faz parsing (CEMIG, boleto), gera drafts. A máquina de estados tem 24 transições mapeadas. **O que falta**: teste end-to-end real com os 1000+ e-mails do CEO, e a integração do scan por período/query no Gmail.

### Maria Oliveira (Backend Sênior)

O MCP Server está funcional com 8 ferramentas implementadas. Posso confirmar que o fluxo `scan_local_folder → list_unparsed → approve_draft_batch` funciona. O `resolve_supplier_candidates` também. **Gaps**: o tool `scan_gmail_label` precisa de refactor para integrar no MCP, e o teste real no VS Code Copilot precisa acontecer (servidor inicia mas nunca foi usado de verdade pelo CEO).

### Roberto Lima (Frontend Sênior)

O app web tem **26 rotas** compiladas no Next.js App Router e **31 componentes** shadcn/ui configurados. O layout geral funciona: sidebar, header, providers (auth, theme, toast). Todas as rotas existem com suas páginas: `/dashboard`, `/dashboard/transactions`, `/dashboard/institutions`, `/dashboard/products`, `/dashboard/recurring`, `/dashboard/statements`, `/dashboard/liabilities`, `/dashboard/documents`, `/dashboard/import`, `/dashboard/reports`, `/dashboard/suppliers`, `/dashboard/settings`. **Server Actions** existem para CRUD de todas as entidades. O dashboard principal busca transações, ciclos de fatura, dívidas, recorrências.

**Mas preciso ser honesto**: muitas páginas são cascas funcionais — têm a estrutura, o layout, os componentes certos, mas a experiência do usuário real (formulários completos, validação visual, feedback, estados vazios, loading) está em estágios variados. Algumas estão mais polidas, outras são esqueleto.

### Sofia Almeida (Frontend Sênior)

Complementando o Roberto: as Server Actions existem para `institutions`, `products`, `transactions`, `recurring`, `suppliers`, `statement-cycles`, `liabilities`, `financial-periods`. Elas fazem CRUD completo via Supabase. O que falta mesmo é **a camada de ingestão no frontend** — as rotas `/dashboard/ingestion/*` simplesmente não existem. Não há tela para listar documentos ingeridos, ver drafts pendentes, fazer split-view PDF vs rascunho, ou aprovar em lote. Isso é o gap mais visível.

### Lucas Ferreira (Mobile Sênior)

Preciso ser direto: o app mobile **não foi iniciado**. Existe a estrutura de diretórios — `apps/mobile/src/` com pastas `components/`, `hooks/`, `lib/`, `navigation/`, `screens/` — todas vazias. O `package.json` tem Expo 52 + React 19 + React Native 0.76 configurados. É isso. Nenhuma tela, nenhum componente, nenhum hook. Isso está correto conforme o planejamento — mobile é Fase posterior ao MVP web.

### Renata Silva (QA Visual/A11y)

Os testes existentes cobrem bem a lógica de negócio: **16 arquivos de teste** entre unitários, integração e e2e. Mas **nenhuma auditoria de acessibilidade** foi feita nas 26 rotas web. Não temos testes visuais. Não temos testes de contraste, foco, navegação por teclado, estados vazios ou de erro. Está no planejamento mas não foi executado.

### Thiago Martins (Front Engineer)

A componentização está boa — 31 componentes shadcn/ui bem configurados, tipagem TypeScript correta com path aliases `@sbf/*`. O `tsconfig.base.json` está em strict mode com composite projects. O design system usa Radix UI + Tailwind CSS 4, que é a escolha certa. O `@sbf/ui-tokens` existe mas está praticamente vazio — é um stub com ~10 linhas. Isso precisa crescer quando formos padronizar tokens entre web e mobile.

---

## Estado Detalhado por Módulo

---

### 1. `packages/domain` — Lógica Pura de Domínio

**Status Geral**: ✅ **90% — Substancialmente Implementado**

| Submódulo           | Arquivo                        | Linhas | Status      | Descrição                                                                                                           |
| ------------------- | ------------------------------ | ------ | ----------- | ------------------------------------------------------------------------------------------------------------------- |
| **financial-cycle** | `src/financial-cycle/index.ts` | ~130   | ✅ Completo | `getCurrentPeriod()`, `generatePeriods()`, `findPeriodForDate()`, `daysRemainingInPeriod()`                         |
| **amortization**    | `src/amortization/index.ts`    | ~170   | ✅ Completo | SAC, Price, Misto, `simulateEarlyPayoff()`, `totalInterestPaid()`, `getOutstandingBalanceAfter()`                   |
| **deduplication**   | `src/deduplication/index.ts`   | ~100   | ✅ Completo | ADR-001 compliant. `deduplicateExpenses()`, `sumDeduplicatedExpenses()`, `getStatementComposition()`                |
| **priority**        | `src/priority/index.ts`        | ~90    | ✅ Completo | `deriveEffectivePriority()`, `calculateSortScore()`, `prioritizeItems()`, `filterByPriority()`, `groupByPriority()` |

**O que foi feito**: Todas as regras de negócio financeiro centrais estão implementadas e testadas. Ciclos personalizados, amortização (3 sistemas), deduplicação de fatura vs transação, priorização multi-fator.

**O que falta**: Nada crítico. Possível evolução futura: simulação de cenários "what-if" para quitação de múltiplas dívidas simultaneamente.

**Próximos passos planejados**: Nenhum imediato. O pacote está estável.

**Testes**: 33+ assertions de amortização, 8+ cenários de ciclo financeiro, 10+ regras de prioridade, 15+ regras de deduplicação. **Todos passando**.

---

### 2. `packages/validation` — Schemas Zod

**Status Geral**: ✅ **100% — Completo**

| Arquivo          | Linhas | Status      | Descrição                                                                                                                                                                |
| ---------------- | ------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/schemas.ts` | ~250+  | ✅ Completo | Schemas para: institutions, financial_products, cards, categories, tags, transactions, transfers, statement_cycles, liabilities, recurring_templates, documents, imports |
| `src/enums.ts`   | ~50    | ✅ Completo | Zod enums: institution_type, product_type, transaction_type, priority, liability_status, etc.                                                                            |

**O que foi feito**: Schemas completos para todas as entidades do domínio, com `.partial()` para updates, validação encadeada, mensagens de erro.

**O que falta**: Nada. Schemas de ingestão podem ser acrescentados quando a UI de ingestão for criada.

**Dependência**: `zod ~3.24.3`

---

### 3. `packages/shared-types` — Tipos TypeScript do Domínio

**Status Geral**: ✅ **100% — Completo**

| Arquivo                 | Linhas | Status         | Descrição                                                                    |
| ----------------------- | ------ | -------------- | ---------------------------------------------------------------------------- |
| `src/database.types.ts` | ~1678  | ✅ Auto-gerado | Tipos extraídos do schema Supabase via `supabase gen types`                  |
| `src/domain/index.ts`   | ~250+  | ✅ Completo    | 20+ aliases de entidade, helpers Row/Insert/Update, enums tipados, RPC types |

**O que foi feito**: Tipagem completa derivada do banco. Todas as entidades têm tipos Insert/Update/Row. Enums do banco estão mapeados para TypeScript.

**O que falta**: Reexecutar `generate-types.sh` se houver novas migrations (hoje está em sincronia).

---

### 4. `packages/operations` — Hash, Fingerprint, Idempotência

**Status Geral**: ✅ **100% — Completo**

| Arquivo              | Linhas | Status | Descrição                                                                                                         |
| -------------------- | ------ | ------ | ----------------------------------------------------------------------------------------------------------------- |
| `src/hash.ts`        | ~30    | ✅     | `computeContentHash()` (SHA-256 do arquivo bruto), `computeCanonicalFingerprint()` (SHA-256 do texto normalizado) |
| `src/origin-key.ts`  | ~30    | ✅     | `buildOriginKey()` — chave de deduplicação por origem (Gmail, local, upload)                                      |
| `src/idempotency.ts` | ~40    | ✅     | `checkIdempotency()` — previne reprocessamento de documentos já ingeridos                                         |

**O que foi feito**: Toda a infraestrutura de deduplicação e idempotência está implementada e testada.

**O que falta**: Nada.

---

### 5. `packages/ingestion-types` — Tipos do Pipeline de Ingestão

**Status Geral**: ✅ **100% — Completo**

| Arquivo        | Linhas | Status | Descrição                                                                                                                                                   |
| -------------- | ------ | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/enums.ts` | ~70    | ✅     | `IngestionRunStatus`, `IngestionJobStatus` (24 estados), `SourceDocumentOrigin`, `DraftRecordType`, `ParserType`, `DraftBatchStatus`                        |
| `src/types.ts` | ~150   | ✅     | Interfaces: IngestionRun, IngestionJob, SourceDocument, DocumentFingerprint, ParsedDocumentVersion, ExtractionResult, DraftRecord, DraftBatch, IngestionLog |

**O que foi feito**: Contratos de tipo completos para todo o pipeline de ingestão. Máquina de estados com 24 transições definida.

**O que falta**: Nada.

---

### 6. `packages/ui-tokens` — Tokens de Design System

**Status Geral**: ⚠️ **5% — Stub/Placeholder**

| Arquivo        | Linhas | Status  | Descrição                                                          |
| -------------- | ------ | ------- | ------------------------------------------------------------------ |
| `src/index.ts` | ~10    | ⚠️ Stub | Praticamente vazio. Placeholder para tokens Tailwind/Radix futuros |

**O que foi feito**: Pacote criado e registrado no workspace.

**O que falta**: Definir tokens de cor, espaçamento, tipografia, breakpoints e sombras que sejam compartilhados entre web e mobile. Deve refletir o design system escolhido (Radix + Tailwind).

**Próximos passos planejados**: Será preenchido quando o design system for padronizado e o mobile iniciar.

---

### 7. `packages/config` — Configurações Compartilhadas

**Status Geral**: ✅ **100% — Funcional**

Contém configurações de ESLint, TypeScript e Vitest reutilizáveis entre apps e packages. Funcionando corretamente.

---

### 8. `apps/web` — Aplicação Web (Next.js 15 + React 19 + Tailwind 4)

**Status Geral**: ⚠️ **60% — Estrutura completa, UX parcial, ingestão ausente**

#### 8.1. Infraestrutura Web

| Item                  | Status | Detalhe                                                                                                                                                                                                                                                                                                                              |
| --------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Next.js 15 App Router | ✅     | Turbopack ativado para dev                                                                                                                                                                                                                                                                                                           |
| React 19              | ✅     | Última versão                                                                                                                                                                                                                                                                                                                        |
| Tailwind CSS 4        | ✅     | Configurado com PostCSS                                                                                                                                                                                                                                                                                                              |
| Radix UI (shadcn/ui)  | ✅     | 31 componentes instalados: button, input, select, dialog, sheet, tabs, table, dropdown-menu, popover, calendar, command, badge, separator, skeleton, scroll-area, tooltip, avatar, switch, card, label, form, textarea, checkbox, radio-group, progress, accordion, alert-dialog, collapsible, navigation-menu, toggle, toggle-group |
| Supabase SSR          | ✅     | Client + Server helpers configurados                                                                                                                                                                                                                                                                                                 |
| Auth (Login)          | ✅     | Página de login, callback, providers                                                                                                                                                                                                                                                                                                 |
| Tema Dark/Light       | ✅     | next-themes configurado                                                                                                                                                                                                                                                                                                              |
| Toast (Sonner)        | ✅     | Notificações configuradas                                                                                                                                                                                                                                                                                                            |
| Layout Dashboard      | ✅     | Sidebar + Header + Content area                                                                                                                                                                                                                                                                                                      |
| Formatação            | ✅     | `formatCurrency()`, `formatDate()` em `lib/format.ts`                                                                                                                                                                                                                                                                                |

#### 8.2. Rotas e Páginas Web (26 rotas compiladas)

| Rota                      | Status            | O que existe                                                        | O que falta                                                            |
| ------------------------- | ----------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `/`                       | ✅                | Landing page básica                                                 | Conteúdo marketing (baixa prioridade)                                  |
| `/login`                  | ✅                | Formulário de auth                                                  | Funcional                                                              |
| `/auth/*`                 | ✅                | Callbacks OAuth                                                     | Funcional                                                              |
| `/dashboard`              | ✅ Parcial        | Busca transações, ciclos, dívidas, recorrências, despesas, receitas | Cards de resumo, gráficos, "primeira tela" operacional com priorização |
| `/dashboard/institutions` | ✅ Parcial        | CRUD via Server Actions                                             | Formulário polido, estados vazios                                      |
| `/dashboard/products`     | ✅ Parcial        | CRUD via Server Actions                                             | UX de associação com instituição, validação visual                     |
| `/dashboard/transactions` | ✅ Parcial        | Listagem + CRUD                                                     | Filtros avançados, paginação, categorização rápida                     |
| `/dashboard/recurring`    | ✅ Parcial        | Listagem + CRUD                                                     | Templates visuais, geração de instâncias                               |
| `/dashboard/statements`   | ✅ Parcial        | Ciclos de fatura                                                    | Composição de fatura, pagamento parcial                                |
| `/dashboard/liabilities`  | ✅ Parcial        | Dívidas + CRUD                                                      | Cronograma visual, simulador de quitação                               |
| `/dashboard/documents`    | ✅ Parcial        | Upload/listagem                                                     | Visualizador de PDF inline, classificação                              |
| `/dashboard/import`       | ✅ Parcial        | Importação CSV                                                      | Detecção automática de colunas funcional                               |
| `/dashboard/reports`      | ✅ Parcial        | Relatórios por período                                              | Gráficos, análise por tag/categoria/fornecedor                         |
| `/dashboard/suppliers`    | ✅ Parcial        | Gestão de fornecedores                                              | Merge visual, auditoria histórica                                      |
| `/dashboard/settings`     | ✅ Parcial        | Preferências                                                        | Configuração de ciclo financeiro visual                                |
| `/dashboard/ingestion/*`  | ❌ **NÃO EXISTE** | —                                                                   | **Toda a UI de ingestão precisa ser criada**                           |

#### 8.3. Server Actions (8 arquivos)

| Action                 | Status  | Arquivo                                |
| ---------------------- | ------- | -------------------------------------- |
| `institutions.ts`      | ✅ CRUD | `src/app/actions/institutions.ts`      |
| `products.ts`          | ✅ CRUD | `src/app/actions/products.ts`          |
| `transactions.ts`      | ✅ CRUD | `src/app/actions/transactions.ts`      |
| `recurring.ts`         | ✅ CRUD | `src/app/actions/recurring.ts`         |
| `suppliers.ts`         | ✅ CRUD | `src/app/actions/suppliers.ts`         |
| `statement-cycles.ts`  | ✅ CRUD | `src/app/actions/statement-cycles.ts`  |
| `liabilities.ts`       | ✅ CRUD | `src/app/actions/liabilities.ts`       |
| `financial-periods.ts` | ✅ CRUD | `src/app/actions/financial-periods.ts` |

#### 8.4. O que falta no Web

**Prioridade ALTA** (bloqueia uso real):

1. **UI de Ingestão completa** — `/dashboard/ingestion` (visão geral), `/dashboard/ingestion/documents` (listagem), `/dashboard/ingestion/drafts` (rascunhos), `/dashboard/ingestion/drafts/[id]` (split-view PDF | draft)
2. **Aprovação de drafts** — formulário de revisão, aprovação individual e em lote
3. **PDF viewer inline** — exibir o documento original ao lado do rascunho
4. **Dashboard operacional completo** — "primeira tela" com vencimentos, prioridades, período financeiro, essenciais vs postergáveis

**Prioridade MÉDIA**: 5. Filtros avançados em transações (por tag, categoria, fornecedor, período) 6. Gráficos e visualizações nos relatórios 7. Simulador visual de quitação antecipada 8. Cronograma visual de amortização 9. Composição de fatura expandida 10. Merge visual de fornecedores 11. Auditoria histórica de fornecedor (dados existem, UI falta)

**Prioridade BAIXA**: 12. A11y audit completo (26 rotas) 13. Testes visuais 14. States: empty, loading, error em todas as páginas 15. Atalhos de teclado na tela de ingestão

---

### 9. `apps/mobile` — App Mobile (React Native + Expo)

**Status Geral**: ⛔ **0% — Não iniciado (conforme planejamento)**

| Item                    | Status | Detalhe                                                            |
| ----------------------- | ------ | ------------------------------------------------------------------ |
| Estrutura de diretórios | ✅     | `components/`, `hooks/`, `lib/`, `navigation/`, `screens/` criados |
| package.json            | ✅     | Expo 52 + React 19 + React Native 0.76                             |
| Telas                   | ❌     | Nenhuma tela implementada                                          |
| Componentes             | ❌     | Nenhum componente implementado                                     |
| Navigation              | ❌     | Nenhuma rota configurada                                           |
| Integração Supabase     | ❌     | Não configurada                                                    |

**O que foi feito**: Boilerplate de diretórios e dependências.

**O que falta**: Tudo. Mobile é planejado para **DEPOIS** do MVP web funcional.

**Próximos passos planejados**: Iniciar após web estar validada em staging. Telas prioritárias: Home (vencimentos + prioridades), Transações (listagem + adição rápida), Configurações.

---

### 10. `apps/mcp-server` — Servidor MCP (Model Context Protocol)

**Status Geral**: ✅ **80% — Funcional, 8 ferramentas, falta integração Gmail**

| Ferramenta                        | Arquivo                                        | Status | Descrição                                                            |
| --------------------------------- | ---------------------------------------------- | ------ | -------------------------------------------------------------------- |
| `scan_local_folder`               | `src/tools/scan_local_folder.ts`               | ✅     | Escaneia diretório local por documentos (PDF, imagem, CSV, XLS, OFX) |
| `list_unparsed_documents`         | `src/tools/list_unparsed_documents.ts`         | ✅     | Lista documentos aguardando processamento                            |
| `reprocess_document`              | `src/tools/reprocess_document.ts`              | ✅     | Reprocessa documentos com erro                                       |
| `resolve_supplier_candidates`     | `src/tools/resolve_supplier_candidates.ts`     | ✅     | Resolve ambiguidades de fornecedor                                   |
| `list_draft_batches`              | `src/tools/list_draft_batches.ts`              | ✅     | Mostra lotes de rascunhos pendentes                                  |
| `approve_draft_batch`             | `src/tools/approve_draft_batch.ts`             | ✅     | Aprova e posta rascunhos                                             |
| `find_documents_without_password` | `src/tools/find_documents_without_password.ts` | ✅     | Identifica PDFs protegidos que precisam de senha                     |
| `recompute_financial_periods`     | `src/tools/recompute_financial_periods.ts`     | ✅     | Recalcula ciclos financeiros personalizados                          |

**O que foi feito**: 8 ferramentas implementadas e registradas. Servidor inicia via stdio transport. Scripts de execução no package.json.

**O que falta**:

1. **`scan_gmail_label`** — ferramenta para disparar scan de Gmail por label (precisa refactor do worker para ser invocável como função)
2. **Teste real no VS Code Copilot** — servidor inicia mas nunca foi testado end-to-end pelo CEO
3. **Documentação das ferramentas** para o Copilot/ChatGPT consumir

**Próximos passos planejados**: Integrar scan_gmail_label, validar com CEO, preparar para Fase 8 (ChatGPT prep).

---

### 11. `supabase/` — Infraestrutura de Banco de Dados

**Status Geral**: ✅ **100% — Completo para MVP**

#### 11.1. Migrations (19 aplicadas)

| #   | Migration                                  | Propósito                                                                                                                                                                                        | Status |
| --- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------ |
| 1   | `create_core_foundation_tables`            | `user_financial_preferences`, `institutions`, `financial_products`, `cards`, `categories`, `tags`, `financial_periods`, `statement_cycles`, `liabilities` + extensões pg_trgm, pgcrypto          | ✅     |
| 2   | `create_supplier_tables`                   | `suppliers`, `supplier_aliases`, `supplier_contracts`, `consumption_metrics`                                                                                                                     | ✅     |
| 3   | `alter_tables_add_supplier_refs`           | FKs: transactions/liabilities/recorrências → suppliers                                                                                                                                           | ✅     |
| 4   | `create_indexes`                           | B-tree em (user_id, created_at), (user_id, type), composites. GIN/trigram para busca textual                                                                                                     | ✅     |
| 5   | `create_triggers`                          | Auto `updated_at`, audit log triggers, refresh de materialized views                                                                                                                             | ✅     |
| 6   | `create_rls_policies`                      | RLS em todas as tabelas: cada usuário vê apenas seus dados                                                                                                                                       | ✅     |
| 7   | `create_views_and_rpcs`                    | View `v_expenses_deduplicated` (ADR-001), RPCs: `search_suppliers`, `calculate_outstanding_balance`, `find_or_create_supplier`                                                                   | ✅     |
| 8   | `create_merge_suppliers_rpc`               | RPC `merge_suppliers(source, target)`                                                                                                                                                            | ✅     |
| 9   | `fix_audit_logs_immutability`              | Audit logs não permitem UPDATE/DELETE                                                                                                                                                            | ✅     |
| 10  | `create_storage_buckets`                   | Buckets: `ingestion-originals`, `documents`                                                                                                                                                      | ✅     |
| 11  | `encrypt_user_secrets`                     | Tabela `user_secrets` com criptografia para senhas de PDF, API keys                                                                                                                              | ✅     |
| 12  | `create_mv_supplier_spending`              | Materialized view de gasto mensal por fornecedor                                                                                                                                                 | ✅     |
| 13  | `create_rpc_confirm_supplier_associations` | RPC para confirmar associações automáticas                                                                                                                                                       | ✅     |
| 14  | `create_ingestion_tables`                  | 9 tabelas: `ingestion_runs`, `ingestion_jobs`, `source_documents`, `document_fingerprints`, `parsed_document_versions`, `extraction_results`, `draft_records`, `draft_batches`, `ingestion_logs` | ✅     |
| 15  | `create_ingestion_rls_policies`            | RLS para tabelas de ingestão                                                                                                                                                                     | ✅     |
| 16  | `create_ingestion_indexes`                 | Indexes para status lookups e filtragem                                                                                                                                                          | ✅     |
| 17  | `create_storage_bucket_ingestion`          | Bucket `ingestion-originals`                                                                                                                                                                     | ✅     |
| 18  | `add_content_hash_dedup`                   | Coluna content_hash + unique per user                                                                                                                                                            | ✅     |
| 19  | Última migration recente                   | Ajustes incrementais                                                                                                                                                                             | ✅     |

**Total de tabelas**: 30+ (domínio financeiro + pipeline de ingestão + audit + secrets)

#### 11.2. Edge Functions (3 deployadas)

| Function                           | Propósito                                                      | Status                         |
| ---------------------------------- | -------------------------------------------------------------- | ------------------------------ |
| `merge-suppliers`                  | Merge atômico de fornecedores duplicados                       | ✅ Deployada em staging + prod |
| `refresh-mv-supplier-spending`     | Refresh da materialized view de gastos                         | ✅ Deployada                   |
| `retroactive-supplier-association` | Vincula transações passadas a fornecedores recém-identificados | ✅ Deployada                   |

#### 11.3. Seed Data

`seed.sql` (~500 linhas) inclui: usuário teste, 3 instituições (Caixa/Nubank/C6), 7+ produtos financeiros, 8 categorias, 7 fornecedores, 2 períodos financeiros, 2 ciclos de fatura, transações realistas, templates recorrentes e dívidas.

#### 11.4. Ambientes Remotos

| Ambiente     | Project Ref            | Status       | Migrations | Edge Functions | Auth                            |
| ------------ | ---------------------- | ------------ | ---------- | -------------- | ------------------------------- |
| **Local**    | —                      | ✅ Funcional | 19/19      | N/A            | Magic link + Google OAuth       |
| **Staging**  | `dcljzgjgnkmxdvhybvpt` | ✅ Ativo     | 19/19      | 3/3 deployadas | ⚠️ Google OAuth NÃO configurado |
| **Produção** | `opwelsgdhksuuewdbefk` | ✅ Ativo     | 19/19      | 3/3 deployadas | ⚠️ Google OAuth NÃO configurado |

**O que falta**:

1. Google OAuth em staging/produção (depende do CEO)
2. Secrets no Vault remoto (refresh token Gmail, senhas de PDF)
3. Validação end-to-end em staging com dados reais

---

### 12. `workers/gmail-scanner` — Scanner de E-mail Gmail

**Status Geral**: ✅ **85% — Funcional, falta teste massivo**

| Arquivo                    | Linhas | Status | Descrição                                                            |
| -------------------------- | ------ | ------ | -------------------------------------------------------------------- |
| `src/index.ts`             | ~200   | ✅     | CLI com opções: --label, --limit, --dry-run, --batch-size            |
| `src/gmail-client.ts`      | ~120   | ✅     | `listMessages(label)`, `getMessage(id)`, `getAttachment(id, attId)`  |
| `src/message-processor.ts` | ~150   | ✅     | Extrai anexos + metadados (from, subject, date), `decodeBase64Url()` |
| `src/supabase.ts`          | ~20    | ✅     | Client init                                                          |
| `get-token.ts`             | ~80    | ✅     | Setup OAuth (gera refresh token)                                     |

**O que foi feito**:

- Autenticação OAuth2 com Google completa
- Listagem de mensagens por label
- Download de anexos (PDF, imagem, etc.)
- Cálculo de origin_key + content_hash
- Upload para Supabase Storage
- Criação de source_documents + ingestion_jobs
- Modo dry-run para teste seguro
- Deduplicação por content_hash (não reprocessa o mesmo arquivo)

**O que falta**:

1. **Teste real com 1000+ e-mails do CEO** (nunca executado em escala)
2. **Scan por período** (filtrar por data — backfill histórico)
3. **Scan por query livre** (buscar por termos além de label)
4. **Tratamento de rate limiting** do Gmail API em volumes grandes

**Próximos passos planejados**: CEO gera refresh token → armazena no Vault → executa `--limit 10` → depois `--limit 100` → depois sem limite.

---

### 13. `workers/ingestion` — Pipeline Core de Ingestão

**Status Geral**: ✅ **75% — Pipeline funcional, falta OpenAI**

| Arquivo                             | Linhas | Status | Descrição                                                                                        |
| ----------------------------------- | ------ | ------ | ------------------------------------------------------------------------------------------------ |
| `src/index.ts`                      | ~100   | ✅     | Polling loop (5s), batch processing (10 jobs), graceful shutdown                                 |
| `src/processor.ts`                  | ~400   | ✅     | State machine: discovered → downloaded → hashed → queued → parsing → parsed                      |
| `src/state-machine.ts`              | ~80    | ✅     | 24 transições válidas, recovery de FAILED                                                        |
| `src/parsers/text-extractor.ts`     | ~80    | ✅     | pdf-parse, PDFs protegidos com senha do user_secrets                                             |
| `src/parsers/parse-orchestrator.ts` | ~180   | ✅     | Detect parser → extract → persist parsed_document_versions + extraction_results                  |
| `src/parsers/cemig-parser.ts`       | ~100   | ✅     | Regex extraction de contas CEMIG (valor, vencimento, consumo kWh)                                |
| `src/parsers/boleto-parser.ts`      | ~80    | ✅     | Regex extraction de boletos genéricos (valor, código de barras)                                  |
| `src/parsers/secret-lookup.ts`      | ~40    | ✅     | Busca senha de PDF no user_secrets                                                               |
| `src/drafts/draft-generator.ts`     | ~120   | ✅     | Classifica → gera draft_records (transaction, recurring_template, consumption_metric, liability) |
| `src/logger.ts`                     | ~30    | ✅     | Persiste ingestion_logs                                                                          |

**O que foi feito**:

- Pipeline completo: download → hash → dedup → parse → draft
- 2 parsers regex (CEMIG, boleto)
- Abertura de PDFs protegidos com senha
- Geração de drafts com score de confiança
- Sugestão automática de fornecedor/categoria/tags
- Máquina de estados com transições atômicas
- Idempotência (não reprocessa duplicatas)
- Logging em banco

**O que falta**:

1. **Parser OpenAI Vision** — hooks existem (`OPENAI_VISION`, `OPENAI_TEXT`) mas não implementados (bloqueado por API key)
2. **Sugestão de período financeiro** — integração com `@sbf/domain` para associar draft ao período correto
3. **Mais parsers regex** — outros tipos de conta (água COPASA, telefone Claro, etc.)
4. **Reconciliação avançada** — cruzar extratos bancários com transações manuais

**Próximos passos planejados**: Testar com documentos reais do CEO → identificar documentos que falham → criar parsers específicos → depois integrar OpenAI para documentos genéricos.

---

### 14. `workers/local-scanner` — Scanner de Pasta Local

**Status Geral**: ⚠️ **Implementado via MCP** (não é worker standalone)

A funcionalidade de scan local existe como ferramenta MCP (`scan_local_folder`). Não existe como worker standalone que monitora um diretório automaticamente. Para o MVP, o fluxo é: CEO usa MCP no VS Code → aponta para pasta → documentos são ingeridos.

**O que falta**: Worker standalone com watch mode (monitoramento contínuo de diretório). Baixa prioridade.

---

### 15. `__tests__/` — Suite de Testes

**Status Geral**: ✅ **85% — Cobertura sólida do domínio, lacunas em UI e a11y**

#### 15.1. Testes Unitários (domain/) — 12 arquivos

| Teste                         | Cenários | Status | Cobre                                                       |
| ----------------------------- | -------- | ------ | ----------------------------------------------------------- |
| `amortization.test.ts`        | ~10      | ✅     | SAC, Price, Misto, quitação antecipada, totalInterestPaid   |
| `financial-cycle.test.ts`     | ~8       | ✅     | getCurrentPeriod, edge cases (mês/ano), clamping            |
| `priority.test.ts`            | ~10      | ✅     | Manual override > tags, sortScore, overdue handling         |
| `deduplication.test.ts`       | ~15      | ✅     | ADR-001 completo: statement_payment ≠ expense, orphan items |
| `hash.test.ts`                | ~4       | ✅     | SHA-256 content + canonical fingerprint                     |
| `origin-key.test.ts`          | ~4       | ✅     | Origin keys por source type                                 |
| `idempotency.test.ts`         | ~3       | ✅     | Prevent reprocessing                                        |
| `state-machine.test.ts`       | ~6       | ✅     | Valid/invalid transitions                                   |
| `supplier-validation.test.ts` | ~4       | ✅     | Name normalization, alias matching                          |
| `validation-schemas.test.ts`  | ~6       | ✅     | Zod parsing + error messages                                |
| `parsers.test.ts`             | ~8       | ✅     | CEMIG regex, boleto regex                                   |
| `draft-generation.test.ts`    | ~5       | ✅     | Extraction → draft type classification                      |
| `setup.test.ts`               | ~2       | ✅     | Test infrastructure verification                            |

#### 15.2. Testes de Integração — 2 arquivos

| Teste                        | Cenários | Status | Cobre                                              |
| ---------------------------- | -------- | ------ | -------------------------------------------------- |
| `ingestion-pipeline.test.ts` | ~8       | ✅     | E2E: upload → hash → parse → draft → review → post |
| `domain-flows.test.ts`       | ~6       | ✅     | Cenários financeiros realistas multi-entidade      |

#### 15.3. Testes E2E — 1 arquivo

| Teste                   | Cenários | Status | Cobre                                                                     |
| ----------------------- | -------- | ------ | ------------------------------------------------------------------------- |
| `user-journeys.test.ts` | ~10      | ✅     | Login → scan → review → post → dashboard; create liability → amortization |

#### 15.4. Configuração

- **Vitest** com 3 profiles: unit (rápidos), integration (banco), e2e (completo)
- **Coverage**: v8 provider para `domain/`, `validation/`, `operations/`
- **Path aliases**: configurados para `@sbf/*`

#### 15.5. O que falta em Testes

1. **Testes visuais** (nenhum)
2. **Testes de acessibilidade** (nenhum)
3. **Testes de UI/componentes** (nenhum com React Testing Library)
4. **Teste de PDF com senha real** (precisa de arquivo protegido real)
5. **Teste de integração MCP no Copilot** (parcialmente feito)
6. **Testes de regressão visual** (nenhum)

---

### 16. `src/` — Aplicação Demo/Sandbox (Root)

**Status Geral**: ⚠️ **Não faz parte do MVP — é artefato do setup Bun**

| Arquivo                               | Descrição                                         |
| ------------------------------------- | ------------------------------------------------- |
| `App.tsx`                             | Demo UI do Bun (logo, animações)                  |
| `APITester.tsx`                       | Client HTTP para testar endpoints                 |
| `frontend.tsx`                        | Entry point SSR estático                          |
| `index.ts`, `index.html`, `index.css` | Entry points                                      |
| `components/ui/*`                     | Componentes duplicados (não usados pelo app real) |

**Intenção**: Era o template default do `bun init`. **Não deve ser mantido** a longo prazo. O app real está em `apps/web/`.

---

### 17. CI/CD e DevOps

**Status Geral**: ⚠️ **70% — Pipeline existe, deploy web é placeholder**

| Item                 | Status             | Detalhe                                                           |
| -------------------- | ------------------ | ----------------------------------------------------------------- |
| GitLab CI/CD         | ✅                 | 6 stages: validate, install, check, test, build, deploy           |
| Variáveis protegidas | ✅                 | SUPABASE_URL, SUPABASE_ANON_KEY, SERVICE_ROLE_KEY em staging/prod |
| Stage validate       | ✅                 | Lint + type check                                                 |
| Stage install        | ✅                 | `bun install`                                                     |
| Stage check          | ✅                 | ESLint + TSC                                                      |
| Stage test           | ✅                 | Vitest (unit + integration)                                       |
| Stage build          | ✅                 | `next build`                                                      |
| Stage deploy         | ⚠️ **PLACEHOLDER** | Não executa `vercel deploy` — precisa de Vercel CLI + token       |

**O que falta**:

1. **Substituir placeholder por Vercel CLI** (D.1 — 2h de trabalho)
2. **Validar pipeline completo em staging** (D.2)
3. **Google OAuth em staging** (D.5 — depende do CEO)
4. **Branch protection rules** no GitLab (develop → staging → main)

---

### 18. Documentação

**Status Geral**: ✅ **80% — Bem documentado, falta atualização pós-refinos recentes**

| Pasta                 | Conteúdo                                                             | Status                         |
| --------------------- | -------------------------------------------------------------------- | ------------------------------ |
| `docs/adrs/`          | 4 ADRs (deduplicação, consumption_metrics, aliases, CI/CD)           | ✅ Completo                    |
| `docs/aprenda/`       | 3 documentos educacionais (agentes, ingestão, arquitetura)           | ✅ Completo                    |
| `docs/checklists/`    | 4 checklists (geral, Verônica, ingestão, gaps)                       | ✅ Mas 004 precisa atualização |
| `docs/passo-a-passo/` | 5 guias práticos (Supabase, ingestão, ações CEO, fluxo, dashboard)   | ✅ Completo                    |
| `docs/planejamento/`  | 6 planos (implementação, CI/CD, gaps, segurança, ingestão, execução) | ✅ Completo                    |
| `docs/refinos/`       | 14 atas de reunião (21/03 a 26/03)                                   | ✅ Completo                    |
| `docs/Veronica/`      | Prompt inicial + especificação funcional                             | ✅ Completo                    |

**O que falta**: Este documento (raio-x 31/03) atualiza o estado. Checklist 004 precisa ser reconciliado com o estado real de hoje.

---

## Prós e Contras — Opções para Próximos Passos

### Opção A: Priorizar UI de Ingestão (recomendação da equipe)

**Prós:**

- Desbloqueia o uso real do sistema pelo CEO
- Pipeline backend já funciona — falta só a interface
- Dados já entram (Gmail scan, local scan) — ficam parados sem aprovação
- Permite validar todo o fluxo end-to-end
- Maior impacto imediato no objetivo "zerar dívidas até fim do ano"

**Contras:**

- ~80h de trabalho frontend (2 semanas com Roberto + Sofia)
- Precisa de design antes de codificar (split-view PDF | draft)
- Atrasa outras melhorias nas páginas existentes

### Opção B: Priorizar Deploy Real + Validação Staging

**Prós:**

- Sistema online de verdade (acessível fora do localhost)
- Valida todo o stack em ambiente real
- Identifica problemas de infra cedo
- 2-4h de trabalho (Fernando + CEO)

**Contras:**

- De nada adianta deploy se não tem UI de ingestão
- Google OAuth precisa do CEO para configurar
- Pode gerar falsa sensação de "pronto"

### Opção C: Abordagem Combinada (B primeiro, depois A)

**Prós:**

- Deploy real em 1 dia (desbloqueia acesso externo)
- UI de ingestão em paralelo (Roberto + Sofia)
- CEO pode usar Supabase Studio + MCP enquanto espera a UI
- Máximo progresso em paralelo

**Contras:**

- Precisa coordenar duas frentes simultâneas
- CEO precisa atuar em D.5 (Google OAuth) e A.3 (Gmail refresh token)

### Opção D: Focar em Dados Reais do CEO Primeiro

**Prós:**

- Validar que o pipeline funciona com dados reais antes de polir UI
- Descobrir bugs de parsing em documentos reais
- Gerar massa de dados reais para popular o dashboard
- 4h de trabalho (CEO executa scan, time analisa erros)

**Contras:**

- Sem UI, CEO precisa aprovar via Studio/MCP (atrito alto)
- Pode descobrir muitos problemas de uma vez
- Precisa de refresh token Gmail (bloqueador do CEO)

---

## Decisão Final

**Decisão**: Opção C modificada — Deploy rápido + Dados reais + UI de ingestão em paralelo

**Justificativa**: A equipe concorda que o maior valor está em desbloquear o uso real do sistema. O caminho mais eficiente é:

1. **Imediato (esta semana)**:
   - D.1: Fernando substitui placeholder de deploy por Vercel CLI (2h)
   - A.1-A.5: CEO gera refresh token, executa primeiro scan Gmail real (4h total)
   - CEO testa aprovação via MCP/Studio enquanto UI é desenvolvida

2. **Próxima semana (07-11 Abril)**:
   - E.1-E.8: Roberto + Sofia iniciam UI de ingestão (40h — listagem + split-view)
   - João + Maria criam mais parsers baseados nos erros do scan real do CEO
   - Fernando valida staging end-to-end

3. **Semana seguinte (14-18 Abril)**:
   - E.9-E.15: UI completa com aprovação em lote + atalhos (40h)
   - Processamento dos 1000+ documentos do CEO
   - Relatórios com dados reais no dashboard

4. **Abril/Maio**:
   - Dashboard operacional "primeira tela" com dados reais
   - Testes de a11y nas 26 rotas
   - Integração OpenAI (quando API key disponível)
   - Mobile: primeiras 3 telas

---

## Ações / Responsáveis / Prazo

| #   | Ação                                             | Responsável     | Prazo      | Bloqueador          |
| --- | ------------------------------------------------ | --------------- | ---------- | ------------------- |
| 1   | Substituir placeholder deploy-web por Vercel CLI | Fernando        | 2026-04-04 | —                   |
| 2   | Gerar refresh token Gmail + armazenar no Vault   | CEO             | 2026-04-04 | Script get-token.ts |
| 3   | Primeiro scan Gmail real (`--limit 10`)          | CEO             | 2026-04-04 | Item 2              |
| 4   | Testar fluxo MCP completo (scan → approve)       | CEO + Maria     | 2026-04-04 | Item 3              |
| 5   | Configurar Google OAuth em staging               | CEO             | 2026-04-07 | Credenciais Google  |
| 6   | Iniciar UI de ingestão (listagem + split-view)   | Roberto + Sofia | 2026-04-11 | —                   |
| 7   | Novos parsers baseados em erros reais            | João + Maria    | 2026-04-11 | Item 3              |
| 8   | Validar staging end-to-end                       | Fernando        | 2026-04-11 | Itens 1, 5          |
| 9   | UI completa: aprovação em lote + atalhos         | Roberto + Sofia | 2026-04-18 | Item 6              |
| 10  | Processamento massivo (1000+ docs CEO)           | João + CEO      | 2026-04-18 | Item 7              |
| 11  | Dashboard operacional com dados reais            | Roberto + Sofia | 2026-04-25 | Item 10             |
| 12  | Auditoria a11y (26 rotas)                        | Renata          | 2026-04-25 | —                   |
| 13  | Configurar API key OpenAI                        | CEO             | 2026-04-30 | Decisão CEO         |
| 14  | Integrar parser OpenAI Vision                    | João + Maria    | 2026-05-09 | Item 13             |
| 15  | Primeiras 3 telas mobile                         | Lucas + Beatriz | 2026-05-16 | Item 11             |

---

## Resumo Executivo para Consultoria

### O que está PRONTO (80%):

- **Arquitetura**: Monorepo com Bun, 7 packages internos, 3 apps, 3 workers
- **Banco de dados**: 30+ tabelas, 19 migrations, RLS completo, 3 Edge Functions, views materializadas, RPCs, indexes otimizados, seed data realista
- **Lógica de negócio**: Ciclos financeiros personalizados, amortização (SAC/Price/Misto), deduplicação (ADR-001), priorização multi-fator — tudo implementado e testado
- **Pipeline de ingestão**: Gmail OAuth → download anexos → hash/dedup → parse PDF → gerar drafts — funcional
- **MCP Server**: 8 ferramentas para operação via VS Code/Copilot
- **Testes**: 168+ assertions em 16 suites (unitário, integração, e2e)
- **Validação**: Zod schemas completos para todas as entidades
- **Tipagem**: TypeScript strict com tipos derivados do banco

### O que FALTA (20%):

- **UI de Ingestão** (❌ não existe) — telas para revisar e aprovar documentos ingeridos (~80h)
- **Deploy real** (⚠️ placeholder) — precisa conectar Vercel CLI (~2h)
- **Google OAuth em staging/prod** (⚠️ não configurado) — depende do CEO
- **Teste com dados reais** (⚠️ nunca feito em escala) — precisa de refresh token Gmail
- **Dashboard operacional "primeira tela"** (⚠️ parcial) — faltam dados reais + gráficos + priorização visual
- **Parser OpenAI** (❌ bloqueado) — sem API key
- **App Mobile** (❌ não iniciado) — planejado para depois do MVP web
- **Auditoria a11y** (❌ não feita) — 26 rotas sem verificação
- **Testes visuais/UI** (❌ nenhum) — sem React Testing Library

### Bloqueadores que dependem do CEO:

1. Gerar refresh token Gmail (script pronto, CEO executa)
2. Armazenar secrets no Vault do Supabase (manual)
3. Configurar Google OAuth em staging/produção
4. Autorizar deploy via Vercel CLI
5. Fornecer API key OpenAI (quando decidir)

### Prioridade Sequencial Recomendada:

```
SEMANA 1 (31 Mar - 04 Abr): Deploy real + primeiro scan Gmail + validação MCP
SEMANA 2 (07 - 11 Abr): UI de ingestão mínima + novos parsers
SEMANA 3 (14 - 18 Abr): UI completa + processamento massivo
SEMANA 4 (21 - 25 Abr): Dashboard com dados reais + a11y audit
MÊS 2 (Maio): OpenAI + Mobile + hardening
```
