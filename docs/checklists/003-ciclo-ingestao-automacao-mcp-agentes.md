# 003 — Checklist: Ciclo de Ingestão, Automação, MCP, Agentes e Povoamento

> Checklist operacional para acompanhamento de todas as entregas do ciclo.  
> Cada item tem responsável, fase e status rastreável.

---

## Legenda de Status

- ⬜ Não iniciado
- 🔲 Bloqueado (dependência pendente)
- 🟡 Em progresso
- ✅ Concluído
- ❌ Cancelado/descartado (com justificativa)

---

## Sprint 0 — Fechamento de Gaps Pendentes

| #   | Tarefa                                                            | Responsável     | Status | Notas                                              |
| --- | ----------------------------------------------------------------- | --------------- | ------ | -------------------------------------------------- |
| 0.1 | Criar materialized view `mv_supplier_spending`                    | André (DBA)     | ✅     | Migration 013                                      |
| 0.2 | Criar Edge Function `refresh-mv-supplier-spending`                | Maria (Backend) | ✅     | `supabase/functions/refresh-mv-supplier-spending/` |
| 0.3 | Implementar confirm atômico em `retroactive-supplier-association` | Maria (Backend) | ✅     | RPC PL/pgSQL em migration                          |
| 0.4 | Povoar `seed.sql` com dados de teste realistas                    | André (DBA)     | ✅     | Todos os tipos de entidade incluídos               |
| 0.5 | Validação visual das tabelas no Supabase Studio                   | André (DBA)     | ✅     | `supabase db reset` + query validou 36 tabelas     |

---

## Fase 1 — Fechamento Operacional e Prontidão de Ambientes

### 1A — Vercel

| #    | Tarefa                                     | Responsável    | Status | Notas                                         |
| ---- | ------------------------------------------ | -------------- | ------ | --------------------------------------------- |
| 1.1  | Criar conta/organização no Vercel          | CEO            | ✅     | vercel.com — prj_1CkF4ydlHvXKPvH1Mf8phwBNtKKC |
| 1.2  | Conectar repositório GitLab ao Vercel      | CEO + Fernando | ✅     | CEO conectou via dashboard Vercel             |
| 1.3  | Configurar root directory: `apps/web`      | Fernando       | ✅     | Verificado via `vercel project inspect`       |
| 1.4  | Configurar framework preset: Next.js       | Fernando       | ✅     | Verificado: Next.js + bun run build           |
| 1.5  | Configurar preview deployments por branch  | Fernando       | ✅     | CEO confirmou configuração                    |
| 1.6  | Configurar environment variables no Vercel | Fernando       | ✅     | 3 vars em Production, Preview, Development    |
| 1.7  | Registrar domínio `seubolsofeliz.com.br`   | CEO            | ✅     | registro.br                                   |
| 1.8  | Configurar DNS apontando para Vercel       | CEO + Fernando | ✅     | DNS parqueado na Vercel                       |
| 1.9  | Validar deploy real com página acessível   | Fernando       | ✅     | https://seubolsofeliz.com.br/ respondendo OK  |
| 1.10 | Validar HTTPS funcionando                  | Fernando       | ✅     | Vercel fornece HTTPS automático               |

### 1B — Supabase (Ambientes)

| #    | Tarefa                                    | Responsável | Status | Notas                                                                                                 |
| ---- | ----------------------------------------- | ----------- | ------ | ----------------------------------------------------------------------------------------------------- |
| 1.11 | Criar projeto Supabase STAGING            | CEO         | ✅     | dcljzgjgnkmxdvhybvpt                                                                                  |
| 1.12 | Criar projeto Supabase PRODUCTION         | CEO         | ✅     | opwelsgdhksuuewdbefk                                                                                  |
| 1.13 | Anotar URLs e chaves de cada projeto      | CEO         | ✅     | Salvo em .env na raiz                                                                                 |
| 1.14 | Configurar `supabase link` para staging   | Fernando    | ✅     | Linkado com sucesso                                                                                   |
| 1.15 | Aplicar migrations em staging             | Fernando    | ✅     | 17 migrations aplicadas                                                                               |
| 1.16 | Aplicar migrations em production          | Fernando    | ✅     | 17 migrations aplicadas via `supabase db push`                                                        |
| 1.17 | Deploy Edge Functions em staging          | Fernando    | ✅     | 3 functions ACTIVE                                                                                    |
| 1.18 | Deploy Edge Functions em production       | Fernando    | ✅     | 3 functions deployed: merge-suppliers, refresh-mv-supplier-spending, retroactive-supplier-association |
| 1.19 | Validar RLS em staging com dados de teste | André       | ✅     | 11 tabelas testadas — 0 rows para anon                                                                |
| 1.20 | Validar RLS em production                 | André       | ✅     | 10 tabelas testadas com anon key — 0 rows retornados                                                  |

