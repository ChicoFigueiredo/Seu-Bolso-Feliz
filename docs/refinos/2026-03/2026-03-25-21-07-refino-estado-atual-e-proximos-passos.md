---
Título da Reunião: Refino — Estado Atual, Gaps, Testes, Deploy e Integração OpenAI
Data e Hora: 2026-03-25 21:07
Participantes:
  - CEO (Chico) — facilitador e decisor final
  - Ana Silva (Arquiteta) — visão estrutural e sequenciamento
  - João Pereira (Backend Bun) — workers, MCP, Gmail
  - Maria Oliveira (Backend Bun) — testes, segurança, idempotência
  - Pedro Santos (Backend Python) — parsing, IA futura
  - Roberto Lima (Frontend) — UI ingestão, dashboard
  - Ricardo Monteiro (Economista) — domínio financeiro
  - Fernando Gomes (DevOps) — CI/CD, deploy, ambientes
  - Camila Duarte (Finanças Pessoais) — experiência do usuário financeiro
Pauta:
  - O que falta?
  - O que é possível testar agora?
  - O que pode subir (deploy)?
  - Como o CEO vai interagir com os resultados?
  - Como vai entrar a OpenAI?
---

# Refino — Estado Atual e Próximos Passos

## 1. Contexto

O projeto **Seu Bolso Feliz** está com o MVP web funcional (26 rotas), backend completo no Supabase (36 tabelas + 17 migrations + 3 Edge Functions), Google Auth funcionando localmente, site publicado em https://seubolsofeliz.com.br/, pipeline de ingestão documental com workers implementados (local-scanner, gmail-scanner, ingestion), parsers CEMIG e boleto operacionais, e MCP Server com 8 tools para o VS Code.

O Gmail Scanner acabou de ser testado com sucesso em dry-run (Label "Comprovantes" encontrada, attachments detectados, paginação funcionando). O objetivo deste refino é mapear o que falta, o que pode ser validado agora e como evoluir para a integração com IA.

### Números atuais

| Dimensão             | Contagem                                    |
| -------------------- | ------------------------------------------- |
| Migrations aplicadas | 17                                          |
| Tabelas no banco     | 36 + 9 ingestão = 45                        |
| Edge Functions       | 3 (operacionais staging + prod)             |
| Workers              | 3 (ingestion, local-scanner, gmail-scanner) |
| MCP Tools            | 8 operacionais                              |
| Rotas web            | 26                                          |
| Testes passando      | ~328 (168 core + 25 gmail + parsers)        |
| Parsers              | 2 (CEMIG, boleto genérico)                  |

---

## 2. Discussão: O que falta?

### Ana Silva (Arquiteta)

O projeto está numa posição sólida. Os principais gaps se dividem em três categorias:

1. **Operacional/DevOps** — O deploy real pelo CI/CD ainda é placeholder. O site está no Vercel, mas o pipeline do GitLab usa `echo` no job de deploy. Isso significa que push para staging/produção não é automático pelo CI.

2. **Pipeline de ingestão** — O scanner de Gmail funciona, mas o fluxo completo (scan → download → parse → draft → revisão → aprovação) ainda não foi testado end-to-end com dados reais. Faltam:
   - Tool MCP `scan_gmail_label` (integrar o scanner ao MCP)
   - Scan por período/query livre (backlog)
   - UI de ingestão (6 rotas inteiras)

3. **IA** — Toda a Fase 7 (Agentes OpenAI) está bloqueada/não iniciada.

### João Pereira (Backend Bun)

Do ponto de vista dos workers e MCP:

**Itens pendentes imediatos:**

- `scan_gmail_label` tool no MCP — é só integrar a função `scanGmailLabel()` que já existe. Trabalho de 30 minutos.
- `scan_gmail_period` — depende de implementar filtro por data no Gmail client (query `after:YYYY/MM/DD before:YYYY/MM/DD`). Trabalho de ~2h.
- Teste de integração do dry-run (mock Supabase) — 3.24 no checklist.
- Documentação do MCP com exemplos reais — 5.14.
- Teste de idempotência das tools MCP — 5.15.

