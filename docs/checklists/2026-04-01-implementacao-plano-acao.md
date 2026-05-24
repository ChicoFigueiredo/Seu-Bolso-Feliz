# Checklist de Implementação — Plano de Ação Completo

> **Fonte de verdade:** Este checklist + refino principal + ADR-005 + ADR-006 (versões mais recentes).
> **Versões antigas no histórico Git possuem modelagem divergente (supplier_match, content_match, document_id, job_id). Ignorar.**
> **Referência:** [refino 2026-03-31](../refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md)

---

## Marco 1 — Staging Operacional + Observabilidade

**Objetivo:** CEO faz login em staging, vê dashboard, e logs do sistema são visíveis.

### Infra e Deploy

- [ ] M1-001 — Verificar 19 migrations aplicadas em staging (supabase db push --linked)
  - **Aceite:** `supabase db push` sem erros
- [ ] M1-002 — Configurar env vars no Vercel (NEXT_PUBLIC_SUPABASE_URL, NEXT_PUBLIC_SUPABASE_ANON_KEY)
  - **Aceite:** Variáveis visíveis no painel Vercel
- [ ] M1-003 — Substituir placeholder de deploy no CI/CD por deploy Vercel real
  - **Aceite:** Push na branch principal gera deploy em staging
- [ ] M1-004 — **CEO:** Configurar Google OAuth no Supabase Staging (redirect URL do Vercel)
  - **Aceite:** Login Google funciona em staging
- [ ] M1-005 — Testar login Google em staging
  - **Aceite:** CEO faz login e vê dashboard

### Logging e Observabilidade

- [ ] M1-006 — Criar migration: tabela `system_logs` (level, module, message, details, user_id, created_at)
  - **Aceite:** Migration aplicada sem erros, RLS ativa
- [ ] M1-007 — Criar server action `insertLog` e `getLogs` (com filtros por nível/módulo/data)
  - **Aceite:** Logs podem ser inseridos e consultados
- [ ] M1-008 — Criar rota `/dashboard/logs` com listagem, filtros por nível/módulo/período
  - **Aceite:** Página funcional com logs visíveis
- [ ] M1-009 — Adicionar link "Logs" no sidebar (seção Gestão)
  - **Aceite:** Link visível e navegável

### Edge Function — Staging Independence

- [x] M1-010 — Criar Edge Function `trigger-ingestion` para disparar reprocessamento remoto
  - **Aceite:** Chamada HTTP dispara reprocessamento de jobs QUEUED em staging

**Critério do Marco:** CEO faz login em staging, vê dashboard, navega para /dashboard/logs.

---

## Marco 2 — Ingestão Visível

**Objetivo:** Documentos ingeridos aparecem listados com status e filtros.

### Server Actions — Ingestão

- [x] M2-001 — Criar server action `listDocuments` (source_documents com joins em ingestion_jobs)
  - **Aceite:** `getSourceDocuments()` em `actions/ingestion.ts`
- [x] M2-002 — Criar server action `getDocument` (detalhe completo do documento + jobs + drafts)
  - **Aceite:** `getDocumentWithRelations()` em `actions/ingestion.ts`
- [x] M2-003 — Criar server action `listDrafts` (draft_records com filtros)
  - **Aceite:** `getDraftRecords()` em `actions/ingestion.ts`
- [x] M2-004 — Criar server action `getDraft` (detalhe do draft + extraction_result)
  - **Aceite:** Draft data via `getDraftRecords()` com filtro por batch

### Páginas de Ingestão

- [x] M2-005 — Criar rota `/dashboard/ingestion` com contadores (total, pendentes, aprovados, erros)
  - **Aceite:** Página com cards de contadores funcionais
- [x] M2-006 — Criar rota `/dashboard/ingestion/documents` com listagem + filtros (status, origem, tipo, período)
  - **Aceite:** Listagem paginada com filtros funcionando
- [x] M2-007 — Componente `<StatusBadge />` para estados da máquina de estados do ingestion
  - **Aceite:** Badge com cor e label por estado
- [x] M2-008 — Componente `<ConfidenceIndicator />` para confiança de extração
  - **Aceite:** Indicador visual com cores
- [x] M2-009 — Empty states e loading states para páginas de ingestão
  - **Aceite:** Implementado nas páginas de ingestão

