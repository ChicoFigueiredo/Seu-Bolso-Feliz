---
Título da Reunião: Refino Obrigatório — Plano de Ação Concreto: Ingestão, IA e Staging
Data e Hora: 2026-03-31 19:40
Participantes:
  - Chico (CEO) — solicitante, decisor final
  - Camila Duarte (Consultora Finanças Pessoais) — cobrança e validação operacional
  - Ana Silva (Arquiteta) — decisões de arquitetura
  - Roberto Lima (Frontend Sr.) — UI de ingestão e chat
  - Sofia Almeida (Frontend Sr.) — server actions e formulários
  - João Pereira (Backend/Bun) — workers, pipeline, integração OpenAI
  - Maria Oliveira (Backend/Bun) — MCP, segurança, testes
  - Fernando Gomes (DevOps) — deploy, ambientes, promoção
  - André Santos (DBA) — migrations, performance, modelagem
  - Ricardo Monteiro (Economista) — validação de regras financeiras
  - Helena Vargas (UX/UI) — design da UI de ingestão
  - Renata Silva (QA) — critérios de aceite, testes
Pauta:
  - Diagnóstico consolidado do estado atual
  - Plano de ação faseado com marcos verificáveis em staging
  - Proposta de integração OpenAI/ChatGPT
  - Proposta de padrões documentais e memória operacional
  - Proposta de conciliação e associação inteligente
  - Proposta de expansão do MCP
  - Proposta de promoção entre ambientes
  - Backlog priorizado de implementação
---

# 1. Resumo Executivo

Este documento responde à demanda da consultora Camila Duarte e do CEO: **um plano de ação concreto, faseado e verificável para tirar a ingestão do limbo e colocá-la funcionando em staging com dados reais, revisão humana e IA integrada.**

**Linha de fundo:** O backend financeiro está 100% pronto (36 tabelas, 19 migrations, 168+ testes, 3 Edge Functions, RLS completo). Os workers de ingestão (Gmail scanner, local scanner, ingestion pipeline) estão 95% prontos. O MCP tem 8 ferramentas. O que falta é:

1. **UI de ingestão** — não existe nenhuma tela para revisar/aprovar documentos
2. **Deploy web real** — o job do GitLab CI é placeholder
3. **Google OAuth em staging** — só funciona em localhost
4. **Integração OpenAI** — parser e chat não integrados
5. **Padrões documentais** — o sistema não aprende com correções humanas
6. **Conciliação** — não há lógica para associar documento a registro existente
7. **Promoção entre ambientes** — não existe ferramenta para mover dados

**Compromisso da equipe:** Entregar 6 marcos verificáveis em staging, cada um testável pelo CEO.

---

# 2. Diagnóstico Consolidado

## 2.1 O Que Está Pronto e Funcionando

| Módulo                                    | Status  | Evidência                                                           |
| ----------------------------------------- | ------- | ------------------------------------------------------------------- |
| Modelagem financeira (domain)             | ✅ 100% | 3 sistemas de amortização, ciclos, priorização, dedup — 80+ testes  |
| Banco de dados (Supabase)                 | ✅ 100% | 36 tabelas, 19 migrations, RLS, triggers, views, 3 Edge Functions   |
| Validação (Zod schemas)                   | ✅ 100% | Schemas para todas as entidades                                     |
| Tipos compartilhados                      | ✅ 100% | 1678 linhas auto-geradas do Supabase                                |
| Operações (hash, origin-key, idempotency) | ✅ 100% | SHA-256, dedup, 12+ testes                                          |
| Tipos de ingestão (enums, interfaces)     | ✅ 100% | 24 estados na máquina de estados                                    |
| Gmail scanner worker                      | ✅ 85%  | OAuth2, listagem, anexos, dedup por content_hash, dry-run           |
| Local scanner worker                      | ✅ 100% | Scan, upload, dedup, filtros por extensão                           |
| Ingestion worker                          | ✅ 95%  | Poll loop, state machine, parsing PDF, draft generation             |
| MCP server                                | ✅ 80%  | 8 ferramentas: scan, list, reprocess, approve, resolve              |
| Web app core                              | ✅ 70%  | 26 rotas, 31 componentes shadcn/ui, auth, server actions CRUD       |
| Testes                                    | ✅ 100% | 168+ testes, todos passando, 0 falhas                               |
| Seed data                                 | ✅ 100% | Dados realistas: 3 bancos, 7 produtos, 8 categorias, 7 fornecedores |

## 2.2 O Que NÃO Está Pronto

| Gap                                       | Impacto                                       | Bloqueador de quem        |
| ----------------------------------------- | --------------------------------------------- | ------------------------- |
| UI de ingestão (`/dashboard/ingestion/*`) | CEO não consegue revisar/aprovar documentos   | Time de frontend          |
| Deploy web real (Vercel CLI no CI)        | Nada visível fora de localhost                | DevOps                    |
| Google OAuth em staging/produção          | Login falha fora de localhost                 | CEO (config GCP) + DevOps |
| Integração OpenAI (parser + chat)         | Sem classificação inteligente, sem assistente | Backend + Frontend        |
| Padrões documentais                       | Sistema não aprende com correções             | Backend + DBA             |
| Conciliação documento↔registro            | Sem associação automática/sugerida            | Backend                   |
| Promoção entre ambientes                  | Sem como mover dados de forma segura          | DevOps                    |
| Dashboard operacional ("primeira tela")   | Sem tela orientada a ação/decisão             | Frontend                  |

## 2.3 O Que Depende do CEO

| Ação                                                        | Urgência                   | Status   |
| ----------------------------------------------------------- | -------------------------- | -------- |
| Configurar Google OAuth no Supabase Staging (redirect URIs) | 🔴 BLOQUEADOR para Marco 1 | Pendente |
| Configurar Google OAuth no Supabase Production              | 🟡 Futuro                  | Pendente |
| Obter API key da OpenAI e colocar no `.env`                 | 🔴 BLOQUEADOR para Marco 4 | Pendente |
| Testar Gmail scan com dados reais (1000+ e-mails)           | 🟠 Validação               | Pendente |
| Validar cada marco em staging                               | Contínuo                   | —        |

---

# 3. Decisões de Arquitetura Para Esta Fase

## 3.1 Chat com IA: Drawer lateral persistente (não página dedicada)

**Decisão:** O chat será um **drawer lateral acessível de qualquer página** do dashboard, não uma página separada.

**Justificativa (Ana Silva):**

- O CEO pode conversar enquanto visualiza documentos, drafts, transações
- Não interrompe o fluxo de trabalho
- Upload de documento pode ser feito tanto pela UI de ingestão quanto pelo chat
- O drawer mantém contexto da conversa enquanto o CEO navega

**Implementação:**

- Componente `<AIChatDrawer />` no layout do dashboard
- Botão flutuante no canto inferior direito
- Estado aberto/fechado persistido no localStorage
- Histórico de conversas salvo no Supabase (`ai_chat_sessions`, `ai_chat_messages`)

## 3.2 Backend da IA: Edge Function + Vercel AI SDK

**Decisão:** Usar **Vercel AI SDK** com streaming no frontend e **Supabase Edge Function** como proxy para OpenAI.

**Justificativa (João Pereira + Maria Oliveira):**

- Vercel AI SDK suporta streaming nativo, bom UX
- Edge Function mantém API key no server-side (nunca exposta ao cliente)
- Edge Function aplica rate limiting, autenticação e auditoria
- Edge Function acessa o MCP interno (ferramentas do pipeline) sem expor ao cliente

**Fluxo:**

```
Frontend (Vercel AI SDK) → API Route Next.js → Supabase Edge Function → OpenAI API
                                                       ↓
                                               Pipeline de ingestão
                                               (Storage, workers, drafts)
```

## 3.3 IA não decide sozinha — sempre via draft + revisão humana

**Decisão:** A IA **nunca grava diretamente** no ledger financeiro. Toda sugestão da IA gera um `draft_record` que precisa de aprovação humana.

**Justificativa (Camila Duarte + Ricardo Monteiro):**

- Dados financeiros exigem precisão — erro de classificação pode distorcer todo o controle
- Nas primeiras semanas, o sistema precisa aprender com as correções do CEO
- Confiança cresce gradualmente — autopost pode ser liberado depois com threshold

## 3.4 Parser híbrido: determinístico primeiro, IA como fallback

**Decisão:** Manter parsers regex (CEMIG, boleto) como **primeira camada**. OpenAI Vision/GPT como **segunda camada** para documentos não reconhecidos.