**O fluxo real hoje:**

1. CEO roda `bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 10` → scanner baixa anexos, cria jobs
2. CEO roda `bun run workers/ingestion/src/index.ts` → worker processa jobs (download → hash → parse → draft)
3. Via MCP no VS Code: `list_draft_batches` → `approve_draft_batch` → dados vão para o ledger

### Maria Oliveira (Backend Bun)

**Testes pendentes:**

- 3.24: dry-run não grava nada (teste integração com mock)
- 3.25: backfill por período (depende de 3.12)
- 4.15: PDF com senha processado com segredo (precisa de PDF protegido real)
- 5.15: idempotência das tools MCP
- Teste e2e real do fluxo scan → parse → draft → approve

**Testes que já podem rodar:**

- **Todos os 328 existentes** continuam verdes
- O scanner pode ser testado com dados reais agora (1000+ emails na label)

### Pedro Santos (Backend Python/Parsing)

Do lado de parsing:

- Os parsers CEMIG e boleto estão funcionando com 47 testes (35 CEMIG + 12 boleto)
- O `parse-orchestrator` integra: extração de texto → parser determinístico → draft generation → output
- A sugestão de período financeiro (4.14) está parcial — a `competence_date` é propagada mas não integra com `@sbf/domain` para resolver o período

**Gap para IA:**

- Todo o módulo `packages/operations/src/ai/` precisa ser criado
- Precisa de API key OpenAI
- Os agentes (Document Parser, Supplier Resolver, Financial Classifier, Reconciler) são melhorias sobre os parsers determinísticos existentes

### Roberto Lima (Frontend)

A UI de ingestão (Fase 6) é um bloco completo de 15 itens não iniciados. Inclui:

- 7 rotas novas (`/dashboard/ingestion/*`)
- PDF viewer inline
- Formulário de revisão de draft (split-view: original × draft)
- Aprovação em lote
- Atalhos de teclado
- Filtros por fornecedor/categoria/tag/período/status

Essa é a interface que vai transformar drafts em registros financeiros reais.

### Fernando Gomes (DevOps)

**Deploy:**

- Site está live no Vercel (deploy manual/automático via Vercel dashboard)
- O pipeline GitLab tem 6 stages funcionais: validate → install → check → test → build → deploy
- O job `deploy-web-real` é placeholder (`echo`) — precisa integrar Vercel CLI
- Staging e Production do Supabase estão com migrations e Edge Functions deployed
- Google Auth funciona localmente, staging pendente de teste e2e

**O que pode subir agora:**

- Qualquer push para `feat/001-criacao-geral` já faz build e test no CI
- O Vercel deploya automaticamente a partir do push (conectado ao GitLab)
- As migrations e Edge Functions já estão em staging e produção

### Ricardo Monteiro (Economista)

A base de cálculos financeiros está sólida:

- Amortização SAC/Price/Misto com 33 testes
- Ciclos financeiros personalizados com 17 testes
- Priorização com 19 testes
- Deduplicação ADR-001 com 14 testes

O que falta do lado financeiro é **dados reais**. A importação de planilhas funciona via CSV, mas o CEO ainda não importou o histórico real dele. Sem dados reais, os relatórios e dashboards são teóricos.

### Camila Duarte (Finanças Pessoais)

O MVP financeiro está pronto para uso real. As funcionalidades essenciais estão lá:

- Cadastro de bancos/produtos
- Transações com categorias/tags/prioridade
- Recorrências
- Faturas
- Dívidas com amortização
- Relatórios por período

O que vai fazer diferença real na vida do CEO é **começar a usar** — cadastrar seus bancos, importar o histórico, classificar as despesas. A ingestão via Gmail é um acelerador, mas não precisa estar 100% para começar.

---

## 3. Discussão: O que é possível testar agora?

### Testes automatizados (todos passando)

| Suite                                                       | Quantidade | Status |
| ----------------------------------------------------------- | ---------- | ------ |
| Unitários domínio                                           | ~127       | ✅     |
| Unitários parsing (CEMIG/boleto)                            | ~47        | ✅     |
| Unitários Gmail (client + processor)                        | 25         | ✅     |
| Unitários ingestão (hash/idempotência/state-machine/drafts) | ~90        | ✅     |
| Integração                                                  | 16         | ✅     |
| E2E                                                         | 10+        | ✅     |

