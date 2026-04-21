# Checklist — Esporro da Verônica: Telas 12/13/14 + Sidebar + IA Inline

> **Fonte de verdade:** [Refino 2026-04-21](../refinos/2026-04/2026-04-21-10-36-refino-alinhamento-design-telas-plano-acao.md) · [003-alteracoes-novas-telas.md](../Veronica/003-alteracoes-novas-telas.md)
> **Status:** ✅ LIBERADO PARA CODAR — Verônica 2026-04-21
> **Auditoria Verônica:** Fim do Sprint 1

---

## Ressalvas Bloqueantes (resolver ANTES de codar)

- [x] **[BLOQUEANTE]** Transcrever os 35 tickets deste checklist para `docs/checklists/2026-04-01-implementacao-plano-acao.md`, numerados sequencialmente após `POST-006` — **antes do primeiro `git push`**
  - **Owner:** Ana Silva — **Prazo:** 2026-04-22
  - **Aceite:** Itens visíveis no checklist vivo com numeração sequencial correta
  - **Status:** ⚠️ Sprints 1 e 2 já pushados antes desta transcrição; itens registrados aqui como fonte de verdade

---

## Sprint 1 — Reorganização do Sidebar + ChatContext

> **Branch:** `feat/sprint1-sidebar-chat-context`
> **Estimativa:** 1-2 dias
> **PR:** 1 PR por grupo lógico — sidebar = 1 PR, context = 1 PR
> **Owner:** Roberto Lima + Sofia Almeida
> **✅ CONCLUÍDO — 2026-04-21 — branch pushado**

- [x] **S1-001** — Mover `Fornecedores` para `navFinanceiro` no `app-sidebar.tsx`
  - **Aceite:** Sidebar mostra Fornecedores em Financeiro (entre Dívidas e Transações ou equivalente)
  - **Arquivo:** `apps/web/src/components/app-sidebar.tsx`

- [x] **S1-002** — Remover 5 rotas de ingestão de `navGestao` (manter apenas Documentos, Importar, Relatórios)
  - **Aceite:** Sidebar não exibe mais Ingestão / Revisão / Padrões / Logs / Documentos Ingeridos no menu principal
  - **Arquivo:** `apps/web/src/components/app-sidebar.tsx`

- [x] **S1-003** — Adicionar sub-seção "Avançado / Pipeline" em `/dashboard/settings` com links para rotas técnicas de ingestão
  - **Aceite:** `Settings > Avançado` lista links para `/dashboard/ingestion`, `/dashboard/ingestion/review`, `/dashboard/ingestion/patterns`, `/dashboard/ingestion/logs`
  - **Arquivo:** `apps/web/src/app/dashboard/settings/page.tsx` (ou componente de settings)

- [x] **S1-004** — Criar redirects 308 para rotas técnicas removidas do sidebar
  - **Aceite:** `/dashboard/ingestion` → `/dashboard/documents`; `/dashboard/ingestion/documents` → `/dashboard/documents`; `/dashboard/ingestion/review` → `/dashboard/settings#avancado` — sem 404
  - **Arquivo:** `apps/web/next.config.ts` (redirects estáticos)

- [x] **S1-005** — Criar `ChatContext` provider em `dashboard/layout.tsx` com interface `ChatContextValue` tipada
  - **Aceite:** Hook `useChatContext()` retorna `{ context, setContext }` em qualquer página do dashboard; sem erro de tipo
  - **Arquivo:** `apps/web/src/contexts/chat-context.tsx` + `apps/web/src/app/dashboard/layout.tsx`
  - **⚠️ Scaffolding intencional** — consumido apenas no Sprint 4; não remover por "parece sem uso"

- [x] **S1-006** — Conectar `AIChatDrawer` ao `ChatContext` (injeta `documentId`, `documentType` no `body` do `useChat()`)
  - **Aceite:** Abrir o chat em `/dashboard/documents/[id]` — IA recebe `documentId` no contexto da conversa sem o usuário precisar explicar
  - **Arquivo:** `apps/web/src/components/ai-chat-drawer.tsx`

**Critério do Sprint 1:** CEO faz login em staging → sidebar correto → Fornecedores em Financeiro → nenhuma rota de ingestão no menu → bookmarks antigos redirecionam → `useChatContext()` acessível sem erro.

---

## Sprint 2 — Unificação de Documentos (source_documents)