**Justificativa (Maria Oliveira):**

- Parsers determinísticos são mais rápidos, baratos e previsíveis
- IA entra quando regex não extrai dados suficientes ou quando o tipo é desconhecido
- Cada extração da IA registra `parser_type = 'openai'` e `confidence_score`

## 3.5 Promoção entre ambientes: CLI com dry-run e escopos

**Decisão:** Criar script TypeScript `scripts/promote.ts` (não ferramenta complexa) com:

- `--from local --to staging`
- `--scope suppliers,patterns,categories`
- `--dry-run`
- Hash de verificação por entidade

**Justificativa (Fernando Gomes):**

- Simplicidade > complexidade nesta fase
- Dry-run obrigatório antes de qualquer promoção real
- Escopos evitam sync cego
- Auditoria via `promotion_logs` no banco

---

# 4. Fases de Execução

## Fase A — Base Operacional Mínima para Staging

### Objetivo

Staging acessível, autenticação funcional, pipeline observável. O CEO consegue fazer login em staging e ver o dashboard.

### O que será reaproveitado

- ✅ Web app inteiro (Next.js, 26 rotas, auth, layout, server actions)
- ✅ 19 migrations Supabase (staging já tem todas aplicadas)
- ✅ 3 Edge Functions (já deployadas em staging)
- ✅ Configuração Vercel (conta e projeto já existem)

### O que precisa ser feito

| #    | Tarefa                                                                                                                                | Responsável         | Tipo       |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------- | ---------- |
| A.1  | Substituir placeholder deploy-web por Vercel CLI real no GitLab CI                                                                    | Fernando Gomes      | infra      |
| A.2  | Configurar variáveis de ambiente no Vercel (SUPABASE_URL, ANON_KEY, etc.)                                                             | Fernando Gomes      | infra      |
| A.3  | **CEO:** Configurar Google OAuth no Supabase Staging Dashboard (redirect URI para domínio Vercel)                                     | CEO                 | manual     |
| A.4  | Configurar domínio customizado na Vercel (seubolsofeliz.com.br ou staging.seubolsofeliz.com.br)                                       | Fernando Gomes      | infra      |
| A.5  | Verificar que todas as 19 migrations estão aplicadas em staging                                                                       | André Santos        | db         |
| A.6  | Seed data opcional em staging (para ter dados de teste)                                                                               | André Santos        | db         |
| A.7  | Testar login via Google OAuth em staging                                                                                              | CEO + Renata        | qa         |
| A.8  | Adicionar logging mínimo: tabela `system_logs` para eventos de pipeline                                                               | André Santos + João | db/backend |
| A.9  | Criar página `/dashboard/logs` (readonly, lista de eventos recentes)                                                                  | Roberto Lima        | frontend   |
| A.10 | **Server action `triggerReprocess(documentId)` para staging** — permite disparar reprocessamento pela UI sem depender de worker local | Sofia Almeida       | backend    |
| A.11 | **Server action `triggerGmailScan(params)` via Edge Function** — permite disparar scan Gmail pela UI em staging sem CLI local         | João Pereira        | backend    |