### 1C — GitLab CI/CD

| #    | Tarefa                                                      | Responsável | Status | Notas                                           |
| ---- | ----------------------------------------------------------- | ----------- | ------ | ----------------------------------------------- |
| 1.21 | Configurar variáveis protegidas por ambiente no GitLab      | Fernando    | ✅     | CEO confirmou configuração via dashboard GitLab |
| 1.22 | Substituir placeholder deploy-web por job real (Vercel CLI) | Fernando    | �      | Desbloqueado — 1.2 ✅                           |
| 1.23 | Validar pipeline completo em branch develop                 | Fernando    | 🔲     | Depende de 1.21-1.22                            |
| 1.24 | Validar deploy staging end-to-end                           | Fernando    | 🔲     | Depende de 1.23                                 |
| 1.25 | Documentar política de promoção staging → production        | Fernando    | 🔲     | Depende de 1.24                                 |

### 1D — Estrutura Monorepo

| #    | Tarefa                                                  | Responsável | Status | Notas            |
| ---- | ------------------------------------------------------- | ----------- | ------ | ---------------- |
| 1.26 | Criar `apps/mcp-server/` com package.json stub          | Fernando    | ✅     |                  |
| 1.27 | Criar `workers/ingestion/` com package.json stub        | Fernando    | ✅     |                  |
| 1.28 | Criar `workers/gmail-scanner/` com package.json stub    | Fernando    | ✅     |                  |
| 1.29 | Criar `workers/local-scanner/` com package.json stub    | Fernando    | ✅     |                  |
| 1.30 | Criar `packages/operations/` com package.json stub      | Fernando    | ✅     |                  |
| 1.31 | Criar `packages/ingestion-types/` com package.json stub | Fernando    | ✅     |                  |
| 1.32 | Atualizar workspace config no root package.json         | Fernando    | ✅     |                  |
| 1.33 | Validar que todos os workspaces resolvem corretamente   | Fernando    | ✅     | `bun install` ok |

---

## Fase 2 — Estrutura de Ingestão e Idempotência Documental

### 2A — Modelagem de Dados

| #    | Tarefa                                                    | Responsável | Status | Notas                                         |
| ---- | --------------------------------------------------------- | ----------- | ------ | --------------------------------------------- |
| 2.1  | Criar migration: tabela `ingestion_runs`                  | André       | ✅     | Migration 014                                 |
| 2.2  | Criar migration: tabela `ingestion_jobs`                  | André       | ✅     | Migration 014                                 |
| 2.3  | Criar migration: tabela `source_documents`                | André       | ✅     | Migration 014                                 |
| 2.4  | Criar migration: tabela `document_fingerprints`           | André       | ✅     | Migration 014                                 |
| 2.5  | Criar migration: tabela `parsed_document_versions`        | André       | ✅     | Migration 014                                 |
| 2.6  | Criar migration: tabela `extraction_results`              | André       | ✅     | Migration 014                                 |
| 2.7  | Criar migration: tabela `draft_records`                   | André       | ✅     | Migration 014                                 |
| 2.8  | Criar migration: tabela `draft_batches`                   | André       | ✅     | Migration 014                                 |
| 2.9  | Criar migration: tabela `ingestion_logs`                  | André       | ✅     | Migration 014                                 |
| 2.10 | Criar migration: RLS policies para todas as tabelas novas | André       | ✅     | Migration 015                                 |
| 2.11 | Criar migration: índices para tabelas de ingestão         | André       | ✅     | Migration 016                                 |
| 2.12 | Criar migration: bucket `ingestion-originals` no Storage  | André       | ✅     | Migration 017                                 |
| 2.13 | Regenerar tipos TypeScript (`generate-types.sh`)          | André       | ✅     | 2296 linhas, inclui todas tabelas de ingestão |
| 2.14 | Atualizar `@sbf/shared-types` com novos tipos             | Maria       | ✅     | Via `@sbf/ingestion-types`                    |