### Sidebar

- [x] M2-010 — Adicionar link "Ingestão" no sidebar (seção Gestão) com ícone
  - **Aceite:** Links para Ingestão, Documentos e Logs no sidebar

**Critério do Marco:** Documentos ingeridos via worker CLI aparecem listados com status e filtros na UI.

---

## Marco 3 — Revisão Humana + Upload Manual

**Objetivo:** CEO abre documento, vê PDF + dados, edita, aprova, e registro aparece em transactions. Upload manual funcional.

### Upload Manual

- [x] M3-001 — Criar server action `uploadDocument` (recebe arquivo, salva no Storage, cria source_document + ingestion_job)
  - **Aceite:** Upload via `UploadDocuments` component + Edge Function trigger
- [x] M3-002 — Componente `<DocumentUpload />` com drag & drop + seleção de arquivo
  - **Aceite:** `UploadDocuments` com drag & drop funcional
- [x] M3-003 — Integrar upload no `/dashboard/ingestion` (botão "Enviar documento" + modal/área de upload)
  - **Aceite:** Integrado na página de ingestão

### Revisão — Split View

- [x] M3-004 — Criar rota `/dashboard/ingestion/documents/[id]` com layout split-view
  - **Aceite:** `DocumentDetailView` com split-view funcional
- [x] M3-005 — Componente `<PDFViewer />` inline (embed ou iframe para PDF do Supabase Storage)
  - **Aceite:** Inline via iframe/img no `DocumentDetailView`
- [x] M3-006 — Componente `<DraftReviewForm />` com campos editáveis (fornecedor, tipo, valor, data, categoria)
  - **Aceite:** `DraftReviewForm` com edição de campos
- [x] M3-007 — Componente `<DraftApprovalActions />` com botões aprovar/rejeitar/reprocessar
  - **Aceite:** Ações integradas no `DraftReviewForm`

### Server Actions — Revisão

- [x] M3-008 — Criar server action `approveDraft` (valida draft, grava no ledger, atualiza status)
  - **Aceite:** `approveDraftRecord()` em `actions/ingestion.ts`
- [x] M3-009 — Criar server action `rejectDraft` (marca rejeitado + motivo obrigatório)
  - **Aceite:** `rejectDraftRecord()` com `corrections.rejection_reason`
- [x] M3-010 — Criar server action `reprocessDocument` (reseta job para QUEUED, limpa drafts anteriores)
  - **Aceite:** `reprocessDocument()` em `actions/ingestion.ts`
- [x] M3-011 — Batch approval: server action `approveBatch` (aprovar múltiplos drafts de uma vez)
  - **Aceite:** `approveDraftBatch()` em `actions/ingestion.ts`

### Indicadores Visuais

- [x] M3-012 — Indicadores no detalhe: erro (vermelho), pendência (amarelo), falta de senha (ícone cadeado), baixa confiança (alerta)
  - **Aceite:** Indicadores visuais no `DocumentDetailView` e `StatusBadge`
- [x] M3-013 — Exclusão operacional de documentos com dois modos (`só documento` e `documento + ingestão`)
  - **Aceite:** diálogo explica o impacto de cada opção; `só documento` preserva trilha de ingestão e oculta o item da lista; `documento + ingestão` limpa jobs/logs/drafts/parse/fingerprint sem quebrar FKs

**Critério do Marco:** CEO faz upload de PDF, vê no split-view, edita campos, aprova, e transação aparece no dashboard.

---

## Marco 4 — IA Acoplada (prep arquitetural + início)

**Objetivo:** Chat funcional, upload pelo chat, sugestão de fornecedor/tipo/campos.

### Infra IA

- [ ] M4-001 — **CEO:** Obter API key OpenAI e configurar em `.env.local` e Vercel env vars
  - **Aceite:** OPENAI_API_KEY acessível pelo backend
- [x] M4-002 — Instalar dependência `ai` (Vercel AI SDK) e `openai` no apps/web
  - **Aceite:** `ai@4.3.19` + `@ai-sdk/openai@1.3.24` instalados
- [x] M4-003 — Criar migration: tabelas `ai_chat_sessions` e `ai_chat_messages` com RLS
  - **Aceite:** Migration `20260401100000_create_ai_chat_tables.sql` com RLS