### Testes manuais que o CEO pode fazer AGORA

1. **Gmail Scanner real** (já testou dry-run com sucesso):

   ```bash
   # Scan real: baixa anexos, cria jobs
   bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 5
   ```

2. **Worker de ingestão** (processa os jobs criados pelo scanner):

   ```bash
   bun run workers/ingestion/src/index.ts
   ```

3. **MCP no VS Code** — pedir ao Copilot:
   - "Liste os documentos não processados" → `list_unparsed_documents`
   - "Liste os rascunhos pendentes" → `list_draft_batches`
   - "Aprove o batch X" → `approve_draft_batch`

4. **Scanner local**:

   ```bash
   # Cria pasta inbox e coloca PDFs lá
   mkdir -p inbox
   cp ~/Downloads/comprovante.pdf inbox/
   bun run workers/local-scanner/src/index.ts
   ```

5. **Web app** — login local com Google Auth:
   ```bash
   bun run dev
   # Acessar http://localhost:3000
   ```

---

## 4. Discussão: O que pode subir (deploy)?

### Fernando Gomes (DevOps)

**Já está deployed:**

- ✅ Site https://seubolsofeliz.com.br/ (Vercel)
- ✅ Supabase staging (17 migrations, 3 Edge Functions, RLS)
- ✅ Supabase production (17 migrations, 3 Edge Functions, RLS)
- ✅ Pipeline GitLab (6 stages, autorun em push)

**O que pode subir imediatamente:**

- Todo código já commitado sobe automaticamente via Vercel que monitora o repo
- Mudanças de banco: `supabase db push --linked` para staging/prod
- Edge Functions: `supabase functions deploy` para staging/prod

**O que falta para deploy completo via CI:**

- Job `deploy-web-real` no GitLab CI integrado com Vercel CLI (item 1.22)
- Validação do pipeline completo staging (1.23)
- Política de promoção staging → production (1.25)

### Ana Silva (Arquiteta)

Recomendo subir em duas ondas:

1. **Onda 1 (agora)**: Push do código do Gmail scanner + fix de .env.local. O Vercel deploya o frontend automaticamente. O scanner roda local.
2. **Onda 2 (pós-validação)**: Quando o CEO testar o fluxo completo (scan → parse → draft → approve) e confirmar que funciona, criar a tool MCP `scan_gmail_label` e subir.

---

## 5. Discussão: Como o CEO vai interagir com os resultados?

### João Pereira (Backend Bun)

Existem **3 canais de interação**:

#### Canal 1: CLI (Workers)

```bash
# Scan Gmail
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 10

# Processar fila de ingestão
bun run workers/ingestion/src/index.ts

# Scan local
bun run workers/local-scanner/src/index.ts
```

**Resultado**: Jobs criados no banco, documentos no Storage, logs no console.

#### Canal 2: MCP via VS Code (GitHub Copilot)

O CEO abre o VS Code e conversa com o Copilot:

- _"Quais documentos ainda não foram processados?"_ → tool `list_unparsed_documents`
- _"Quais rascunhos estão esperando aprovação?"_ → tool `list_draft_batches`
- _"Aprova o batch 123, mas rejeita o draft 456"_ → tool `approve_draft_batch`
- _"Escaneia a pasta ~/Downloads"_ → tool `scan_local_folder`
- _"Busca fornecedor CEMIG"_ → tool `resolve_supplier_candidates`
- _"Tem PDFs que falharam por senha?"_ → tool `find_documents_without_password`
- _"Recalcula meus períodos financeiros a partir do dia 20"_ → tool `recompute_financial_periods`
- _"Reprocessa o documento X"_ → tool `reprocess_document`

**Resultado**: Copilot executa a tool e apresenta o retorno formatado em chat.

#### Canal 3: Web App (Dashboard)

- Login em http://localhost:3000 (ou produção)
- Dashboard com visão operacional: vencimentos, prioridades, fila
- CRUD completo de todas entidades
- Relatórios por período/fornecedor/categoria