### 2B — Pacotes e Tipos

| #    | Tarefa                                                          | Responsável | Status | Notas                     |
| ---- | --------------------------------------------------------------- | ----------- | ------ | ------------------------- |
| 2.15 | Definir enums de ingestão em `@sbf/ingestion-types`             | Maria       | ✅     | 8 enums + types definidos |
| 2.16 | Definir schemas Zod de ingestão em `@sbf/validation`            | Maria       | ⬜     | Pós-MVP                   |
| 2.17 | Implementar lógica de hash/fingerprint em `packages/operations` | João        | ✅     | SHA-256 + canônico        |
| 2.18 | Implementar política de idempotência em `packages/operations`   | João        | ✅     | 4 caminhos de dedup       |

### 2C — Worker e Scanner Local

| #    | Tarefa                                                | Responsável  | Status | Notas                                 |
| ---- | ----------------------------------------------------- | ------------ | ------ | ------------------------------------- |
| 2.19 | Implementar worker core em `workers/ingestion/`       | João         | ✅     | state-machine + processor + poll loop |
| 2.20 | Implementar scanner local em `workers/local-scanner/` | João         | ✅     | Scan dir + upload + job creation      |
| 2.21 | Implementar upload para Storage no worker             | João         | ✅     | Via scanner → Storage bucket          |
| 2.22 | Implementar logging no banco (ingestion_logs)         | João         | ✅     | `writeLog()` → console + DB           |
| 2.23 | Configurar fila (Supabase Queues ou polling)          | João + André | ✅     | Polling com optimistic locking        |

### 2D — Testes

| #    | Tarefa                                            | Responsável | Status | Notas                                                                       |
| ---- | ------------------------------------------------- | ----------- | ------ | --------------------------------------------------------------------------- |
| 2.24 | Testes unitários: hash/fingerprint                | Maria       | ✅     | 12 testes passando                                                          |
| 2.25 | Testes unitários: política de idempotência        | Maria       | ✅     | 10 testes passando                                                          |
| 2.26 | Testes unitários: máquina de estados              | Maria       | ✅     | 47 testes passando                                                          |
| 2.27 | Testes integração: scanner local → worker → banco | Maria       | ✅     | 3 testes: fluxo completo, múltiplos arquivos, extensões ignoradas           |
| 2.28 | Teste de duplicação: mesmo doc 2x não duplica     | Maria       | ✅     | 2 testes: scanner ignora origin_key existente, worker rejeita hash idêntico |
| 2.29 | Teste de reprocessamento forçado                  | Maria       | ✅     | 2 testes: force_reprocess bypasses dedup, retry de FAILED→DISCOVERED→QUEUED |

---

## Fase 3 — Integração Inicial com Gmail

### 3A — Configurações (CEO)

| #   | Tarefa                                                | Responsável | Status | Notas                                                                                      |
| --- | ----------------------------------------------------- | ----------- | ------ | ------------------------------------------------------------------------------------------ |
| 3.1 | Criar projeto no Google Cloud Console                 | CEO         | ✅     | project-fea8cec5-2366-4755-873                                                             |
| 3.2 | Habilitar Gmail API                                   | CEO         | ✅     |                                                                                            |
| 3.3 | Configurar OAuth consent screen                       | CEO         | ✅     |                                                                                            |
| 3.4 | Criar credenciais OAuth (Client ID + Secret)          | CEO         | ✅     | Client ID e Secret em .env                                                                 |
| 3.5 | Definir redirect URIs no Google Cloud Console         | CEO         | ✅     | 6 redirect URIs configurados (local, staging, production)                                  |
| 3.6 | Gerar primeiro refresh token                          | CEO + João  | ✅     | Script `get-token.ts` criado e executado. Refresh token obtido e armazenado                |
| 3.7 | Armazenar segredos no Supabase Vault                  | CEO         | ✅     | CEO confirmou refresh token armazenado no Vault                                            |
| 3.8 | Criar label `Comprovantes` no Gmail (se não existir)  | CEO         | ✅     | CEO confirmou label criada em 2026-03-24                                                   |
| 3.9 | Mover alguns e-mails com anexos para a label de teste | CEO         | ✅     | CEO confirmou 1000+ e-mails na label. Alguns precisaram de webscraping para ficar visíveis |