### API de Chat

- [x] M4-004 — Criar API Route `app/api/chat/route.ts` com auth, rate limiting, streaming
  - **Aceite:** Rate limiting 10/min + 100/dia, streaming via `streamText()`
- [x] M4-005 — System prompt do SBF (contexto financeiro, personalidade, instruções de tool use)
  - **Aceite:** `system-prompt.ts` com persona e guardrails
- [x] M4-006 — Function calling: tools iniciais (list_documents, get_document, list_drafts, approve_draft, reject_draft)
  - **Aceite:** 12 tools em `lib/ai/tools.ts`

### UI de Chat

- [x] M4-007 — Componente `<AIChatDrawer />` com Vercel AI SDK useChat hook
  - **Aceite:** `AIChatDrawer` com `useChat()` e streaming
- [x] M4-008 — Upload de arquivo no chat (drag & drop que chama uploadDocument + informa à IA)
  - **Aceite:** Paperclip + file picker no `AIChatDrawer`, upload → trigger-ingestion → mensagem automática
- [x] M4-009 — Renderização de mensagens: markdown, tabelas, badges de status
  - **Aceite:** `ChatMarkdown` com GFM tables, code blocks, listas, links, headings, blockquotes
- [x] M4-010 — Botão de chat no header/sidebar + toggle de drawer
  - **Aceite:** `ChatToggle` flutuante no dashboard layout

### Function Calling Avançado

- [x] M4-011 — Tools: suggest_supplier, suggest_document_type, explain_classification
  - **Aceite:** `suggestDocumentType` + `explainClassification` em `tools.ts` (suggest_supplier já existia)
- [x] M4-012 — Tools: list_pending_documents, list_error_documents, list_missing_password_documents
  - **Aceite:** `listErrorDocuments` + `listMissingPasswordDocuments` no registry (list_pending já existia)
- [x] M4-013 — Tools: batch_approve_drafts (com confirmação humana)
  - **Aceite:** `batchApproveDrafts` tool registrada com limit de 20 drafts, confirmação via system prompt
- [x] M4-014 — Auditoria: registrar toda interação com IA em ai_chat_messages
  - **Aceite:** Mensagens user/assistant + tool calls (nome, args, resultado) persistidos em `ai_chat_messages`

**Critério do Marco:** CEO abre drawer de chat, arrasta PDF, recebe sugestão de fornecedor/tipo, aprova via chat.

---

## Marco 5 — Padrões Documentais + Reconciliação + Gmail

### Padrões (Fase D)

- [ ] M5-001 — Migration: tabelas `document_patterns` e `pattern_feedback` conforme ADR-006
  - **Aceite:** Tabelas criadas com exata modelagem do ADR-006 (supplier_id FK, field_mappings, version, etc.)
- [ ] M5-002 — Server actions: CRUD patterns (create, list, get, update, deactivate)
  - **Aceite:** Padrões editáveis e listáveis
- [ ] M5-003 — Rota `/dashboard/ingestion/patterns` com lista de padrões + detalhe + histórico
  - **Aceite:** Página funcional com success_count, feedback_count, is_active
- [ ] M5-004 — Integrar busca de padrões no ingestion worker (match por supplier_id + document_type)
  - **Aceite:** Worker usa padrão quando encontra match
- [ ] M5-005 — Auto-desativação: padrão com feedback_count > 3 e taxa < 50% → is_active = false
  - **Aceite:** Teste automatizado valida regra
- [ ] M5-006 — Tools de chat: list_document_patterns, register_document_pattern, update_pattern
  - **Aceite:** IA gerencia padrões via chat
- [ ] M5-007 — Seed: 3-5 padrões iniciais (CEMIG, boleto, fatura cartão)
  - **Aceite:** Padrões pré-carregados no seed.sql

### Reconciliação (Fase E)

- [ ] M5-008 — Módulo reconciliation.ts no worker: heurísticas por valor+data+fornecedor
  - **Aceite:** Match determinístico funcional com score de confiança
- [ ] M5-009 — Migration: colunas reconciliation_status, reconciled_with em draft_records
  - **Aceite:** Migration aplicada
- [ ] M5-010 — Integrar reconciliação no ingestion worker (pós extração, pré review)
  - **Aceite:** Drafts chegam com sugestão de reconciliação