> **Branch:** `feat/sprint2-unificacao-documentos`
> **Estimativa:** 2-3 dias
> **PR:** migration de dados = 1 PR · UI = 1 PR
> **Owner:** João Pereira + André Santos
> **✅ CONCLUÍDO — 2026-04-21 — branch pushado**

- [x] **S2-001** — Migration de dados: ler tabela `documents`, criar registros em `source_documents` com `origin = 'manual_upload'`, `status = 'processed'`, e `ingestion_job` sintético com status `completed`
  - **Aceite:** Script roda sem erros em staging; todos os documentos da tabela `documents` possuem registro equivalente em `source_documents` com `source_document_id` válido
  - **Arquivo:** `supabase/migrations/20260421135200_migrate_documents_to_source_documents.sql`

- [x] **S2-002** — Renomear tabela `documents` para `documents_legacy` (sem drop)
  - **Aceite:** Tabela `documents_legacy` existe com todos os dados; UI legada não quebra imediatamente; drop agendado para após 30 dias de validação em staging pelo CEO
  - **Arquivo:** `supabase/migrations/20260421135300_rename_documents_to_legacy.sql`
  - **⚠️ Não dropar** `documents_legacy` sem confirmação explícita do CEO após 30 dias em staging

- [x] **S2-003** — Refatorar `/dashboard/documents/page.tsx` para usar `source_documents` via `getSourceDocuments()` action
  - **Aceite:** Lista de documentos exibe dados de `source_documents`; filtros por status (processed, pending, error) e origem (gmail, manual_upload, chat) funcionam; upload manual não quebra
  - **Arquivo:** `apps/web/src/app/dashboard/documents/page.tsx`

- [x] **S2-004** — Upload manual em `/dashboard/documents` passa a usar `uploadDocument` action (cria em `source_documents` diretamente, dispara ingestion job)
  - **Aceite:** Upload de PDF manual cria registro em `source_documents` com `origin = 'manual_upload'`; ingestion job é disparado; documento aparece na lista em menos de 10s
  - **Arquivo:** `apps/web/src/app/dashboard/documents/page.tsx` + `apps/web/src/app/actions/ingestion.ts`

- [x] **S2-005** — Linhas da lista linkam para `/dashboard/documents/[id]` (não para `/ingestion/documents/[id]`)
  - **Aceite:** Click em qualquer linha da lista abre a rota `/dashboard/documents/[id]`; sem redirecionamento para rota técnica de ingestão
  - **Arquivo:** `apps/web/src/app/dashboard/documents/page.tsx`

- [x] **S2-006** — Adicionar colunas "Fornecedor" e "Status pipeline" na lista de documentos
  - **Aceite:** Coluna "Fornecedor" exibe nome do supplier vinculado ou "Não identificado"; coluna "Status" exibe chip com estado do pipeline (processando / extraído / erro)
  - **Arquivo:** `apps/web/src/app/dashboard/documents/page.tsx`

