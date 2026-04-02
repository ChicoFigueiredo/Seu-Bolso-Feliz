# Checklist de Implementação — Ingestão + IA + Staging

> Referência: [refino 2026-03-31](../refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md)

---

## Marco 1 — Staging Operacional (Fase A)

- [ ] 001 — Deploy Vercel real: substituir placeholder no GitLab CI
- [ ] 002 — Configurar env vars no Vercel (SUPABASE_URL, ANON_KEY, SERVICE_KEY)
- [ ] 003 — **CEO:** Configurar Google OAuth no Supabase Staging
- [ ] 004 — Configurar domínio customizado no Vercel
- [ ] 005 — Verificar 19 migrations aplicadas em staging
- [ ] 006 — Testar login Google em staging
- [ ] 006a — Logging/observabilidade: tabela `system_logs` + Edge Function de append
- [ ] 006b — Página `/dashboard/logs` com filtros por nível/módulo/data
- [ ] 006c — Edge Function `trigger-ingestion` para disparar scan/reprocessamento remoto (staging independente de local)

**Critério Marco 1:** CEO faz login em staging, vê dashboard, e logs do sistema são visíveis em `/dashboard/logs`.

---

## Marco 2 — Ingestão Visível (Fase B — início)

- [x] 007 — Server actions ingestão: listDocuments, getDocument
- [x] 008 — Server actions ingestão: listDrafts, getDraft
- [x] 009 — Rota `/dashboard/ingestion` com contadores
- [x] 010 — Rota `/dashboard/ingestion/documents` com listagem + filtros
- [x] 011 — Componente `<StatusBadge />` para 24 estados
- [x] 012 — Componente `<ConfidenceIndicator />`
- [x] 013 — Empty/loading states para páginas de ingestão

**Critério Marco 2:** Documentos ingeridos aparecem listados com status e filtros.

---

## Marco 3 — Revisão Humana (Fase B — completa)

- [x] 014 — Rota `/dashboard/ingestion/documents/[id]` com split-view
- [x] 015 — Componente `<PDFViewer />` inline
- [x] 016 — Componente `<DraftReviewForm />` com edição de campos
- [x] 017 — Componente `<DraftApprovalActions />` com aprovar/rejeitar/reprocessar
- [x] 018 — Server action: approveDraft (gravar no ledger + update status)
- [x] 019 — Server action: rejectDraft (marcar rejeitado + motivo)
- [x] 020 — Server action: reprocessDocument (resetar para QUEUED)
- [ ] 021 — Batch approval: selecionar + aprovar múltiplos drafts
- [x] 022 — Indicadores visuais: erro, pendência, falta de senha, baixa confiança
- [x] 023a — Upload manual via UI: drag & drop na página de ingestão

**Critério Marco 3:** CEO abre documento, vê PDF + dados, edita, aprova, e registro aparece em transactions. Upload manual funcional.

---

## Marco 4 — IA Acoplada (Fase C)

- [ ] 023 — **CEO:** Obter API key OpenAI e colocar no `.env`
- [x] 024 — Migration: tabelas ai_chat_sessions e ai_chat_messages com RLS
- [x] 025 — API Route `/api/chat` com auth, rate limiting, streaming
- [x] 026 — System prompt do SBF (contexto financeiro, personalidade)
- [x] 027 — Function calling tools (12 tools implementadas)
- [x] 028 — Componente `<AIChatDrawer />` com Vercel AI SDK useChat
- [ ] 029 — Upload de arquivo no chat (drag & drop)
- [ ] 030 — Renderização de mensagens: markdown, tabelas, badges
- [ ] 031 — Parser OpenAI: fallback quando regex falha
- [ ] 032 — OpenAI Vision: análise de imagem de documento
- [ ] 033 — Auditoria: log toda interação com IA
- [ ] 034 — Config `OPENAI_API_KEY` no Vercel env vars

**Critério Marco 4:** CEO abre drawer de chat, arrasta PDF, recebe sugestão, aprova via chat.

---

## Marco 5 — Padrões + Reconciliação + Gmail (Fases D + E + F)

### Padrões Documentais (Fase D)

- [ ] 035 — Migration: tabelas document_patterns e pattern_feedback com RLS
- [ ] 036 — Server actions: CRUD patterns, listagem, feedback
- [ ] 037 — Rota `/dashboard/ingestion/patterns` — lista + detalhe + edição
- [ ] 038 — Integrar padrões no ingestion worker
- [ ] 039 — Auto-desativação de padrões ruins (feedback_count > 3, success < 50%)
- [ ] 040 — Tools de chat: list/register/update patterns
- [ ] 041 — Seed: 3-5 padrões iniciais (CEMIG, boleto, fatura cartão)

### Reconciliação (Fase E)

- [ ] 042 — Módulo reconciliation.ts: heurísticas determinísticas
- [ ] 043 — Migration: colunas reconciliation_status, reconciled_with em draft_records
- [ ] 044 — Integrar reconciliação no ingestion worker
- [ ] 045 — API Route: buscar candidatos de conciliação para draft_id
- [ ] 046 — Componente `<ReconciliationPanel />` no detalhe do documento
- [ ] 047 — Tool IA: suggest_reconciliation
- [ ] 048 — Testes: cenários match, não-match, duplicado, recorrência

### Gmail Avançado (Fase F)

- [ ] 049 — Gmail scan por query livre (--query)
- [ ] 050 — Gmail scan por período (--after, --before)
- [ ] 051 — Rate limiting Gmail API
- [ ] 052 — Progresso do scan na UI
- [ ] 053 — Local scanner: dedup por content_hash
- [ ] 055 — MCP tools: scan_gmail_label, scan_gmail_query, scan_gmail_period

**Critério Marco 5:** Padrões funcionam, reconciliação sugere matches, Gmail scan completo.

---

## Marco 6 — Promoção entre Ambientes (Fase G)

- [ ] 056 — Migration: tabela promotion_logs
- [ ] 057 — Script promote.ts com --from, --to, --scope, --dry-run
- [ ] 058 — Lógica de diff por hash entre ambientes
- [ ] 059 — Lógica de merge: insert/update/skip
- [ ] 060 — Auditoria em promotion_logs
- [ ] 061 — MCP tools: promote_to_staging, promote_to_production

**Critério Marco 6:** `promote.ts --dry-run` mostra preview correto, execução real funciona.

---

## Pós Marco 6

- [ ] 062 — Dashboard operacional "primeira tela"
- [ ] 064 — A11y audit nas páginas de ingestão
- [ ] 065 — Testes E2E com volume real (1000+ docs)
- [ ] 066 — Gráficos e visualizações em relatórios
- [ ] 067 — Merge visual de fornecedores
- [ ] 068 — Cronograma visual de amortização