> **Nota (Correção #2 — Veronica):** A partir do Marco 1, staging NÃO depende de operação local. O CEO pode: (a) fazer upload manual pela UI (Marco 3, task B.15/022b), (b) disparar reprocessamento pela UI (task A.10), e (c) disparar scan Gmail via Edge Function (task A.11). Workers locais são alternativa para volume/backfill, não dependência.

> **Nota (Correção #4 — Veronica):** Logging/observabilidade é Marco 1, não pós-Marco 6. Tasks A.8 e A.9 são obrigatórias para staging operacional. A referência anterior no backlog P3 (task 063) foi removida por redundância.

### O que sobe em staging

- App web real na Vercel (não placeholder)
- Login via Google OAuth funcional
- Dashboard acessível com dados

### Como o CEO testa

1. Acessar `https://staging.seubolsofeliz.com.br` (ou URL da Vercel)
2. Fazer login com Google
3. Ver o dashboard com dados
4. Ver página `/dashboard/logs` (pode estar vazia, mas carrega)

### Critérios de aceite

- [ ] URL de staging acessível publicamente
- [ ] Login via Google funciona sem erro
- [ ] Dashboard carrega com layout e sidebar
- [ ] Página de logs existe e carrega
- [ ] HTTPS obrigatório

### Riscos e bloqueadores

- 🔴 **Bloqueador CEO:** Configurar OAuth no Supabase Staging (task A.3)
- 🟡 Possível problema de CORS entre Vercel e Supabase Staging

---

## Fase B — UI de Ingestão Mínima, Mas Usável

### Objetivo

CEO consegue ver documentos ingeridos, abrir o detalhe de cada um, ver o PDF ao lado dos drafts, editar campos e aprovar/rejeitar.

### O que será reaproveitado

- ✅ 31 componentes shadcn/ui (table, dialog, sheet, badge, skeleton, tabs, card, button, form, select, etc.)
- ✅ Server actions pattern (já funciona para institutions, products, transactions, etc.)
- ✅ Supabase client SSR (já configurado)
- ✅ Layout dashboard com sidebar (já existe)
- ✅ Tabelas de ingestão no banco (ingestion_jobs, source_documents, draft_records, draft_batches)
- ✅ Enums e tipos de ingestão (packages/ingestion-types)

### O que precisa ser criado do zero

| #    | Tarefa                                                                                                                                     | Responsável   | Tipo     |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------ | ------------- | -------- |
| B.1  | Server actions para ingestão: `listDocuments`, `getDocument`, `listDrafts`, `getDraft`, `approveDraft`, `rejectDraft`, `reprocessDocument` | Sofia Almeida | backend  |
| B.2  | Rota `/dashboard/ingestion` — Página principal com contadores (total, pendentes, aprovados, rejeitados, erros)                             | Roberto Lima  | frontend |
| B.3  | Rota `/dashboard/ingestion/documents` — Listagem com filtros (status, origem, fornecedor, tipo, período)                                   | Roberto Lima  | frontend |
| B.4  | Rota `/dashboard/ingestion/documents/[id]` — Detalhe com split-view: PDF à esquerda, dados à direita                                       | Roberto Lima  | frontend |
| B.5  | Componente `<PDFViewer />` — Renderizar PDF do Supabase Storage inline (usar react-pdf ou iframe)                                          | Roberto Lima  | frontend |
| B.6  | Componente `<DraftReviewForm />` — Formulário editável: descrição, valor, data, categoria, fornecedor, tags, tipo                          | Sofia Almeida | frontend |
| B.7  | Componente `<DraftApprovalActions />` — Botões: aprovar, rejeitar, reprocessar, com confirmação                                            | Sofia Almeida | frontend |
| B.8  | Componente `<StatusBadge />` — Badge visual para cada status da máquina de estados (24 estados)                                            | Roberto Lima  | frontend |
| B.9  | Componente `<ConfidenceIndicator />` — Barra ou ícone mostrando confiança da extração (0-100%)                                             | Roberto Lima  | frontend |
| B.10 | Lógica de aprovação: ao aprovar draft, gravar no ledger (transactions/recurring/etc.) e atualizar status                                   | Sofia + João  | backend  |
| B.11 | Lógica de reprocessamento: resetar status do job para QUEUED e disparar reanálise                                                          | João Pereira  | backend  |
| B.12 | Batch approval: selecionar múltiplos drafts e aprovar de uma vez                                                                           | Sofia Almeida | frontend |
| B.13 | Indicadores visuais: erro (vermelho), pendência (amarelo), falta de senha (ícone cadeado), baixa confiança (warning)                       | Roberto Lima  | frontend |
| B.14 | Empty states e loading states para todas as páginas de ingestão                                                                            | Roberto Lima  | frontend |
| B.15 | **Upload manual via UI: drag & drop de arquivo na página de ingestão** (antecipado do Marco 5 por decisão da consultora)                   | Roberto Lima  | frontend |

### O que sobe em staging

- 4 páginas novas de ingestão funcionais
- PDF viewer inline
- Formulário de revisão com edição
- Aprovação/rejeição individual e em lote
- **Upload manual de documentos via drag & drop (sem depender de worker local)**

### Como o CEO testa

1. **Via upload manual (não depende de worker local):** Acessar `/dashboard/ingestion`, arrastar PDF para zona de upload → documento entra no pipeline automaticamente
2. **Via Gmail scanner (alternativa local):** `bun run pipeline:passo01:scan-gmail:10` → `bun run pipeline:passo02:ingest`
3. **Via Edge Function de reprocessamento (staging centralizado):** Acessar `/dashboard/ingestion`, clicar "Reprocessar" em qualquer documento pendente
4. Acessar `/dashboard/ingestion` — ver contadores
5. Acessar `/dashboard/ingestion/documents` — ver lista de documentos
6. Clicar num documento — ver split-view com PDF e dados
7. Editar campos do draft (descrição, valor, categoria)
8. Aprovar um draft — verificar que aparece em `/dashboard/transactions`
9. Rejeitar outro draft — verificar que fica marcado como rejeitado
10. Selecionar 5 drafts e aprovar em lote

### Critérios de aceite

- [ ] Página `/dashboard/ingestion` carrega com contadores
- [ ] Listagem mostra documentos com status, origem e data
- [ ] Filtros por status e origem funcionam
- [ ] Detalhe mostra PDF inline ao lado dos dados extraídos
- [ ] Formulário permite editar descrição, valor, data, categoria, fornecedor, tags
- [ ] Aprovação individual grava registro no ledger e atualiza status
- [ ] Rejeição marca draft como rejeitado
- [ ] Aprovação em lote funciona para 5+ drafts
- [ ] Documentos com erro mostram indicador visual vermelho
- [ ] Documentos sem senha mostram ícone de cadeado
- [ ] Drafts com baixa confiança mostram warning

### Riscos e bloqueadores

- 🟡 PDF viewer pode ter problemas com PDFs protegidos por senha (exibir mensagem clara)
- 🟡 Drafts gerados pelo worker podem ter campos incompletos (tratar gracefully)

---

## Fase C — Integração OpenAI / ChatGPT Dentro do Seu Bolso Feliz

### Objetivo

Chat funcional na interface, upload de documento pelo chat, IA sugerindo fornecedor/tipo/campos, explicação das classificações, listagem de pendências.

### Arquitetura Proposta

```
┌─────────────────────────────────────────────────┐
│  Frontend (Next.js)                              │
│  ┌───────────────────────────────────────────┐  │
│  │  <AIChatDrawer />                         │  │
│  │  - Vercel AI SDK (useChat hook)           │  │
│  │  - Streaming de respostas                 │  │
│  │  - Upload de arquivos no chat             │  │
│  │  - Histórico local + persistido           │  │
│  └─────────────┬─────────────────────────────┘  │
│                │                                  │
│  ┌─────────────▼─────────────────────────────┐  │
│  │  API Route: /api/chat                     │  │
│  │  - Autenticação via Supabase session      │  │
│  │  - Rate limiting (10 msgs/min)            │  │
│  │  - Logging de mensagens                   │  │
│  │  - Upload → Storage + pipeline            │  │
│  └─────────────┬─────────────────────────────┘  │
└────────────────┼────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  OpenAI API (gpt-4o / gpt-4o-mini)             │
│  - System prompt com contexto do SBF            │
│  - Function calling (tools)                     │
│  - Vision para análise de documentos            │
│                                                  │
│  Tools disponíveis para o modelo:               │
│  ┌────────────────────────────────────────────┐ │
│  │ list_pending_documents()                   │ │
│  │ get_document_details(id)                   │ │
│  │ suggest_supplier(text)                     │ │
│  │ suggest_document_type(text)                │ │
│  │ list_documents_with_errors()               │ │
│  │ list_documents_without_password()          │ │
│  │ list_unresolved_suppliers()                │ │
│  │ approve_draft(id, corrections?)            │ │
│  │ reject_draft(id, reason)                   │ │
│  │ upload_and_ingest(file)                    │ │
│  │ search_transactions(query)                 │ │
│  │ suggest_reconciliation(draft_id)           │ │
│  │ explain_classification(job_id)             │ │
│  │ list_document_patterns()                   │ │
│  │ register_document_pattern(pattern)         │ │
│  └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

### O que precisa ser feito

| #    | Tarefa                                                                      | Responsável    | Tipo     |
| ---- | --------------------------------------------------------------------------- | -------------- | -------- |
| C.1  | **CEO:** Obter API key OpenAI e adicionar ao `.env` e Vercel env vars       | CEO            | manual   |
| C.2  | Migration: tabelas `ai_chat_sessions` e `ai_chat_messages` com RLS          | André Santos   | db       |
| C.3  | API Route `/api/chat` com autenticação, rate limiting, streaming            | João Pereira   | backend  |
| C.4  | System prompt do SBF (contexto financeiro, regras, personalidade)           | João + Camila  | backend  |
| C.5  | Implementar function calling tools (15 tools listadas acima)                | João + Maria   | backend  |
| C.6  | Componente `<AIChatDrawer />` com Vercel AI SDK `useChat`                   | Roberto Lima   | frontend |
| C.7  | Upload de arquivo no chat (drag & drop + botão)                             | Roberto Lima   | frontend |
| C.8  | Renderização de mensagens: markdown, tabelas, badges de status              | Roberto Lima   | frontend |
| C.9  | Integração OpenAI Vision: enviar imagem de documento para análise           | João Pereira   | backend  |
| C.10 | Parser OpenAI: fallback quando parser regex não extrai dados suficientes    | João Pereira   | backend  |
| C.11 | Auditoria: toda interação com IA logada (`ai_chat_messages` + `audit_logs`) | Maria Oliveira | backend  |
| C.12 | Tratamento de erro: API key inválida, rate limit, timeout                   | João Pereira   | backend  |

### Casos de uso do chat — Exemplos concretos

**Exemplo 1 — Upload via chat:**

```
CEO: [arrasta PDF para o chat]
IA: Recebi o documento "fatura-nubank-mar-2026.pdf". Enviei para processamento.
    Fornecedor sugerido: Nubank (confiança: 95%)
    Tipo: Fatura de cartão de crédito
    Valor total: R$ 2.847,32
    Vencimento: 15/04/2026
    Deseja que eu crie o draft para revisão?
CEO: Sim, cria
IA: Draft criado! Você pode revisá-lo em /dashboard/ingestion/documents/abc123
```

**Exemplo 2 — Listagem de pendências:**

```
CEO: O que tá pendente?
IA: Você tem:
    - 12 documentos aguardando revisão
    - 3 documentos com erro de parsing
    - 2 documentos sem senha de PDF
    - 1 fornecedor não resolvido
    Quer que eu detalhe algum grupo?
```

**Exemplo 3 — Aprovação assistida:**

```
CEO: Aprova todos os da CEMIG
IA: Encontrei 4 drafts da CEMIG:
    1. Fatura Mar/2026 — R$ 312,45 — confiança 98%
    2. Fatura Fev/2026 — R$ 287,12 — confiança 97%
    3. Fatura Jan/2026 — R$ 301,88 — confiança 96%
    4. Fatura Dez/2025 — R$ 445,23 — confiança 89% ⚠️
    Os 3 primeiros têm alta confiança. O último tem confiança 89% porque
    o valor está fora do padrão histórico. Quer aprovar os 3 primeiros
    e revisar o último manualmente?
```

### Fora de escopo agora (Fase C)

- ❌ Consultoria financeira ampla (quanto investir, onde aplicar)
- ❌ Multiagentes sofisticados (múltiplos modelos conversando entre si)
- ❌ Chat no mobile
- ❌ Automações secundárias (webhooks, cron jobs via chat)
- ❌ Integração com outros LLMs além do OpenAI
- ❌ Voice input/output
- ❌ Autopost sem revisão humana (virá em fase futura com threshold de confiança)

### Critérios de aceite

- [ ] Drawer de chat abre/fecha de qualquer página do dashboard
- [ ] CEO consegue enviar mensagem e receber resposta com streaming
- [ ] Upload de arquivo no chat funciona e documento entra no pipeline
- [ ] Chat lista pendências de ingestão corretamente
- [ ] Chat sugere fornecedor com score de confiança
- [ ] Chat explica por que classificou algo de determinada forma
- [ ] Chat permite aprovar drafts com confirmação
- [ ] Histórico de conversas persiste entre sessões
- [ ] Rate limiting impede abuso (10 msgs/min)
- [ ] API key nunca aparece no frontend

---

## Fase D — Padrões de Documentos e Memória Operacional

### Objetivo

O sistema aprende com as correções humanas e reaproveita padrões em documentos futuros. O CEO não precisa reensinar o sistema toda vez.

### Modelagem proposta

**Nova tabela: `document_patterns`**

```sql
CREATE TABLE document_patterns (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES auth.users(id),
  name TEXT NOT NULL,                           -- Ex: "Fatura CEMIG"
  supplier_id UUID REFERENCES suppliers(id),    -- Fornecedor associado
  document_type TEXT NOT NULL,                  -- Ex: "utility_bill", "credit_card_statement"
  institution_id UUID REFERENCES institutions(id),
  extraction_rules JSONB NOT NULL DEFAULT '{}', -- Regras de extração: campos, posições, regex
  field_mappings JSONB NOT NULL DEFAULT '{}',   -- Mapeamento: campo_extraído → campo_destino
  sample_fingerprints TEXT[] DEFAULT '{}',      -- Hashes de documentos-modelo
  confidence_threshold NUMERIC(3,2) DEFAULT 0.80, -- Mínimo para auto-sugestão
  version INT NOT NULL DEFAULT 1,
  is_active BOOLEAN DEFAULT true,
  feedback_count INT DEFAULT 0,                 -- Quantas vezes foi corrigido
  success_count INT DEFAULT 0,                  -- Quantas vezes acertou
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(user_id, name, version)
);
```

**Nova tabela: `pattern_feedback`**

```sql
CREATE TABLE pattern_feedback (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES auth.users(id),
  pattern_id UUID NOT NULL REFERENCES document_patterns(id),
  source_document_id UUID REFERENCES source_documents(id),
  feedback_type TEXT NOT NULL CHECK (feedback_type IN ('correct', 'incorrect', 'partial', 'improved')),
  corrections JSONB DEFAULT '{}',               -- O que o CEO corrigiu
  notes TEXT,
  created_at TIMESTAMPTZ DEFAULT now()
);
```

### Fluxo operacional

1. **Parser processa documento** → extrai campos
2. **Sistema busca padrões existentes** para o mesmo fornecedor/tipo
3. Se encontra padrão com `is_active = true`:
   - Aplica `extraction_rules` e `field_mappings`
   - Compara resultado com extração bruta
   - Se confiança > `confidence_threshold`: sugere automaticamente
   - Se confiança < threshold: mostra para revisão com indicação "padrão existente com baixa confiança"
4. **CEO revisa e aprova/corrige**
5. Se corrigiu:
   - Cria registro em `pattern_feedback` com tipo `improved`
   - Incrementa `feedback_count` no padrão
   - Se feedback negativo > 3x: marca padrão como `is_active = false` (precisa de revisão)
6. Se aprovou sem correção:
   - Incrementa `success_count`

### Como eu corrijo um padrão

Via UI (`/dashboard/ingestion/patterns`):

- Lista de padrões ativos
- Detalhe do padrão com regras de extração
- Histórico de feedback (acertos vs erros)
- Botão para desativar padrão ruim
- Botão para criar nova versão (incrementa `version`, copia regras, CEO ajusta)

Via chat:

```
CEO: O padrão da CEMIG tá errado, ele tá puxando o valor de consumo no campo errado
IA: Entendi. O padrão "Fatura CEMIG v2" mapeia o campo "total" para a posição X.
    Qual é a posição correta?
CEO: É o valor que aparece depois de "TOTAL A PAGAR"
IA: Atualizei o padrão. Nova versão v3 criada. Vou reprocessar os 2 documentos
    que usaram a versão incorreta. Deseja?
```

### Como impedir que padrão ruim contamine extrações futuras

- Padrão com `feedback_count` >= 3 e taxa de sucesso < 50%: automaticamente desativado
- Padrão desativado não é usado em extrações futuras
- CEO pode reativar manualmente após correção
- Versionamento garante que correção não altera extrações já aprovadas

### O que precisa ser feito

| #   | Tarefa                                                                                 | Responsável    | Tipo     |
| --- | -------------------------------------------------------------------------------------- | -------------- | -------- |
| D.1 | Migration: tabelas `document_patterns` e `pattern_feedback` com RLS                    | André Santos   | db       |
| D.2 | Server actions: CRUD de patterns, listagem, feedback                                   | Sofia Almeida  | backend  |
| D.3 | Rota `/dashboard/ingestion/patterns` — lista + detalhe + edição                        | Roberto Lima   | frontend |
| D.4 | Integrar padrões no ingestion worker (buscar padrão antes de parsear)                  | João Pereira   | backend  |
| D.5 | Lógica de auto-desativação de padrões ruins                                            | Maria Oliveira | backend  |
| D.6 | Tools de chat: `list_document_patterns`, `register_document_pattern`, `update_pattern` | João Pereira   | backend  |
| D.7 | Seed: criar 3-5 padrões iniciais (CEMIG, boleto, fatura cartão)                        | João + Ricardo | backend  |

### Critérios de aceite

- [ ] Página `/dashboard/ingestion/patterns` lista padrões existentes
- [ ] CEO consegue criar padrão manualmente
- [ ] CEO consegue editar/corrigir padrão existente
- [ ] Padrão é usado automaticamente quando fornecedor/tipo bate
- [ ] Feedback é registrado quando CEO corrige extração
- [ ] Padrão com muitas correções é automaticamente desativado
- [ ] Chat lista e gerencia padrões

---

## Fase E — Reconciliação e Associação Inteligente

### Objetivo

O sistema associa documentos a registros financeiros existentes, evita duplicações e sugere conciliações.

### Heurísticas de conciliação

**Camada 1 — Parser determinístico:**

- Match por `supplier_id` + `amount` + `due_date` (±3 dias)
- Match por `origin_key` (já deduplicado por hash)
- Match por `description` normalizada (Levenshtein distance < 0.2)
- Match por `barcode` / `nosso_numero` (quando disponível em boletos)

**Camada 2 — IA (OpenAI):**

- Análise semântica do texto extraído vs transações existentes
- Sugestão de fornecedor baseada em histórico de aliases
- Detecção de padrão recorrente (mesma fatura todo mês)
- Score de confiança para cada sugestão

**Política de confiança:**

| Score   | Comportamento                                             |
| ------- | --------------------------------------------------------- |
| 95-100% | Sugestão destacada em verde, botão "aprovar sugerido"     |
| 80-94%  | Sugestão com indicador amarelo, revisão recomendada       |
| 60-79%  | Sugestão com warning, edição obrigatória antes de aprovar |
| < 60%   | Sem sugestão automática, CEO classifica manualmente       |

**Política de bloqueio para autopost:**

- Nunca autopost na Fase 1 (toda aprovação é manual)
- Na Fase 2 (futura): autopost apenas para score >= 98% E padrão com >= 10 acertos consecutivos
- Na Fase 3 (futura): CEO configura threshold por fornecedor/tipo

### Interface de revisão da conciliação

No detalhe do documento (`/dashboard/ingestion/documents/[id]`):

- Seção "Conciliação sugerida"
- Lista de registros candidatos com score
- Botão "vincular a este registro" para cada candidato
- Botão "criar novo registro" se nenhum candidato serve
- Indicação visual: "novo", "recorrência", "duplicado", "pagamento de fatura"

### O que precisa ser feito

| #   | Tarefa                                                                         | Responsável    | Tipo     |
| --- | ------------------------------------------------------------------------------ | -------------- | -------- |
| E.1 | Módulo `reconciliation.ts` no packages/operations: heurísticas determinísticas | Maria Oliveira | backend  |
| E.2 | Integrar reconciliação no ingestion worker (executar após draft generation)    | João Pereira   | backend  |
| E.3 | API Route para buscar candidatos de conciliação dado um draft_id               | João Pereira   | backend  |
| E.4 | Componente `<ReconciliationPanel />` no detalhe do documento                   | Roberto Lima   | frontend |
| E.5 | Tool de IA: `suggest_reconciliation(draft_id)` — retorna candidatos ranqueados | João Pereira   | backend  |
| E.6 | Coluna `reconciliation_status` e `reconciled_with` na tabela `draft_records`   | André Santos   | db       |
| E.7 | Testes: cenários de match, não-match, duplicado, recorrência                   | Renata Silva   | qa       |

### Critérios de aceite

- [ ] Draft mostra candidatos de conciliação com score
- [ ] Score >= 95% aparece destacado em verde
- [ ] Score < 60% não mostra sugestão automática
- [ ] CEO consegue vincular draft a transação existente
- [ ] CEO consegue ignorar sugestão e criar registro novo
- [ ] Duplicados são detectados e marcados
- [ ] Chat explica por que sugeriu determinada conciliação

---

## Fase F — Ingestão por Gmail e Pasta Local Sem Enrolação

### Objetivo

Os dois canais de ingestão (Gmail e pasta local) funcionam de ponta a ponta com visibilidade na UI.

### Gmail — Plano Concreto

**O que já está implementado:**

- ✅ OAuth2 com refresh token
- ✅ Listagem por label
- ✅ Download de anexos
- ✅ Upload para Storage
- ✅ Dedup por content_hash
- ✅ Dry-run
- ✅ Logging

**O que falta:**

| #   | Tarefa                                                                                                              | Responsável    | Tipo     |
| --- | ------------------------------------------------------------------------------------------------------------------- | -------------- | -------- |
| F.1 | Scan por query livre: `--query "from:nubank subject:fatura"`                                                        | João Pereira   | backend  |
| F.2 | Scan por período: `--after 2026-01-01 --before 2026-03-31`                                                          | João Pereira   | backend  |
| F.3 | Backfill histórico: processar últimos 6 meses de e-mails financeiros                                                | João Pereira   | backend  |
| F.4 | Rate limiting: respeitar quota Gmail API (250 unidades/segundo)                                                     | João Pereira   | backend  |
| F.5 | Progresso na UI: mostrar scan em andamento (`/dashboard/ingestion` mostra "scan Gmail rodando, 45/300 processados") | Roberto Lima   | frontend |
| F.6 | Reprocessamento de e-mail específico via UI                                                                         | Sofia Almeida  | frontend |
| F.7 | MCP tool: `scan_gmail_label`, `scan_gmail_query`, `scan_gmail_period`                                               | Maria Oliveira | mcp      |

### Pasta Local — Plano Concreto

**O que já está implementado:**

- ✅ Scan de diretório
- ✅ Filtros por extensão (PDF, CSV, XLS, OFX, imagem)
- ✅ Upload para Storage
- ✅ Dedup por filename + size

**O que falta:**

| #    | Tarefa                                                                             | Responsável    | Tipo     |
| ---- | ---------------------------------------------------------------------------------- | -------------- | -------- |
| F.8  | Dedup por content_hash (mesmo que Gmail scanner) — substituir dedup por filename   | João Pereira   | backend  |
| F.9  | Watch mode: `--watch` para monitorar pasta continuamente                           | João Pereira   | backend  |
| F.10 | Upload manual via UI: drag & drop na página de ingestão                            | Roberto Lima   | frontend |
| F.11 | MCP tool: `scan_local_folder` já existe — adicionar `--recursive` e `--extensions` | Maria Oliveira | mcp      |

### Critérios de aceite — Gmail

- [ ] `bun run pipeline:passo01:scan-gmail -- --query "from:nubank"` funciona
- [ ] `bun run pipeline:passo01:scan-gmail -- --after 2026-01-01` funciona
- [ ] Rate limiting respeita quota do Gmail API
- [ ] Documentos do Gmail aparecem na UI de ingestão com origem "gmail"
- [ ] Reprocessamento funciona para e-mail específico

### Critérios de aceite — Pasta Local

- [ ] Dedup por content_hash funciona
- [ ] Upload manual via UI funciona
- [ ] Documentos locais aparecem na UI com origem "local_scan"
- [ ] Watch mode monitora pasta e processa novos arquivos automaticamente

---

## Fase G — Promoção e Intercâmbio Entre Ambientes

### Objetivo

Forma segura, rastreável e idempotente de mover dados entre local, staging e produção.

### Entidades promovíveis

| Entidade           | Pode promover | Observação            |
| ------------------ | ------------- | --------------------- |
| suppliers          | ✅            | Nome, aliases, tipo   |
| supplier_aliases   | ✅            | Vinculado ao supplier |
| categories         | ✅            | Árvore de categorias  |
| tags               | ✅            | Por usuário           |
| document_patterns  | ✅            | Regras de extração    |
| institutions       | ✅            | Bancos e instituições |
| financial_products | ✅            | Contas, cartões, etc. |

### Entidades NÃO promovíveis

| Entidade         | Por quê                                             |
| ---------------- | --------------------------------------------------- |
| transactions     | Dados financeiros reais — cada ambiente tem os seus |
| ingestion_jobs   | Específico do ambiente                              |
| source_documents | Específico do ambiente                              |
| draft_records    | Específico do ambiente                              |
| ai_chat_messages | Específico do ambiente                              |
| audit_logs       | Específico do ambiente                              |
| user_secrets     | Nunca sai do ambiente                               |

### Implementação: `scripts/promote.ts`

```bash
# Exemplo de uso
bun run scripts/promote.ts \
  --from local \
  --to staging \
  --scope suppliers,categories,document_patterns \
  --dry-run

# Output:
# DRY RUN — Nenhuma alteração será feita
#
# suppliers: 7 encontrados, 3 novos, 2 atualizados, 2 iguais
#   + Nubank (novo)
#   + C6 Bank (novo)
#   + CEMIG (novo)
#   ~ Caixa Econômica (campo 'type' diferente: 'bank' → 'public_bank')
#   ~ Banco do Brasil (campo 'aliases' +1 novo alias)
#   = Bradesco (igual)
#   = Itaú (igual)
#
# categories: 8 encontrados, 0 novos, 0 atualizados, 8 iguais
#
# document_patterns: 3 encontrados, 3 novos
#   + Fatura CEMIG v1 (novo)
#   + Fatura Nubank v1 (novo)
#   + Boleto Genérico v1 (novo)
#
# Total: 6 inserts, 2 updates, 10 skips
# Execute sem --dry-run para aplicar
```

### O que precisa ser feito

| #   | Tarefa                                                                                               | Responsável    | Tipo  |
| --- | ---------------------------------------------------------------------------------------------------- | -------------- | ----- |
| G.1 | Migration: tabela `promotion_logs` (ambiente_origem, ambiente_destino, escopo, resultado, timestamp) | André Santos   | db    |
| G.2 | Script `scripts/promote.ts` com argumentos `--from`, `--to`, `--scope`, `--dry-run`                  | Fernando Gomes | infra |
| G.3 | Lógica de diff por hash: comparar entidades entre ambientes                                          | Fernando Gomes | infra |
| G.4 | Lógica de merge: insert para novos, update para diferentes, skip para iguais                         | Fernando Gomes | infra |
| G.5 | Auditoria: registrar em `promotion_logs`                                                             | Fernando Gomes | infra |
| G.6 | MCP tool: `promote_to_staging`, `promote_to_production` (com dry-run)                                | Maria Oliveira | mcp   |
| G.7 | Documentação: instruções no guia do CEO                                                              | Fernando Gomes | docs  |

### Critérios de aceite

- [ ] `--dry-run` mostra preview sem alterar nada
- [ ] Promoção por escopo funciona (suppliers sozinho, patterns sozinho, etc.)
- [ ] Entidades iguais são ignoradas (idempotência)
- [ ] Entidades diferentes mostram diff antes de atualizar
- [ ] `promotion_logs` registra cada promoção com detalhes
- [ ] Entidades não-promovíveis são bloqueadas com erro claro
- [ ] Chat consegue executar promoção com confirmação humana

> **Nota (Correção #6 — Veronica):** Documentos (`source_documents`), drafts (`draft_records`), transações e logs NÃO são promovíveis entre ambientes. A alternativa é: (a) o CEO re-ingere os documentos em cada ambiente via upload manual ou Gmail scan, (b) padrões e fornecedores promovidos garantem que a re-ingestão gera resultados consistentes, (c) para "carregar" dados de referência em staging, usar seed SQL ou o próprio `promote.ts` com escopo restrito a entidades de catálogo.

---

# 4.8 Nota de Consistência de Modelagem (Correção #3 — Veronica)

As tabelas reais de ingestão no banco são:

| Nome real da tabela | Referência correta nos documentos                      |
| ------------------- | ------------------------------------------------------ |
| `ingestion_jobs`    | Jobs de ingestão (fila de processamento)               |
| `source_documents`  | Documentos-fonte (arquivo original + metadados)        |
| `draft_records`     | Rascunhos gerados pela extração (pendentes de revisão) |
| `draft_batches`     | Lotes de drafts para aprovação em batch                |

**Referências incorretas corrigidas neste documento:**

- ~~`ingested_documents`~~ → `source_documents` (**corrigido** também na ADR-006)
- ~~`job_id`~~ → `source_document_id` (na tabela `pattern_feedback`)
- ~~`document_id`~~ → `source_document_id` (na tabela `pattern_feedback` da ADR-006)

Toda menção futura deve usar os nomes reais acima.

---

# 4.9 Matriz de Decisão Documental (Correção #5 — Veronica)

Esta matriz define **que tipo de documento gera qual tipo de registro** após aprovação humana.

| Tipo de Documento                                             | Registro Gerado                           | Tabela Destino                                           | Observação                                                                                                |
| ------------------------------------------------------------- | ----------------------------------------- | -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| **Conta de consumo** (energia, água, gás, internet, telefone) | Transação (despesa)                       | `transactions`                                           | Uma transação por fatura. Pode gerar recorrência se padrão detectado                                      |
| **Boleto avulso**                                             | Transação (despesa)                       | `transactions`                                           | Transação única. Fornecedor associado                                                                     |
| **Fatura de cartão de crédito**                               | Registro de fatura + N itens              | `card_statements` + `card_statement_items`               | Fatura gera registro-pai + cada item vira um `card_statement_item`. Itens parcelados referenciam parcelas |
| **Extrato bancário**                                          | N transações                              | `transactions`                                           | Cada linha do extrato vira uma transação. Transferências internas detectadas e marcadas                   |
| **Comprovante de pagamento**                                  | Documento de apoio                        | `source_documents` (apenas)                              | Vinculado à transação existente via `reconciled_with`. Não gera novo registro financeiro                  |
| **Comprovante de transferência (PIX/TED/DOC)**                | Transferência interna                     | `transactions` (tipo `transfer`)                         | Gera UM registro de transferência entre contas próprias. NÃO é despesa                                    |
| **Boleto de empréstimo/financiamento**                        | Parcela de passivo                        | `liability_payments`                                     | Decompõe em: amortização, juros, seguros, taxas. Atualiza saldo devedor                                   |
| **Contrato de empréstimo**                                    | Passivo (dívida)                          | `liabilities`                                            | Cria registro de passivo com cronograma. Não gera transação imediata                                      |
| **Informe de rendimentos**                                    | Documento de apoio                        | `source_documents` (apenas)                              | Informação fiscal. Não gera registro financeiro                                                           |
| **Nota fiscal**                                               | Transação (despesa) OU documento de apoio | `transactions` OU apenas `source_documents`              | Se NF de compra: gera transação. Se NF de serviço já registrado: documento de apoio                       |
| **Recorrência detectada**                                     | Template recorrente                       | `recurring_templates`                                    | Quando 3+ documentos do mesmo fornecedor/valor/período são detectados, propor criação de recorrência      |
| **Documento desconhecido**                                    | Pendente de classificação                 | Nenhuma (fica como `source_documents` até classificação) | Aguarda revisão humana para definir tipo                                                                  |

### Regras da matriz

1. **Nunca gerar registro financeiro sem aprovação humana** (fases iniciais)
2. **Transferência interna ≠ despesa** — sempre classificar corretamente
3. **Pagamento de fatura ≠ nova despesa** — vincular à fatura existente
4. **Comprovante ≠ transação** — comprovante é evidência, não registro novo
5. **Na dúvida, documento de apoio** — melhor subclassificar do que gerar registro errado

---

# 5. Marcos de Staging — Resumo Visual

| Marco       | Fase | O que o CEO vê                        | Como testa                                                        |
| ----------- | ---- | ------------------------------------- | ----------------------------------------------------------------- |
| **Marco 1** | A    | Login funcional em URL pública        | Acessar staging, login Google, ver dashboard                      |
| **Marco 2** | B    | Documentos listados na UI de ingestão | Scan Gmail → ver documentos em `/dashboard/ingestion`             |
| **Marco 3** | B    | Split-view + edição + aprovação       | Abrir documento, editar campos, aprovar, ver em transactions      |
| **Marco 4** | C    | Chat funcional com upload e sugestões | Abrir drawer, arrastar PDF, receber sugestão, aprovar via chat    |
| **Marco 5** | D    | Padrões aprendidos e reutilizados     | Corrigir extração, ver padrão criado, processar documento similar |
| **Marco 6** | G    | Promoção segura local→staging         | Rodar `promote.ts --dry-run`, ver preview, executar               |

---

# 6. Proposta de Expansão do MCP

## 6.1 Ferramentas MCP Existentes (8)

| Tool                              | Status       |
| --------------------------------- | ------------ |
| `scan_local_folder`               | ✅ Funcional |
| `list_unparsed_documents`         | ✅ Funcional |
| `reprocess_document`              | ✅ Funcional |
| `resolve_supplier_candidates`     | ✅ Funcional |
| `list_draft_batches`              | ✅ Funcional |
| `approve_draft_batch`             | ✅ Funcional |
| `find_documents_without_password` | ✅ Funcional |
| `recompute_financial_periods`     | ✅ Funcional |

## 6.2 Ferramentas MCP Novas a Implementar

| Tool                         | Fase | Propósito                           | Responsável |
| ---------------------------- | ---- | ----------------------------------- | ----------- |
| `scan_gmail_label`           | F    | Scan Gmail por label                | Maria       |
| `scan_gmail_query`           | F    | Scan Gmail por query livre          | Maria       |
| `scan_gmail_period`          | F    | Scan Gmail por período              | Maria       |
| `get_document_details`       | B    | Detalhes de um documento específico | Maria       |
| `suggest_supplier`           | C    | Sugerir fornecedor para texto       | João        |
| `suggest_document_type`      | C    | Sugerir tipo de documento           | João        |
| `explain_classification`     | C    | Explicar por que classificou assim  | João        |
| `suggest_reconciliation`     | E    | Sugerir conciliação draft↔registro  | João        |
| `approve_document`           | C    | Aprovar draft individual            | Maria       |
| `reject_document`            | C    | Rejeitar draft com motivo           | Maria       |
| `reclassify_document`        | C    | Reclassificar tipo/fornecedor       | Maria       |
| `register_document_pattern`  | D    | Registrar novo padrão               | Maria       |
| `list_document_patterns`     | D    | Listar padrões existentes           | Maria       |
| `upload_and_ingest`          | C    | Upload de arquivo + pipeline        | João        |
| `promote_to_staging`         | G    | Promover entidades local→staging    | Maria       |
| `promote_to_production`      | G    | Promover entidades staging→prod     | Maria       |
| `search_transactions`        | C    | Buscar transações por query         | Maria       |
| `list_documents_with_errors` | C    | Listar documentos com erro          | Maria       |

**Total após expansão: 26 ferramentas MCP**

---

# 7. Bloqueadores do CEO

| #   | Ação                                                | Quando                      | Urgência      | Detalhes                                                                            |
| --- | --------------------------------------------------- | --------------------------- | ------------- | ----------------------------------------------------------------------------------- |
| 1   | Configurar Google OAuth no Supabase Staging         | Antes do Marco 1            | 🔴 BLOQUEADOR | Dashboard Supabase → Authentication → Providers → Google → Redirect URI             |
| 2   | Obter API key OpenAI                                | Antes do Marco 4            | 🔴 BLOQUEADOR | platform.openai.com → API Keys → gerar key, colocar em `.env` como `OPENAI_API_KEY` |
| 3   | Configurar `OPENAI_API_KEY` no Vercel               | Antes do Marco 4 em staging | 🔴 BLOQUEADOR | Vercel Dashboard → Settings → Environment Variables                                 |
| 4   | Testar login em staging após deploy                 | Marco 1                     | 🟠 VALIDAÇÃO  | Acessar URL, login Google, confirmar dados                                          |
| 5   | Testar Gmail scan com volume real                   | Marco 2                     | 🟠 VALIDAÇÃO  | `bun run pipeline:passo01:scan-gmail -- --limit 100`                                |
| 6   | Validar cada documento processado nos primeiros 20  | Marco 3                     | 🟠 VALIDAÇÃO  | Abrir `/dashboard/ingestion/documents`, revisar 20 documentos manualmente           |
| 7   | Configurar Google OAuth no Supabase Production      | Antes de ir para produção   | 🟡 FUTURO     | Mesma config que staging, com redirect URI de produção                              |
| 8   | Decidir threshold de autopost ou manter tudo manual | Após Marco 5                | 🟡 FUTURO     | Reunião de revisão após 100 documentos aprovados                                    |

---

# 8. Itens Fora de Escopo Agora

| Item                                    | Por quê                                    |
| --------------------------------------- | ------------------------------------------ |
| Mobile (React Native/Expo)              | Depende de MVP web completo                |
| Consultoria financeira ampla via chat   | Foco é ingestão, não planejamento          |
| Multiagentes sofisticados               | Complexidade desnecessária agora           |
| Open Banking / integração com bancos    | Regulatório + complexidade                 |
| OCR avançado (handwriting, fotos ruins) | OpenAI Vision cobre 90% dos casos          |
| Webhook do Gmail (push)                 | Poll resolve para o volume atual           |
| Voice input/output no chat              | Nice-to-have, não essencial                |
| Autopost sem revisão humana             | Precisa de histórico de acertos primeiro   |
| Dashboards analíticos avançados         | Primeiro precisa ter dados entrando        |
| A11y audit completo                     | Importante, mas não bloqueia uso real      |
| Testes visuais automatizados            | Nice-to-have                               |
| i18n/l10n                               | Sistema é em português, público brasileiro |

---

# 9. Backlog Priorizado de Implementação

## Prioridade P0 — BLOQUEADOR (Marco 1)

| ID  | Tarefa                                                              | Tipo   | Dep.    | Responsável | Marco |
| --- | ------------------------------------------------------------------- | ------ | ------- | ----------- | ----- |
| 001 | Deploy web real: substituir placeholder por Vercel CLI no GitLab CI | infra  | —       | Fernando    | 1     |
| 002 | Configurar env vars no Vercel (SUPABASE_URL, ANON_KEY, SERVICE_KEY) | infra  | 001     | Fernando    | 1     |
| 003 | **CEO:** Config Google OAuth no Supabase Staging                    | manual | —       | CEO         | 1     |
| 004 | Config domínio customizado no Vercel                                | infra  | 001     | Fernando    | 1     |
| 005 | Verificar 19 migrations aplicadas em staging                        | db     | —       | André       | 1     |
| 006 | Testar login Google em staging                                      | qa     | 001,003 | Renata      | 1     |

**Critério de aceite:** CEO faz login em staging e vê dashboard.

## Prioridade P0 — BLOQUEADOR (Marco 2)

| ID  | Tarefa                                                       | Tipo     | Dep.    | Responsável | Marco |
| --- | ------------------------------------------------------------ | -------- | ------- | ----------- | ----- |
| 007 | Server actions ingestão: listDocuments, getDocument          | backend  | —       | Sofia       | 2     |
| 008 | Server actions ingestão: listDrafts, getDraft                | backend  | —       | Sofia       | 2     |
| 009 | Rota `/dashboard/ingestion` com contadores                   | frontend | 007     | Roberto     | 2     |
| 010 | Rota `/dashboard/ingestion/documents` com listagem + filtros | frontend | 007     | Roberto     | 2     |
| 011 | Componente `<StatusBadge />` para 24 estados                 | frontend | —       | Roberto     | 2     |
| 012 | Componente `<ConfidenceIndicator />`                         | frontend | —       | Roberto     | 2     |
| 013 | Empty/loading states para páginas de ingestão                | frontend | 009,010 | Roberto     | 2     |

**Critério de aceite:** Documentos ingeridos aparecem listados com status e filtros.

## Prioridade P0 — BLOQUEADOR (Marco 3)

| ID   | Tarefa                                                                 | Tipo     | Dep.    | Responsável  | Marco |
| ---- | ---------------------------------------------------------------------- | -------- | ------- | ------------ | ----- |
| 014  | Rota `/dashboard/ingestion/documents/[id]` com split-view              | frontend | 007     | Roberto      | 3     |
| 015  | Componente `<PDFViewer />` inline (react-pdf ou iframe)                | frontend | —       | Roberto      | 3     |
| 016  | Componente `<DraftReviewForm />` com edição de campos                  | frontend | 008     | Sofia        | 3     |
| 017  | Componente `<DraftApprovalActions />` com aprovar/rejeitar/reprocessar | frontend | 008     | Sofia        | 3     |
| 018  | Server action: approveDraft (gravar no ledger + update status)         | backend  | —       | Sofia + João | 3     |
| 019  | Server action: rejectDraft (marcar rejeitado + motivo)                 | backend  | —       | Sofia        | 3     |
| 020  | Server action: reprocessDocument (resetar para QUEUED)                 | backend  | —       | João         | 3     |
| 021  | Batch approval: selecionar + aprovar múltiplos drafts                  | frontend | 018     | Sofia        | 3     |
| 022  | Indicadores visuais: erro, pendência, falta de senha, baixa confiança  | frontend | 011,012 | Roberto      | 3     |
| 022b | **Upload manual via UI: drag & drop na página de ingestão**            | frontend | 007     | Roberto      | 3     |

**Critério de aceite:** CEO abre documento, vê PDF + dados, edita, aprova, e registro aparece em transactions. CEO consegue fazer upload manual de PDF pela UI sem depender de worker local.

## Prioridade P1 — ALTO (Marco 4)

| ID  | Tarefa                                                                               | Tipo     | Dep. | Responsável   | Marco |
| --- | ------------------------------------------------------------------------------------ | -------- | ---- | ------------- | ----- |
| 023 | **CEO:** Obter API key OpenAI e colocar no `.env`                                    | manual   | —    | CEO           | 4     |
| 024 | Migration: tabelas ai_chat_sessions e ai_chat_messages com RLS                       | db       | —    | André         | 4     |
| 025 | API Route `/api/chat` com auth, rate limiting, streaming                             | backend  | 024  | João          | 4     |
| 026 | System prompt do SBF (contexto financeiro, personalidade)                            | backend  | —    | João + Camila | 4     |
| 027 | Function calling tools (15 tools): list_pending, get_details, suggest, approve, etc. | backend  | 025  | João + Maria  | 4     |
| 028 | Componente `<AIChatDrawer />` com Vercel AI SDK useChat                              | frontend | 025  | Roberto       | 4     |
| 029 | Upload de arquivo no chat (drag & drop)                                              | frontend | 028  | Roberto       | 4     |
| 030 | Renderização de mensagens: markdown, tabelas, badges                                 | frontend | 028  | Roberto       | 4     |
| 031 | Parser OpenAI: fallback quando regex falha                                           | backend  | 023  | João          | 4     |
| 032 | OpenAI Vision: análise de imagem de documento                                        | backend  | 023  | João          | 4     |
| 033 | Auditoria: log toda interação com IA                                                 | backend  | 024  | Maria         | 4     |
| 034 | Config `OPENAI_API_KEY` no Vercel env vars                                           | infra    | 023  | Fernando      | 4     |

**Critério de aceite:** CEO abre drawer de chat, arrasta PDF, recebe sugestão, aprova via chat.

## Prioridade P1 — ALTO (Marco 5)

| ID  | Tarefa                                                                | Tipo     | Dep.    | Responsável    | Marco |
| --- | --------------------------------------------------------------------- | -------- | ------- | -------------- | ----- |
| 035 | Migration: tabelas document_patterns e pattern_feedback com RLS       | db       | —       | André          | 5     |
| 036 | Server actions: CRUD patterns, listagem, feedback                     | backend  | 035     | Sofia          | 5     |
| 037 | Rota `/dashboard/ingestion/patterns` — lista + detalhe + edição       | frontend | 036     | Roberto        | 5     |
| 038 | Integrar padrões no ingestion worker                                  | backend  | 035     | João           | 5     |
| 039 | Auto-desativação de padrões ruins (feedback_count > 3, success < 50%) | backend  | 035     | Maria          | 5     |
| 040 | Tools de chat: list/register/update patterns                          | backend  | 035,027 | João           | 5     |
| 041 | Seed: 3-5 padrões iniciais (CEMIG, boleto, fatura cartão)             | backend  | 035     | João + Ricardo | 5     |

**Critério de aceite:** CEO corrige extração, padrão é criado, próximo documento similar usa o padrão.

## Prioridade P1 — ALTO (Marco 5 — Reconciliação)

| ID  | Tarefa                                                                     | Tipo     | Dep.    | Responsável | Marco |
| --- | -------------------------------------------------------------------------- | -------- | ------- | ----------- | ----- |
| 042 | Módulo reconciliation.ts: heurísticas determinísticas                      | backend  | —       | Maria       | 5     |
| 043 | Migration: colunas reconciliation_status, reconciled_with em draft_records | db       | —       | André       | 5     |
| 044 | Integrar reconciliação no ingestion worker (após draft generation)         | backend  | 042     | João        | 5     |
| 045 | API Route: buscar candidatos de conciliação para draft_id                  | backend  | 042     | João        | 5     |
| 046 | Componente `<ReconciliationPanel />` no detalhe do documento               | frontend | 045     | Roberto     | 5     |
| 047 | Tool IA: suggest_reconciliation                                            | backend  | 042,027 | João        | 5     |
| 048 | Testes: cenários match, não-match, duplicado, recorrência                  | qa       | 042     | Renata      | 5     |

**Critério de aceite:** Draft mostra candidatos de conciliação com score, CEO vincula ou ignora.

## Prioridade P2 — MÉDIO (Marco 5 — Gmail avançado)

| ID      | Tarefa                                                           | Tipo         | Dep.    | Responsável | Marco |
| ------- | ---------------------------------------------------------------- | ------------ | ------- | ----------- | ----- | ----------------------------------- |
| 049     | Gmail scan por query livre (--query)                             | worker       | —       | João        | 5     |
| 050     | Gmail scan por período (--after, --before)                       | worker       | —       | João        | 5     |
| 051     | Rate limiting Gmail API                                          | worker       | —       | João        | 5     |
| 052     | Progresso do scan na UI                                          | frontend     | 010     | Roberto     | 5     |
| 053     | Local scanner: dedup por content_hash                            | worker       | —       | João        | 5     |
| ~~054~~ | ~~Upload manual via UI: drag & drop na ingestão~~                | ~~frontend~~ | ~~007~~ | ~~Roberto~~ | ~~5~~ | **Movido para Marco 3 (task 022b)** |
| 055     | MCP tools: scan_gmail_label, scan_gmail_query, scan_gmail_period | mcp          | 049,050 | Maria       | 5     |

**Critério de aceite:** Gmail scan por query/período funciona, upload manual funciona.

## Prioridade P2 — MÉDIO (Marco 6)

| ID  | Tarefa                                                 | Tipo  | Dep. | Responsável | Marco |
| --- | ------------------------------------------------------ | ----- | ---- | ----------- | ----- |
| 056 | Migration: tabela promotion_logs                       | db    | —    | André       | 6     |
| 057 | Script promote.ts com --from, --to, --scope, --dry-run | infra | 056  | Fernando    | 6     |
| 058 | Lógica de diff por hash entre ambientes                | infra | 057  | Fernando    | 6     |
| 059 | Lógica de merge: insert/update/skip                    | infra | 058  | Fernando    | 6     |
| 060 | Auditoria em promotion_logs                            | infra | 059  | Fernando    | 6     |
| 061 | MCP tools: promote_to_staging, promote_to_production   | mcp   | 057  | Maria       | 6     |

**Critério de aceite:** `promote.ts --dry-run` mostra preview correto, execução real funciona.

## Prioridade P3 — Após Marcos 1-6

| ID      | Tarefa                                                                            | Tipo            | Dep.  | Responsável         | Marco     |
| ------- | --------------------------------------------------------------------------------- | --------------- | ----- | ------------------- | --------- | ----------------------------------------- |
| 062     | Dashboard operacional ("primeira tela"): vencimentos + essenciais vs postergáveis | frontend        | —     | Roberto             | pós-6     |
| ~~063~~ | ~~Logging/observabilidade: tabela system_logs + página /dashboard/logs~~          | ~~db+frontend~~ | ~~—~~ | ~~André + Roberto~~ | ~~pós-6~~ | **Movido para Marco 1 (tasks A.8 + A.9)** |
| 064     | A11y audit nas páginas de ingestão                                                | qa              | —     | Renata              | pós-6     |
| 065     | Testes E2E com volume real (1000+ docs)                                           | qa              | —     | Renata + João       | pós-6     |
| 066     | Gráficos e visualizações em relatórios                                            | frontend        | —     | Roberto             | pós-6     |
| 067     | Merge visual de fornecedores                                                      | frontend        | —     | Sofia               | pós-6     |
| 068     | Cronograma visual de amortização                                                  | frontend        | —     | Roberto             | pós-6     |

---

# 10. Discussão da Equipe

## Ana Silva (Arquiteta)

O plano está focado e incremental. Cada marco entrega valor testável. A decisão de usar Vercel AI SDK + Edge Function como proxy é a mais segura e performática. A IA nunca grava direto no ledger — isso é inegociável.

## Ricardo Monteiro (Economista)

A reconciliação precisa ser rigorosa. Uma duplicação pode distorcer todo o controle financeiro. O threshold de confiança de 80% para sugestão e 98% para autopost futuro me parece adequado. Sugiro que nas primeiras 2 semanas tudo seja revisão manual, mesmo com alta confiança.

## Camila Duarte (Consultora)

Finalmente um plano concreto! O CEO precisa ver resultados rápido. Marco 1 (staging) e Marco 2 (ingestão visível) são urgentes. Se em 1 semana ele conseguir ver documentos na tela, já muda a percepção. O chat é o diferencial — mas só faz sentido se a base (fases A e B) estiver sólida.

## João Pereira (Backend)

Workers estão 95% prontos. A integração OpenAI é a parte mais complexa — o function calling com 15+ tools precisa de testes cuidadosos. Sugiro implementar as tools incrementalmente: primeiro as de listagem (simples), depois as de ação (aprovar/rejeitar), por último as de análise (reconciliação).

## Maria Oliveira (Backend/Segurança)

Rate limiting no chat é obrigatório. Além de 10 msgs/min, sugiro também limite de tokens por sessão (não gastar US$50 em uma conversa). Audit log de toda interação com OpenAI é mandatório. API key nunca no frontend — sempre via Edge Function.

## Roberto Lima (Frontend)

A UI de ingestão é a prioridade 1. Vou começar pelas páginas de listagem (mais simples), depois split-view (mais complexo). O PDF viewer vai usar `react-pdf` — já usei em outros projetos, funciona bem com Next.js. O chat drawer será baseado no padrão que o shadcn/ui usa para sheets.

## Sofia Almeida (Frontend)

Server actions de ingestão seguem o mesmo padrão das outras (institutions, products). A lógica de aprovação é o ponto mais sensível — precisa gravar no ledger E atualizar o status do draft atomicamente.

## Fernando Gomes (DevOps)

Deploy na Vercel é simples — já tenho a config. O promote.ts é a parte mais delicada: preciso conectar em dois bancos simultaneamente e comparar. Vou usar Supabase JS client para ambos, com credenciais passadas via argumento (nunca hardcoded).

## André Santos (DBA)

Migrations novas: ai_chat_sessions, ai_chat_messages, document_patterns, pattern_feedback, promotion_logs. São 5 migrations com RLS em todas. Vou garantir que todas são idempotentes.

## Renata Silva (QA)

Vou criar testes de integração para: aprovação de draft → registro no ledger, rejeição, reprocessamento, reconciliação. Critérios de aceite claros para cada marco — finalmente!

---

# 11. Decisão Final

**O plano de 6 marcos é aprovado por consenso.**

**Ordem de execução:**

1. **Marco 1** (Fase A) — Staging operacional — IMEDIATO
2. **Marco 2** (Fase B início) — Ingestão visível — Sequencial a Marco 1
3. **Marco 3** (Fase B completa) — Revisão humana — Sequencial a Marco 2
4. **Marco 4** (Fase C) — IA acoplada — Paralelo parcial com Marco 3
5. **Marco 5** (Fases D + E + F) — Padrões + reconciliação — Sequencial a Marco 4
6. **Marco 6** (Fase G) — Promoção entre ambientes — Sequencial a Marco 5

**Bloqueadores imediatos do CEO:**

1. 🔴 Configurar Google OAuth no Supabase Staging (AGORA)
2. 🔴 Obter API key OpenAI (antes do Marco 4)

---

# 12. Ações / Responsáveis / Prazo

| Ação                                     | Responsável            | Prazo         |
| ---------------------------------------- | ---------------------- | ------------- |
| Deploy Vercel real (tasks 001-006)       | Fernando               | Marco 1       |
| CEO configura OAuth staging              | CEO                    | Marco 1       |
| Server actions ingestão (tasks 007-008)  | Sofia                  | Marco 2       |
| UI ingestão listagem (tasks 009-013)     | Roberto                | Marco 2       |
| UI ingestão split-view (tasks 014-022)   | Roberto + Sofia        | Marco 3       |
| CEO obtém API key OpenAI                 | CEO                    | Antes Marco 4 |
| Integração OpenAI chat (tasks 024-034)   | João + Roberto         | Marco 4       |
| Padrões documentais (tasks 035-041)      | André + Sofia + João   | Marco 5       |
| Reconciliação (tasks 042-048)            | Maria + Roberto + João | Marco 5       |
| Gmail avançado (tasks 049-055)           | João + Roberto + Maria | Marco 5       |
| Promoção entre ambientes (tasks 056-061) | Fernando + Maria       | Marco 6       |