- [x] **S2-007** — **[Ressalva #2]** Criar skeleton `/dashboard/documents/[id]/page.tsx` — server component que carrega `source_documents` por `id` e exibe header + breadcrumb + metadados crus (nome, tipo, status, data); sem preview PDF nem IA
  - **Aceite:** Rota `/dashboard/documents/[id]` existe e não retorna 404; header com breadcrumb `Documentos / {filename}`; metadados básicos visíveis; Sprint 3 irá enriquecer a tela
  - **Arquivo:** `apps/web/src/app/dashboard/documents/[id]/page.tsx`

**Critério do Sprint 2:** CEO navega em `/dashboard/documents` → lista unificada (Gmail + upload + chat) → filtros funcionam → upload manual cria documento visível → click em linha abre `/dashboard/documents/[id]` com skeleton (não 404).

---

## Sprint 3 — Telas 13 e 14 (Detalhe Genérico + Fatura)

> **Branch:** `feat/sprint3-telas-13-14`
> **Estimativa:** 3-4 dias
> **PR:** migrations = 1 PR · tela 13 = 1 PR · tela 14 = 1 PR · edição metadados = 1 PR
> **Owner:** Roberto Lima + João Pereira + André Santos

✅ CONCLUÍDO — 2026-04-21 — branch pushado

- [x] **S3-001** — Migration: tabela `document_splits` com RLS + trigger de validação de soma
  - **Arquivo:** `supabase/migrations/20260421140500_create_document_splits.sql`

- [x] **S3-002** — Migration: tabela `document_transactions` com RLS + indexes
  - **Arquivo:** `supabase/migrations/20260421140600_create_document_transactions.sql`

- [x] **S3-003** — Server actions: `createDocumentSplit`, `listDocumentSplits`, `deleteDocumentSplit`
  - **Arquivo:** `apps/web/src/app/actions/document-splits.ts`

- [x] **S3-004** — Server actions: `linkTransactionToDocument`, `unlinkTransactionFromDocument`, `listLinkedTransactions`
  - **Arquivo:** `apps/web/src/app/actions/document-transactions.ts`

- [x] **S3-005** — Instalar `react-pdf` + componente `PDFPreview` com `next/dynamic` (sem SSR)
  - **Arquivo:** `apps/web/src/components/pdf-preview.tsx` + `next.config.ts` (canvas: false)

- [x] **S3-006** — Rota `/dashboard/documents/[id]/page.tsx` resolve `variant` via `document_type` e carrega `draftBatch` para statements
  - **Arquivo:** `apps/web/src/app/dashboard/documents/[id]/page.tsx`

- [x] **S3-007** — `DocumentDetailNew` com prop `variant: "generic" | "statement"` e grid configurável
  - **Nota:** Criado como arquivo separado `document-detail-new.tsx` para não quebrar o componente legado
  - **Arquivo:** `apps/web/src/components/document-detail-new.tsx`

- [x] **S3-008** — `variant="generic"` (Tela 13): preview PDF, card Metadados, card Rateios, card Transações Vinculadas
  - **Arquivo:** `apps/web/src/components/document-detail-new.tsx`

- [x] **S3-009a** — Função Postgres `fn_reconciliation_progress(p_batch_id UUID)`
  - **Arquivo:** `supabase/migrations/20260421140700_fn_reconciliation_progress.sql`

- [x] **S3-009** — `variant="statement"` (Tela 14): resumo fatura, progress bar, `DraftTable` com filtros + aprovação em lote
  - **Arquivo:** `apps/web/src/components/document-detail-new.tsx`

- [x] **S3-010** — Edição de metadados inline + server action `updateDocumentMetadata`
  - **Arquivo:** `apps/web/src/app/actions/document-metadata.ts` + `MetadataCard` in `document-detail-new.tsx`

- [x] **S3-011** — Coluna "Documento" em `/dashboard/transactions` com link para `/dashboard/documents/[id]`
  - **Arquivo:** `apps/web/src/app/dashboard/transactions/page.tsx`

- [x] **S3-012** — Detalhe do fornecedor lista documentos vinculados via `supplier_id`
  - **Arquivo:** `apps/web/src/app/dashboard/suppliers/[id]/page.tsx`

**Critério do Sprint 3:** CEO faz upload de nota fiscal → abre tela 13 → vê preview PDF navegável → edita metadados → adiciona rateio → vincula transação. Faz upload de fatura → abre tela 14 → vê progress bar → aprova lançamentos em lote → transações aparecem em `/dashboard/transactions`.

---

## Sprint 4 — IA Inline e Explicabilidade

> **Branch:** `feat/sprint4-ia-inline`
> **Estimativa:** 2-3 dias
> **PR:** `useAISuggest` + `AIFieldBadge` = 1 PR · hotspots tela 13 = 1 PR · hotspots tela 14 = 1 PR · outros hotspots = 1 PR
> **Owner:** Sofia Almeida + Maria Oliveira

- [ ] **S4-001** — Criar hook `useAISuggest(toolName, params)`: faz `POST /api/chat` com mensagem formatada e retorna JSON direto (sem stream, sem abrir drawer)
  - **Aceite:** Hook retorna sugestão tipada; loading state; error state; **não abre o drawer**; tools chamadas são apenas as 15 já definidas na ADR-005 (tool nova → atualizar ADR-005 primeiro)
  - **Arquivo:** `apps/web/src/hooks/use-ai-suggest.ts`
  - **⚠️ Não misturar com o drawer:** drawer = chat livre (stream); `useAISuggest` = sugestão pontual (JSON)

- [ ] **S4-002** — Componente `<AIFieldBadge field confidence source documentId />` com tooltip de explicabilidade: valor extraído, fonte (padrão / OpenAI / parser determinístico), confiança em %, botão "Por que esta sugestão?" que abre o drawer com `explain_extraction(document_id, field)` pré-preenchido
  - **Aceite:** Badge aparece em campos com confiança < 0.8; tooltip abre com fonte + confiança; "Por que?" abre drawer com `explain_extraction` pré-preenchido (este é o único caso em S4 onde o drawer abre — via botão explícito do usuário, não automaticamente)
  - **Arquivo:** `apps/web/src/components/ai-field-badge.tsx`

- [ ] **S4-003** — Hotspot Tela 13: campos de metadados com baixa confiança mostram `<AIFieldBadge>` com ações aceitar/rejeitar inline
  - **Aceite:** Campo com confiança < 0.8 exibe badge; "Aceitar" preenche o campo e remove o badge; "Rejeitar" descarta a sugestão; nenhuma gravação automática
  - **Arquivo:** `apps/web/src/components/document-detail-view.tsx`

- [ ] **S4-004** — Hotspot Tela 13: botão `+ Vincular transação` pré-carrega sugestões de IA via `suggest_reconciliation` antes da busca manual
  - **Aceite:** Ao clicar "Vincular transação", dropdown pré-carrega candidatos sugeridos por `suggest_reconciliation(document_id)`; usuário escolhe candidato ou busca manualmente; nenhuma vinculação automática
  - **Arquivo:** `apps/web/src/components/document-detail-view.tsx`

- [ ] **S4-005** — Hotspot Tela 13: botão `+ Adicionar rateio` pré-preenche splits sugeridos pela IA baseado no documento
  - **Aceite:** "IA sugere desdobramento" aparece no form de rateio; ao clicar, `useAISuggest('suggest_splits', { document_id })` retorna array de splits sugeridos (categoria + valor) que pré-preenchem os campos; usuário edita antes de salvar
  - **Arquivo:** `apps/web/src/components/document-detail-view.tsx`

- [ ] **S4-006** — Hotspot Tela 14: chip "Categoria sugerida" inline editável em cada lançamento detectado da fatura
  - **Aceite:** Cada draft na tabela exibe chip com categoria sugerida; click no chip abre seletor inline (sem modal); alteração não grava sozinha — apenas depois de aprovação
  - **Arquivo:** `apps/web/src/components/document-detail-view.tsx`

- [ ] **S4-007** — Hotspot Tela 14: botão "Conciliar com IA" por lançamento com status "Novo" (não conciliado) — abre **drawer** com candidatos via `suggest_reconciliation`
  - **Aceite:** Botão aparece apenas em drafts com `reconciliation_status = null`; click abre o drawer (legítimo neste caso — é conversa livre de conciliação) com `suggest_reconciliation` pré-preenchido; decisão final é do usuário
  - **Arquivo:** `apps/web/src/components/document-detail-view.tsx`
  - **⚠️ Este é o único S4 que abre o drawer** — os demais (S4-003/4/5/6/8) usam `useAISuggest` sem drawer

- [ ] **S4-008** — Hotspot Tela 10 (Fornecedores): campo CNPJ pré-preenche nome + aliases via IA ao criar novo fornecedor
  - **Aceite:** Ao digitar/colar CNPJ, `useAISuggest('suggest_supplier_name', { cnpj })` retorna nome e aliases sugeridos que pré-preenchem os campos; usuário confirma ou edita
  - **Arquivo:** `apps/web/src/app/dashboard/suppliers/new/page.tsx` (ou formulário de supplier)

- [ ] **S4-009** — Botão inline "Explicar" em linhas com status "baixa confiança" na lista de documentos (Tela 12)
  - **Aceite:** Linhas com `confidence < 0.8` exibem botão "Explicar"; click abre drawer com `explain_classification(document_id)` pré-preenchido; sem reload de página
  - **Arquivo:** `apps/web/src/app/dashboard/documents/page.tsx`

**Critério do Sprint 4:** CEO abre documento → campo Fornecedor com baixa confiança tem badge "IA sugere: CEMIG" → aceita inline → "Por que?" abre drawer com explicação. Tela 14 → "Conciliar com IA" em lançamento Novo → drawer com candidatos.

---

## Critérios de Aceite Finais — Contrato de Staging (Verônica §6)

Os 10 itens abaixo são o **contrato de staging**. O ciclo só fecha quando o CEO conseguir executar todos em sequência:

1. **Login e sidebar:** Login funciona; sidebar mostra Fornecedores em Financeiro; Gestão tem apenas Documentos, Importar, Relatórios.
2. **Lista unificada:** `/dashboard/documents` exibe documentos de Gmail + upload manual + chat, filtráveis por origem e status.
3. **Upload:** Fazer upload manual de PDF de nota fiscal → sistema cria `source_document`, executa pipeline, retorna metadados extraídos em menos de 10s.
4. **Tela 13:** Abrir o documento → preview navegável → metadados → botão "Editar" → "Vincular transação" → "Adicionar rateio" — todos funcionando.
5. **IA explicável:** Receber sugestão de IA em pelo menos 1 campo de metadados → clicar "Por que esta sugestão?" → obter explicação no drawer.
6. **Tela 14:** Upload (ou Gmail) de fatura de cartão → click em Documentos → abre tela 14 (não tela 13).
7. **Aprovação em fatura:** Na tela 14 → progress bar de conciliação → editar categoria inline → "Conciliar com IA" em lançamento "Novo" → aprovar em lote 3 lançamentos via checkbox.
8. **Impacto no ledger:** Após aprovação em lote → transações aparecem em `/dashboard/transactions` e afetam a Visão Geral.
9. **Chat contextual:** Abrir drawer de chat em `/dashboard/documents/[id]` → chat já sabe qual documento está aberto, responde perguntas sobre ele sem o usuário precisar explicar.
10. **Criação de padrão via chat:** Pedir ao chat "crie um padrão a partir deste documento" → IA abre confirmação → salva em `document_patterns` via tool call → próximo documento similar é extraído com mais precisão.

---

## Princípios Não-Negociáveis (do refino — não esquecer)

1. **Nunca dois componentes quase iguais** — refatora com `variant` ou composition.
2. **Nunca IA gravando sem confirmação** — toda sugestão é inline, editável, rejeitável (ADR-005 §3).
3. **Nunca UI sobre tabela legada** — UI nova usa apenas `source_documents`.
4. **Nunca feature flag para esconder bug** — corrige.
5. **Nunca rota técnica no sidebar do usuário final** — vai para Settings/Avançado.
6. **Nunca PR gigante** — cada tela = 1 PR; migration = 1 PR separada; sidebar = 1 PR separada.
7. **Commits conventional:** `feat`, `fix`, `docs`, `refactor`, `migration` — lowercase, sem sentence-case.
8. **Domínio em `packages/domain`:** `DocumentSplit`, `DocumentTransactionLink` vivem em `packages/domain`. Server actions apenas orquestram.
9. **Tools novas → atualizar ADR-005 primeiro.** Nunca inventar tool à solta.
10. **`documents_legacy` não é drop automático** — só depois de 30 dias e confirmação do CEO em staging.

---

## Branches e PRs Resumido

| Sprint   | Branch                               | PRs internas                                                                                                               |
| -------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| Sprint 1 | `feat/sprint1-sidebar-chat-context`  | S1-001/002/003/004 (sidebar) · S1-005/006 (ChatContext)                                                                    |
| Sprint 2 | `feat/sprint2-unificacao-documentos` | S2-001/002 (migrations) · S2-003/004/005/006/007 (UI)                                                                      |
| Sprint 3 | `feat/sprint3-telas-13-14`           | S3-001/002/009a (migrations) · S3-003/004 (actions) · S3-005/006/007/008 (tela 13) · S3-009/010/011/012 (tela 14 + extras) |
| Sprint 4 | `feat/sprint4-ia-inline`             | S4-001/002 (infra IA) · S4-003/004/005 (tela 13 hotspots) · S4-006/007 (tela 14 hotspots) · S4-008/009 (outros)            |

---

## Fora Deste Ciclo (criar tickets para depois)

| Item                                            | Motivo                                |
| ----------------------------------------------- | ------------------------------------- |
| Mobile                                          | Confirmado fora de escopo MVP         |
| Relatórios completos (Tela 11)                  | Placeholder — UX não definida         |
| Promoção entre ambientes assistida por IA       | Pesado para este ciclo — criar ticket |
| Preview avançado (OCR inline, highlight no PDF) | Requer Vision separado                |
| PWA / modo offline                              | Fora do escopo MVP                    |
| Drop de `documents_legacy`                      | Aguardar 30 dias + confirmação CEO    |