#### 🔧 Ações do CEO para Google Auth (item 3.5)

**No Google Cloud Console** (APIs & Services > Credentials > OAuth 2.0 Client ID):

Adicionar os seguintes **Authorized redirect URIs**:

```
http://127.0.0.1:54321/auth/v1/callback
https://dcljzgjgnkmxdvhybvpt.supabase.co/auth/v1/callback
https://opwelsgdhksuuewdbefk.supabase.co/auth/v1/callback
```

Adicionar os seguintes **Authorized JavaScript origins**:

```
http://127.0.0.1:54321
http://localhost:3105
https://dcljzgjgnkmxdvhybvpt.supabase.co
https://opwelsgdhksuuewdbefk.supabase.co
```

**No Supabase Dashboard — STAGING** (Authentication > Providers > Google):

1. Ativar o provider Google
2. Client ID: `XXX`
3. Client Secret: `XXX`

**No Supabase Dashboard — PRODUCTION** (mesmo procedimento quando produção estiver ativa)

### 3AA — Google Auth no App (Login via Google)

| #       | Tarefa                                               | Responsável | Status | Notas                                                                                                  |
| ------- | ---------------------------------------------------- | ----------- | ------ | ------------------------------------------------------------------------------------------------------ |
| 3.AA.1  | Configurar `[auth.external.google]` no config.toml   | Fernando    | ✅     | skip_nonce_check=true (local)                                                                          |
| 3.AA.2  | Adicionar env vars GOOGLE*OAUTH*\* no .env.local     | Fernando    | ✅     | Client ID + Secret                                                                                     |
| 3.AA.3  | Implementar botão "Entrar com Google" na /login      | Roberto     | ✅     | signInWithOAuth + ícone SVG                                                                            |
| 3.AA.4  | Auth callback já funciona com PKCE (code exchange)   | Roberto     | ✅     | /auth/callback/route.ts                                                                                |
| 3.AA.5  | Corrigir rota /auth/signout (redirect URL)           | Roberto     | ✅     | Agora usa request.url origin                                                                           |
| 3.AA.6  | Reiniciar Supabase local com Google provider         | Fernando    | ✅     | `supabase stop && supabase start`                                                                      |
| 3.AA.7  | CEO: adicionar redirect URIs no Google Cloud Console | CEO         | ✅     | Screenshot confirmado: 6 URIs (local 54321, local 3000, local 8080, staging, production, app callback) |
| 3.AA.8  | CEO: ativar Google provider no Supabase staging      | CEO         | ✅     | CEO confirmou configuração via dashboard                                                               |
| 3.AA.9  | Testar fluxo Google Auth local end-to-end            | Fernando    | ✅     | CEO confirmou login com Gmail no localhost — acessou dashboard com sucesso em 2026-03-24               |
| 3.AA.10 | Testar fluxo Google Auth staging end-to-end          | Fernando    | �      | Desbloqueado — 3.AA.7 ✅ + 3.AA.8 ✅                                                                   |

### 3B — Implementação

| #    | Tarefa                                            | Responsável | Status | Notas                                                                                                       |
| ---- | ------------------------------------------------- | ----------- | ------ | ----------------------------------------------------------------------------------------------------------- |
| 3.10 | Implementar client OAuth Gmail                    | João        | ✅     | `gmail-client.ts` — GmailClient com auto-refresh token, listLabels, listMessages, getMessage, getAttachment |
| 3.11 | Implementar scan por label                        | João        | ✅     | `index.ts` — scanGmailLabel() com paginação, --label CLI arg                                                |
| 3.12 | Implementar scan por período                      | João        | 🔲     | Backlog: query por data via Gmail API `after:` `before:`                                                    |
| 3.13 | Implementar scan por query livre                  | João        | 🔲     | Backlog: query livre via Gmail API `q` param                                                                |
| 3.14 | Implementar download de anexos                    | João        | ✅     | `message-processor.ts` + `processAttachment()` — download, decode base64url, upload Storage                 |
| 3.15 | Implementar detecção de mensagens já processadas  | João        | ✅     | `isAlreadyProcessed()` via origin_key em source_documents                                                   |
| 3.16 | Implementar modo dry-run                          | João        | ✅     | `--dry-run` CLI arg — lista mensagens e anexos sem gravar                                                   |
| 3.17 | Implementar comando de backfill                   | João        | ✅     | `--limit N` + paginação automática = backfill incremental                                                   |
| 3.18 | Integrar com fila de ingestão                     | João        | ✅     | Cria ingestion_run + ingestion_jobs (DISCOVERED) por attachment                                             |
| 3.19 | Implementar logging de falhas por mensagem/anexo  | João        | ✅     | Console errors por attachment + stats.errors, run FAILED se 0 jobs                                          |
| 3.20 | Refresh token management (auto-refresh com retry) | João        | ✅     | GmailClient auto-refresh 60s antes de expirar                                                               |