- [ ] M5-011 — Componente `<ReconciliationPanel />` no detalhe do documento
  - **Aceite:** Mostra candidatos de match com botão "vincular"
- [ ] M5-012 — Tool IA: suggest_reconciliation
  - **Aceite:** IA sugere match quando perguntada

### Gmail Avançado (Fase F)

- [ ] M5-013 — Gmail scan por query livre (--query)
  - **Aceite:** Worker aceita flag --query
- [ ] M5-014 — Gmail scan por período (--after, --before)
  - **Aceite:** Worker filtra por data
- [ ] M5-015 — Rate limiting Gmail API (respeitar quotas)
  - **Aceite:** Backoff exponencial implementado
- [ ] M5-016 — Progresso do scan na UI (jobs em andamento, contagem)
  - **Aceite:** Página de ingestão mostra scan em progress
- [ ] M5-017 — MCP tools: scan_gmail_label, scan_gmail_query, scan_gmail_period
  - **Aceite:** Tools funcionais no MCP server

**Critério do Marco:** Padrões funcionam, reconciliação sugere matches, Gmail scan completo com progresso na UI.

---

## Marco 6 — Promoção entre Ambientes

- [ ] M6-001 — Migration: tabela `promotion_logs` com RLS
  - **Aceite:** Tabela criada
- [ ] M6-002 — Script `promote.ts` com --from, --to, --scope, --dry-run
  - **Aceite:** Dry-run mostra preview correto
- [ ] M6-003 — Lógica de diff por hash entre ambientes
  - **Aceite:** Detecta itens novos/alterados/iguais
- [ ] M6-004 — Lógica de merge: insert/update/skip com idempotência
  - **Aceite:** Re-execução não duplica dados
- [ ] M6-005 — Auditoria em promotion_logs (quem, quando, o que, resultado)
  - **Aceite:** Cada promoção fica registrada
- [ ] M6-006 — MCP tools: promote_to_staging, promote_to_production
  - **Aceite:** Tools funcionais no MCP server

**Critério do Marco:** `promote.ts --dry-run` mostra preview, execução real funciona, auditável.

---

## Pós Marcos

- [ ] POST-001 — Dashboard operacional "primeira tela" (priorização, vencimentos, essenciais)
- [ ] POST-002 — A11y audit nas páginas de ingestão (contraste, foco, teclado)
- [ ] POST-003 — Testes E2E com volume real (1000+ docs)
- [ ] POST-004 — Gráficos e visualizações em relatórios
- [ ] POST-005 — Merge visual de fornecedores
- [ ] POST-006 — Cronograma visual de amortização

---

## Marco Local — Homologação Técnica (Documentos + IA + Interpretação)

> Adicionado em 2026-05-02 para reduzir risco antes de staging. Foco direto no que Verônica e CEO pediram para as telas 12/13/14 e IA inline.

- [ ] ML-001 — Subir ambiente local completo (web + supabase + worker ingestion)
  - **Aceite:** app acessível e sem erros críticos de build
- [ ] ML-002 — Validar lista unificada em `/dashboard/documents` com upload manual
  - **Aceite:** documento novo aparece com status de pipeline
- [ ] ML-003 — Validar tela 13 (`variant="generic"`) com preview PDF e edição de metadados
  - **Aceite:** preview navega páginas e salva metadados
- [ ] ML-004 — Validar tela 14 (`variant="statement"`) com progress bar server-side
  - **Aceite:** progress bar usa `fn_reconciliation_progress` e tabela de drafts renderiza
- [ ] ML-005 — Validar IA de explicabilidade (`AIFieldBadge` + botão "Por que?")
  - **Aceite:** drawer abre com contexto e resposta de explicação
- [ ] ML-006 — Validar IA de reconciliação/rateio/documentos
  - **Aceite:** `suggest_reconciliation`, `suggest_splits`, `explain_classification` retornam resultados na UI
- [ ] ML-007 — Validar sugestão de fornecedor por CNPJ
  - **Aceite:** formulário de fornecedor pré-preenche nome quando houver sugestão
- [ ] ML-008 — Registrar evidências para staging
  - **Aceite:** lista de IDs testados + prints dos fluxos 12/13/14 + IA anexada ao MR