**Fase futura (Fase 6)**: UI de ingestão no dashboard (revisar drafts, aprovar lotes, visualizar PDFs inline).

### Camila Duarte (Finanças Pessoais)

Na prática do dia-a-dia do CEO:

1. **Semanal**: Rodar o Gmail scanner para baixar comprovantes novos
2. **Após scan**: Worker processa → parsers extraem dados → drafts criados
3. **Revisão**: Via MCP no VS Code (rápido) ou futuramente via UI web
4. **Aprovação**: Dados validados viram transações, recorrências, métricas reais
5. **Dashboard**: Acompanhar saldo, vencimentos, prioridades, relatórios

O canal mais poderoso a curto prazo é o **MCP no VS Code** — o CEO já está no VS Code o tempo todo, e pode pedir pro Copilot fazer as operações sem sair do editor.

---

## 6. Discussão: Como vai entrar a OpenAI?

### Pedro Santos (Backend Python/Parsing)

A integração com OpenAI está planejada na **Fase 7** do checklist 003. A arquitetura foi desenhada desde o início para separar o núcleo determinístico da camada de IA.

#### Onde a IA entra

| Agente                   | O que faz                                                                                                 | Substitui/melhora                                 |
| ------------------------ | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| **Document Parser**      | Lê PDFs genéricos que os parsers determinísticos não conseguem. Extrai dados estruturados via GPT-4o/4.1. | Fallback quando CEMIG/boleto parser não reconhece |
| **Supplier Resolver**    | Identifica fornecedor a partir de texto livre (nome fantasia, CNPJ, contexto).                            | Melhora `resolve_supplier_candidates` com NLU     |
| **Financial Classifier** | Classifica evento tipo (EXPENSE/INCOME/TRANSFER), categoria, tags, prioridade sugerida.                   | Melhora `suggestCategory`/`suggestTags`           |
| **Reconciler**           | Cruza lançamentos importados com extratos bancários. Identifica duplicatas prováveis.                     | Novo — não existe equivalente determinístico      |

#### Arquitetura da integração

```
PDF/Comprovante
     │
     ▼
[Parser determinístico]  ←── CEMIG, boleto, CSV (alta confiança)
     │
     ├─ confiança ≥ 0.7 → draft direto (revisão rápida)
     │
     └─ confiança < 0.7 → [Agente OpenAI]
                                │
                                ├─ Document Parser (extrai dados)
                                ├─ Supplier Resolver (identifica fornecedor)
                                ├─ Financial Classifier (classifica)
                                │
                                ▼
                          draft com score IA (revisão humana obrigatória)
```

#### Princípios obrigatórios

1. **IA nunca grava direto no ledger** — sempre gera draft que precisa de aprovação
2. **Chamadas IA são auditáveis** — prompt, response, tokens, custo logados em `ingestion_logs`
3. **Fallback determinístico** — se IA falha, o parser determinístico continua funcionando
4. **Custo controlado** — fila batch, não real-time; tokens limitados por chamada

#### Pré-requisitos (bloqueantes)

1. **API key OpenAI** — CEO precisa configurar conta e obter key
2. **Adicionar `OPENAI_API_KEY` ao `.env.example` e `.env`**
3. **Definir system prompts** — calibrar com dados reais do CEO (comprovantes CEMIG, boletos, recibos)
4. **Budget alerta** — configurar limite de gasto mensal na OpenAI

### Ana Silva (Arquiteta)

A sequência recomendada para IA:

1. **Primeiro**: CEO testa o fluxo completo sem IA (scan → parse → draft → approve) com os parsers determinísticos
2. **Segundo**: Identificar quais documentos os parsers não conseguem processar (confiança baixa)
3. **Terceiro**: Configurar o agente Document Parser para esses casos
4. **Quarto**: Gradualmente adicionar Supplier Resolver e Financial Classifier
5. **Quinto**: Reconciler (mais complexo, exige dados bancários importados)

Não recomendo ativar IA antes de ter dados reais no sistema. Os system prompts precisam ser calibrados com exemplos reais de comprovantes do CEO.