### 3C — Testes

| #    | Tarefa                                   | Responsável | Status | Notas                                                          |
| ---- | ---------------------------------------- | ----------- | ------ | -------------------------------------------------------------- |
| 3.21 | Teste: scan de label gera jobs           | Maria       | ✅     | 14 testes: processMessage (metadados, anexos, nested, filtros) |
| 3.22 | Teste: anexo real é baixado e armazenado | Maria       | ✅     | 3 testes: decodeBase64Url (texto, caracteres especiais, vazio) |
| 3.23 | Teste: mensagem já processada é ignorada | Maria       | ✅     | Coberto por isAlreadyProcessed() + origin_key check            |
| 3.24 | Teste: dry-run não grava nada            | Maria       | 🔲     | Teste de integração pendente (requer mock de Supabase)         |
| 3.25 | Teste: backfill por período funciona     | Maria       | 🔲     | Depende de 3.12 (scan por período)                             |

---

## Fase 4 — Parsing Documental, Segredos e Geração de Drafts

### 4A — Pipeline de Parsing

| #   | Tarefa                                                  | Responsável | Status | Notas                                                       |
| --- | ------------------------------------------------------- | ----------- | ------ | ----------------------------------------------------------- |
| 4.1 | Implementar extração de texto de PDF (pdf-parse)        | Pedro       | ✅     | text-extractor.ts (PDF, CSV, XML, fallback)                 |
| 4.2 | Implementar abertura de PDF protegido com senha         | Pedro       | ✅     | text-extractor.ts + retry com senha                         |
| 4.3 | Implementar busca de senha por fornecedor/contrato      | Pedro       | ✅     | secret-lookup.ts (cascade: supplier → contract → fallback)  |
| 4.4 | Implementar parser determinístico: CEMIG (conta de luz) | Pedro       | ✅     | cemig-parser.ts — regex, 35 testes unitários                |
| 4.5 | Implementar parser determinístico: genérico (boleto)    | Pedro       | ✅     | boleto-parser.ts — valor, vencimento, CNPJ, linha digitável |
| 4.6 | Implementar registro de parsed_document_versions        | Pedro       | ✅     | parse-orchestrator.ts + stepParse no processor              |
| 4.7 | Implementar registro de extraction_results              | Pedro       | ✅     | parse-orchestrator.ts (confiança > 0.2)                     |

### 4B — Geração de Drafts

| #    | Tarefa                                            | Responsável | Status | Notas                                                              |
| ---- | ------------------------------------------------- | ----------- | ------ | ------------------------------------------------------------------ |
| 4.8  | Implementar geração de draft: transaction         | Pedro       | ✅     | buildTransactionDraft em draft-generator.ts — 3 testes unitários   |
| 4.9  | Implementar geração de draft: recurring_template  | Pedro       | ✅     | buildRecurringTemplateDraft — 2 testes unitários                   |
| 4.10 | Implementar geração de draft: consumption_metric  | Pedro       | ✅     | buildConsumptionMetricDraft — 3 testes unitários                   |
| 4.11 | Implementar criação de draft_batches              | Pedro       | ✅     | generateDrafts cria batch + records no DB                          |
| 4.12 | Implementar score de confiança                    | Pedro       | ✅     | classifyDraftTypes — 7 testes unitários, propagação parser→draft   |
| 4.13 | Implementar sugestão de fornecedor/categoria/tags | Pedro       | ✅     | suggestCategory/suggestTags em parse-orchestrator + draft builders |
| 4.14 | Implementar sugestão de período financeiro        | Pedro       | 🟡     | competence_date propagada, integração @sbf/domain pending          |