### Ricardo Monteiro (Economista)

A IA vai agregar mais valor na **classificação** do que na **extração**. Os parsers determinísticos (CEMIG, boleto) são excelentes para documentos padronizados. A IA vai brilhar em:

- Comprovantes de transferência PIX (texto livre)
- Recibos de pagamento genéricos
- Classificar se uma despesa é "essencial" ou "postergável"
- Sugerir se um pagamento é recorrente
- Resumo mensal em linguagem natural

---

## 7. Prós e Contras

### Opção A: Focar em IA agora

**Prós:**

- Valor percebido alto (interação inteligente)
- Diferencial competitivo
- CEO curioso para testar

**Contras:**

- Sem dados reais, system prompts serão genéricos e fracos
- Custo recorrente (API tokens)
- Mais complexidade antes de validar o fluxo base
- Pipeline de ingestão não foi testado end-to-end com dados reais

### Opção B: Validar fluxo completo primeiro, IA depois (RECOMENDADO)

**Prós:**

- Garante que a base funciona antes de adicionar complexidade
- Gera dados reais para calibrar IA
- Identifica gaps reais (quais docs os parsers não pegam)
- Menor risco e custo

**Contras:**

- IA demora mais para chegar
- CEO precisa ter paciência com revisão manual

### Opção C: Implementar UI de ingestão primeiro

**Prós:**

- Dashboard visual para revisar drafts
- Experiência mais amigável que o MCP/CLI
- Fase 6 completa valoriza o produto

**Contras:**

- 15 itens, estimativa de 1-2 sprints
- MCP já oferece funcionalidade de revisão (via conversação)
- Pode atrasar validação do fluxo real

---

## 8. Decisão Final

**CEO decide pela Opção B com elementos da C:**

### Sequência aprovada:

1. **Imediato (esta semana)**:
   - ✅ Fix .env.local (feito)
   - Testar fluxo completo real: Gmail scan → parse → draft → approve via MCP
   - Criar tool MCP `scan_gmail_label`
   - Push do código do Gmail scanner

2. **Sprint curto (próximas 2 semanas)**:
   - Deploy real via CI/CD (Vercel CLI no GitLab)
   - Google Auth no staging
   - Cadastrar dados reais do CEO (bancos, produtos, cartões)
   - Importar histórico CSV do CEO
   - Testar com 50-100 emails do Gmail

3. **Sprint médio (abril)**:
   - UI básica de ingestão (rotas essenciais: lista de drafts, aprovação, PDF viewer)
   - Documentação do MCP
   - Testes e2e pendentes (3.24, 3.25, 4.15, 5.15)

4. **Sprint futuro (maio)**:
   - Configurar OpenAI
   - Agente Document Parser (para PDFs que parsers determinísticos não reconhecem)
   - Agente Financial Classifier
   - Calibrar system prompts com dados reais

---

## 9. Ações / Responsáveis / Prazo

| Ação                                             | Responsável    | Prazo      |
| ------------------------------------------------ | -------------- | ---------- |
| Testar Gmail scan REAL (--limit 10, sem dry-run) | CEO            | 2026-03-26 |
| Testar worker ingestion com jobs reais           | CEO            | 2026-03-26 |
| Testar fluxo completo via MCP (list → approve)   | CEO + João     | 2026-03-27 |
| Implementar tool MCP `scan_gmail_label`          | João           | 2026-03-27 |
| Push código gmail-scanner + .env.local fix       | CEO            | 2026-03-26 |
| Integrar deploy Vercel CLI no GitLab CI (1.22)   | Fernando       | 2026-03-28 |
| Testar Google Auth staging e2e (3.AA.10)         | Fernando + CEO | 2026-03-28 |
| Cadastrar bancos/produtos reais do CEO           | CEO            | 2026-03-31 |
| Importar histórico CSV                           | CEO            | 2026-04-01 |
| Iniciar UI ingestão (rotas mínimas)              | Roberto        | 2026-04-07 |
| Criar conta OpenAI                               | CEO            | 2026-04-15 |
| Configurar agente Document Parser                | Pedro          | 2026-05-01 |