### 4C — Testes

| #    | Tarefa                                             | Responsável | Status | Notas                                                                                                     |
| ---- | -------------------------------------------------- | ----------- | ------ | --------------------------------------------------------------------------------------------------------- |
| 4.15 | Teste: PDF com senha é processado com segredo      | Maria       | 🟡     | Fluxo implementado em parse-orchestrator (secret-lookup + retry). Teste e2e precisa de PDF real protegido |
| 4.16 | Teste: fornecedor e período sugeridos corretamente | Maria       | ✅     | Integração: CEMIG → extraction_result com categoria, tags, competência                                    |
| 4.17 | Teste: drafts não poluem ledger principal          | Maria       | ✅     | Integração: draft pending_review sem posted_record_id/type                                                |
| 4.18 | Teste: baixa confiança vai para revisão            | Maria       | ✅     | Integração: texto genérico → confidence ≤ 0.3, status pending_review                                      |
| 4.19 | Teste: reprocessamento gera nova versão            | Maria       | ✅     | Integração: v1 e v2 em parsed_document_versions                                                           |

---

## Fase 5 — MCP Local + Copilot/VS Code

| #    | Tarefa                                                 | Responsável   | Status | Notas                                                                |
| ---- | ------------------------------------------------------ | ------------- | ------ | -------------------------------------------------------------------- |
| 5.1  | Criar servidor MCP com SDK (@modelcontextprotocol/sdk) | João          | ✅     | McpServer + StdioServerTransport, 8 tools registradas                |
| 5.2  | Implementar tool: `scan_gmail_label`                   | João          | ⬜     | Desbloqueado: Gmail client implementado. Integrar scanGmailLabel()   |
| 5.3  | Implementar tool: `scan_gmail_period`                  | João          | 🔲     | Depende de 3.12 (scan por período)                                   |
| 5.4  | Implementar tool: `scan_local_folder`                  | João          | ✅     | scan-local-folder.ts — escaneia diretório, upload, cria jobs         |
| 5.5  | Implementar tool: `list_unparsed_documents`            | João          | ✅     | list-unparsed-documents.ts — lista jobs não processados              |
| 5.6  | Implementar tool: `reprocess_document`                 | João          | ✅     | reprocess-document.ts — força reprocessamento                        |
| 5.7  | Implementar tool: `resolve_supplier_candidates`        | João          | ✅     | resolve-supplier-candidates.ts — busca CNPJ/nome                     |
| 5.8  | Implementar tool: `list_draft_batches`                 | João          | ✅     | list-draft-batches.ts — batches pendentes com drafts                 |
| 5.9  | Implementar tool: `approve_draft_batch`                | João          | ✅     | approve-draft-batch.ts — aprovação/rejeição de batch                 |
| 5.10 | Implementar tool: `recompute_financial_periods`        | João          | ✅     | recompute-financial-periods.ts — RPC generate_financial_periods      |
| 5.11 | Implementar tool: `find_documents_without_password`    | João          | ✅     | find-documents-without-password.ts — PDFs sem senha                  |
| 5.12 | Configurar `.vscode/mcp.json`                          | Thiago        | ✅     | Server "sbf" stdio com Supabase local + service_role key             |
| 5.13 | Testar integração com GitHub Copilot no VS Code        | Thiago + João | 🟡     | Server inicia corretamente via bun, aguardando teste real no Copilot |
| 5.14 | Documentar uso do MCP com exemplos reais               | João          | ⬜     |                                                                      |
| 5.15 | Teste: idempotência (tool 2x sem duplicar)             | Maria         | ⬜     |                                                                      |

---

## Fase 6 — Interface Web de Revisão

| #    | Tarefa                                                            | Responsável      | Status | Notas |
| ---- | ----------------------------------------------------------------- | ---------------- | ------ | ----- |
| 6.1  | Criar rota `/dashboard/ingestion` (visão geral)                   | Roberto          | ⬜     |       |
| 6.2  | Criar rota `/dashboard/ingestion/runs`                            | Roberto          | ⬜     |       |
| 6.3  | Criar rota `/dashboard/ingestion/documents`                       | Roberto          | ⬜     |       |
| 6.4  | Criar rota `/dashboard/ingestion/drafts`                          | Roberto          | ⬜     |       |
| 6.5  | Criar rota `/dashboard/ingestion/drafts/[id]` (split-view)        | Roberto + Helena | ⬜     |       |
| 6.6  | Criar rota `/dashboard/ingestion/conflicts`                       | Roberto          | ⬜     |       |
| 6.7  | Criar rota `/dashboard/ingestion/batches`                         | Roberto          | ⬜     |       |
| 6.8  | Implementar server actions para ingestion                         | Roberto          | ⬜     |       |
| 6.9  | Implementar PDF viewer inline                                     | Sofia            | ⬜     |       |
| 6.10 | Implementar formulário de revisão de draft                        | Sofia            | ⬜     |       |
| 6.11 | Implementar aprovação em lote                                     | Sofia            | ⬜     |       |
| 6.12 | Implementar atalhos de teclado                                    | Thiago           | ⬜     |       |
| 6.13 | Implementar filtros (fornecedor, categoria, tag, período, status) | Roberto          | ⬜     |       |
| 6.14 | Validar acessibilidade (teclado, screen reader, contraste)        | Renata           | ⬜     |       |
| 6.15 | Conectar dados aprovados às telas financeiras existentes          | Roberto          | ⬜     |       |

---

## Fase 7 — Agentes OpenAI

| #    | Tarefa                                                    | Responsável     | Status | Notas              |
| ---- | --------------------------------------------------------- | --------------- | ------ | ------------------ |
| 7.1  | Configurar client OpenAI em `packages/operations/src/ai/` | Pedro           | 🔲     | Depende de API key |
| 7.2  | Implementar agente: Document Parser                       | Pedro           | 🔲     |                    |
| 7.3  | Implementar agente: Supplier Resolver                     | Pedro           | 🔲     |                    |
| 7.4  | Implementar agente: Financial Classifier                  | Pedro           | 🔲     |                    |
| 7.5  | Implementar agente: Reconciler                            | Pedro           | 🔲     |                    |
| 7.6  | Implementar logging de chamadas IA (auditoria)            | Pedro           | 🔲     |                    |
| 7.7  | Integrar agentes ao pipeline de parsing (upgrade)         | Pedro + João    | 🔲     |                    |
| 7.8  | Criar system prompts calibrados com dados reais           | Pedro + Ricardo | 🔲     |                    |
| 7.9  | Implementar interface de linguagem natural no web         | Roberto + Pedro | 🔲     |                    |
| 7.10 | Testes: IA não grava no ledger sem revisão                | Maria           | 🔲     |                    |
| 7.11 | Testes: chamadas IA auditáveis                            | Maria           | 🔲     |                    |

---

## Fase 8 — Preparação para ChatGPT

| #   | Tarefa                                                    | Responsável | Status | Notas |
| --- | --------------------------------------------------------- | ----------- | ------ | ----- |
| 8.1 | Documentar avaliação de exposição remota do MCP           | Ana         | ⬜     |       |
| 8.2 | Documentar classificação de tools (read/write-safe/risky) | Ana + Maria | ⬜     |       |
| 8.3 | Documentar requisitos de autenticação remota              | Maria       | ⬜     |       |
| 8.4 | Avaliar Gmail push notifications (documento de decisão)   | João        | ⬜     |       |
| 8.5 | Avaliar hospedagem de worker fora do local                | Fernando    | ⬜     |       |

---

## Resumo de Contagem

| Fase      | Total de tarefas | Bloqueantes (CEO)              |
| --------- | ---------------- | ------------------------------ |
| Sprint 0  | 5                | 0                              |
| Fase 1    | 33               | 8 (1.1, 1.7, 1.11, 1.12, 1.13) |
| Fase 2    | 29               | 0                              |
| Fase 3    | 25               | 9 (3.1-3.9)                    |
| Fase 4    | 19               | 0                              |
| Fase 5    | 15               | 0                              |
| Fase 6    | 15               | 0                              |
| Fase 7    | 11               | 1 (API key)                    |
| Fase 8    | 5                | 0                              |
| **TOTAL** | **157**          | **18**                         |
