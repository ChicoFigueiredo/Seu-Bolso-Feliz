# 005 — Plano Faseado: Ingestão, Automação, MCP, Agentes e Povoamento

> Documento de planejamento detalhado para o ciclo de ingestão, automação, MCP, agentes e povoamento de dados do **Seu Bolso Feliz**.  
> Referência: Prompt faseado da Verônica + Ata de refino de 2026-03-22.

---

## Visão Geral

### Objetivo do Ciclo
Deixar o sistema apto a:
- Ler e-mails e anexos do Gmail
- Ler documentos locais
- Detectar e evitar duplicidade por hash/fingerprint
- Extrair dados de comprovantes, faturas e PDFs
- Gerar registros em modo draft para revisão humana
- Permitir operação assistida via MCP e interface web
- Consolidar histórico real para orientar evolução do produto

### Princípio Operacional Absoluto
> **IA não grava diretamente no ledger principal por decisão livre.**  
> Toda ingestão automática passa por estados intermediários com revisão humana obrigatória.

---

## Arquitetura Operacional

### Componentes e Responsabilidades

```
┌─────────────────────────────────────────────────────────────────┐
│                        VERCEL (Next.js)                        │
│  ┌──────────┐ ┌──────────────┐ ┌──────────┐ ┌──────────────┐ │
│  │ Dashboard │ │ Telas Revisão│ │ API Routes│ │Server Actions│ │
│  └──────────┘ └──────────────┘ └──────────┘ └──────────────┘ │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                 packages/operations (lógica compartilhada)       │
└───────┬───────────────────┬───────────────────┬─────────────────┘
        │                   │                   │
┌───────▼───────┐   ┌──────▼──────┐    ┌───────▼───────┐
│  MCP Server   │   │   Workers   │    │   OpenAI      │
│(apps/mcp-srv) │   │  (workers/) │    │  (Fase 7+)    │
└───────┬───────┘   └──────┬──────┘    └───────┬───────┘
        │                  │                    │
┌───────▼──────────────────▼────────────────────▼─────────────────┐
│                      SUPABASE                                    │
│  ┌────────┐ ┌────┐ ┌───┐ ┌───────┐ ┌─────┐ ┌─────┐ ┌──────┐  │
│  │Postgres│ │Auth│ │RLS│ │Storage│ │Vault│ │Queue│ │ Cron │  │
│  └────────┘ └────┘ └───┘ └───────┘ └─────┘ └─────┘ └──────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Estrutura do Monorepo (Final)

```
seu.bolso.feliz/
├── apps/
│   ├── web/                    ← Next.js (Vercel) [existente]
│   ├── mobile/                 ← React Native/Expo [existente, vazio]
│   └── mcp-server/             ← MCP server local [Fase 5 — NOVO]
├── workers/
│   ├── ingestion/              ← Worker core de processamento [Fase 2 — NOVO]
│   ├── gmail-scanner/          ← Scanner de e-mails [Fase 3 — NOVO]
│   └── local-scanner/          ← Scanner de arquivos locais [Fase 2 — NOVO]
├── packages/
│   ├── config/                 ← Configurações compartilhadas [existente]
│   ├── domain/                 ← Lógica de domínio pura [existente]
│   ├── validation/             ← Schemas Zod [existente]
│   ├── shared-types/           ← Tipos TypeScript [existente]
│   ├── ui-tokens/              ← Tokens visuais [existente, stub]
│   ├── operations/             ← Serviços de domínio compartilhados [Fase 2 — NOVO]
│   └── ingestion-types/        ← Tipos e enums de ingestão [Fase 2 — NOVO]
├── supabase/
│   ├── config.toml
│   ├── seed.sql
│   ├── migrations/             ← [14+ migrations existentes]
│   ├── functions/              ← [2 Edge Functions existentes]
│   └── snippets/
├── __tests__/                  ← [168 testes existentes]
└── docs/
```

---

## Sprint 0 — Fechamento de Gaps Pendentes

### Objetivo
Fechar os gaps do ciclo anterior antes de avançar.

### Entregas
1. Criar materialized view `mv_supplier_spending`
2. Criar Edge Function `refresh-mv-supplier-spending`
3. Implementar confirm atômico na Edge Function `retroactive-supplier-association`
4. Povoar `seed.sql` com dados de teste realistas
5. Validação visual no Studio

### Critérios de Aceite
- `supabase db reset` cria banco funcional com dados de teste
- View materializada funciona e pode ser atualizada via Edge Function
- Confirm atômico garante integridade transacional

---

## Fase 1 — Fechamento Operacional e Prontidão de Ambientes

### Objetivo
Infraestrutura real de deployment e ambientes prontos para ingestão.

### 1.1 Vercel

| Tarefa | Responsável | Tipo |
|--------|-------------|------|
| Criar conta/organização Vercel | CEO | Manual |
| Conectar repositório GitLab ao Vercel | CEO + DevOps | Manual/Config |
| Configurar root directory: `apps/web` | DevOps | Config |
| Configurar framework preset: Next.js | DevOps | Config |
| Configurar preview deployments (por branch/MR) | DevOps | Config |
| Configurar environment variables (staging/prod) | DevOps | Config |
| Registrar domínio `seubolsofeliz.com.br` | CEO | Manual |
| Apontar DNS para Vercel | CEO + DevOps | Manual |
| Validar deploy real com página funcional | DevOps | Teste |

### 1.2 Supabase (ambientes)

| Tarefa | Responsável | Tipo |
|--------|-------------|------|
| Criar projeto Supabase STAGING | CEO | Manual |
| Criar projeto Supabase PRODUCTION | CEO | Manual |
| Anotar URLs e chaves de cada projeto | CEO | Manual |
| Configurar `supabase link` para staging | DevOps | Config |
| Aplicar migrations em staging | DevOps | CLI |
| Aplicar migrations em production | DevOps | CLI |
| Deploy Edge Functions em staging | DevOps | CLI |
| Validar RLS em staging | DBA | Teste |

### 1.3 GitLab CI/CD

| Tarefa | Responsável | Tipo |
|--------|-------------|------|
| Configurar variáveis protegidas por ambiente | DevOps | Config |
| Substituir placeholder de deploy-web por job real (Vercel CLI) | DevOps | Code |
| Validar pipeline completo em branch develop | DevOps | Teste |
| Validar deploy staging end-to-end | DevOps | Teste |
| Documentar política de promoção staging → production | DevOps | Doc |

### 1.4 Estrutura do Monorepo

| Tarefa | Responsável | Tipo |
|--------|-------------|------|
| Criar diretório `apps/mcp-server/` com package.json stub | DevOps | Code |
| Criar diretório `workers/ingestion/` com package.json stub | DevOps | Code |
| Criar diretório `workers/gmail-scanner/` com package.json stub | DevOps | Code |
| Criar diretório `workers/local-scanner/` com package.json stub | DevOps | Code |
| Criar diretório `packages/operations/` com package.json stub | DevOps | Code |
| Atualizar workspace config no root package.json | DevOps | Code |

### Segredos e Variáveis por Ambiente

| Variável | Local | Staging | Production |
|----------|-------|---------|------------|
| `NEXT_PUBLIC_SUPABASE_URL` | supabase local | projeto staging | projeto production |
| `NEXT_PUBLIC_SUPABASE_ANON_KEY` | local | staging | production |
| `SUPABASE_SERVICE_ROLE_KEY` | local | staging | production |
| `SUPABASE_DB_PASSWORD` | local | staging | production |
| `SUPABASE_PROJECT_ID` | — | staging ID | production ID |
| `SUPABASE_ACCESS_TOKEN` | token pessoal | CI/CD | CI/CD |
| `VERCEL_TOKEN` | — | CI/CD | CI/CD |
| `VERCEL_ORG_ID` | — | Vercel | Vercel |
| `VERCEL_PROJECT_ID` | — | Vercel | Vercel |

### Critérios de Aceite
- [ ] Deploy web real funcionando no Vercel (URL acessível)
- [ ] Preview deployment por branch/MR funcionando
- [ ] Pipeline GitLab executando deploy real (não placeholder)
- [ ] Supabase staging com migrations e Edge Functions aplicadas
- [ ] Checklist de variáveis e segredos 100% concluído
- [ ] Nenhum deploy crítico dependente de passo manual obscuro

---

## Fase 2 — Estrutura de Ingestão e Idempotência Documental

### Objetivo
Base técnica para ingestão de documentos com deduplicação e reprocessamento seguro.

### 2.1 Modelagem de Dados

#### Tabelas Novas

```sql
-- Execuções do worker
CREATE TABLE ingestion_runs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_type TEXT NOT NULL,        -- 'gmail', 'local', 'manual_upload'
  status TEXT NOT NULL DEFAULT 'running',  -- 'running', 'completed', 'failed', 'cancelled'
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  completed_at TIMESTAMPTZ,
  metadata JSONB DEFAULT '{}',      -- parâmetros da execução (label, período, pasta)
  stats JSONB DEFAULT '{}',         -- contadores (total, processados, duplicados, falhas)
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- Jobs individuais de processamento
CREATE TABLE ingestion_jobs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  run_id UUID REFERENCES ingestion_runs(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_document_id UUID,          -- preenchido após criação do source_document
  status TEXT NOT NULL DEFAULT 'discovered',
  -- Máquina de estados:
  -- discovered → downloaded → hashed → queued → parsing → parsed →
  -- classified → reconciled → drafted → pending_review → approved → posted
  -- (qualquer) → failed
  error_message TEXT,
  error_details JSONB,
  retry_count INT DEFAULT 0,
  max_retries INT DEFAULT 3,
  metadata JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- Documentos canônicos ingeridos
CREATE TABLE source_documents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  origin_type TEXT NOT NULL,        -- 'gmail', 'local_file', 'manual_upload'
  origin_key TEXT NOT NULL,         -- chave única de proveniência
  -- Gmail fields
  gmail_message_id TEXT,
  gmail_thread_id TEXT,
  gmail_attachment_id TEXT,
  gmail_label TEXT,
  gmail_date TIMESTAMPTZ,
  gmail_from TEXT,
  gmail_subject TEXT,
  -- Local file fields
  local_filepath TEXT,
  local_mtime TIMESTAMPTZ,
  -- Common fields
  filename TEXT NOT NULL,
  mime_type TEXT,
  file_size_bytes BIGINT,
  storage_path TEXT,                -- caminho no Supabase Storage
  status TEXT NOT NULL DEFAULT 'new',
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(user_id, origin_type, origin_key)
);

-- Fingerprints de documentos
CREATE TABLE document_fingerprints (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  source_document_id UUID REFERENCES source_documents(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  content_hash TEXT NOT NULL,        -- SHA-256 dos bytes brutos
  canonical_fingerprint TEXT,        -- hash do conteúdo extraído normalizado
  hash_algorithm TEXT DEFAULT 'sha256',
  created_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(user_id, content_hash)
);

-- Versões de parsing/extração
CREATE TABLE parsed_document_versions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  source_document_id UUID REFERENCES source_documents(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  version_number INT NOT NULL DEFAULT 1,
  parser_type TEXT NOT NULL,         -- 'local_text', 'local_regex', 'openai_vision', 'openai_text'
  parser_version TEXT,               -- versão do parser/model
  raw_text TEXT,                     -- texto extraído bruto
  structured_data JSONB,             -- dados extraídos estruturados
  confidence_score NUMERIC(3,2),     -- 0.00 a 1.00
  metadata JSONB DEFAULT '{}',       -- tokens usados, custo, tempo de processamento
  created_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(source_document_id, version_number)
);

-- Resultados de extração estruturados
CREATE TABLE extraction_results (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  parsed_version_id UUID REFERENCES parsed_document_versions(id) NOT NULL,
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  supplier_name_raw TEXT,
  supplier_id UUID REFERENCES suppliers(id),
  supplier_confidence NUMERIC(3,2),
  competence_date DATE,
  due_date DATE,
  total_amount NUMERIC(12,2),
  currency TEXT DEFAULT 'BRL',
  breakdown JSONB,                   -- {tarifa, impostos, multa, juros, etc}
  document_number TEXT,              -- código de barras, nosso número
  contract_identifier TEXT,          -- unidade consumidora, contrato
  consumption_data JSONB,            -- {kWh, m3, etc}
  category_suggestion TEXT,
  tags_suggestion TEXT[],
  priority_suggestion TEXT,
  financial_period_suggestion JSONB,
  metadata JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now()
);

-- Drafts de registros financeiros
CREATE TABLE draft_records (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  batch_id UUID REFERENCES draft_batches(id),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_document_id UUID REFERENCES source_documents(id),
  extraction_result_id UUID REFERENCES extraction_results(id),
  draft_type TEXT NOT NULL,          -- 'transaction', 'recurring_template', 'liability', 'consumption_metric'
  status TEXT NOT NULL DEFAULT 'pending_review',
  -- pending_review → approved → posted
  -- pending_review → rejected → archived
  -- pending_review → corrected → pending_review (loop)
  draft_data JSONB NOT NULL,         -- dados do draft conforme tipo
  corrections JSONB,                 -- correções manuais do usuário
  confidence_score NUMERIC(3,2),
  approved_at TIMESTAMPTZ,
  approved_by UUID REFERENCES auth.users(id),
  posted_record_id UUID,             -- ID do registro final no ledger
  posted_record_type TEXT,           -- 'transaction', 'recurring_template', etc.
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- Lotes de drafts para aprovação em massa
CREATE TABLE draft_batches (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  run_id UUID REFERENCES ingestion_runs(id),
  name TEXT,
  status TEXT NOT NULL DEFAULT 'open',  -- 'open', 'reviewing', 'approved', 'partial', 'rejected'
  total_drafts INT DEFAULT 0,
  approved_count INT DEFAULT 0,
  rejected_count INT DEFAULT 0,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- Logs de ingestão
CREATE TABLE ingestion_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  run_id UUID REFERENCES ingestion_runs(id),
  job_id UUID REFERENCES ingestion_jobs(id),
  level TEXT NOT NULL DEFAULT 'info',  -- 'debug', 'info', 'warn', 'error'
  message TEXT NOT NULL,
  details JSONB DEFAULT '{}',
  created_at TIMESTAMPTZ DEFAULT now()
);
```

### 2.2 Máquina de Estados

```
INGESTION JOB STATES:
═══════════════════════════════════════════════════════
                                                       
  discovered ──► downloaded ──► hashed ──► queued      
                                             │         
                                    ┌────────▼────────┐
                                    │    parsing      │
                                    └────────┬────────┘
                                             │         
                      parsed ◄───────────────┘         
                        │                              
                   classified                          
                        │                              
                   reconciled                          
                        │                              
                     drafted                           
                        │                              
                 pending_review                        
                    ╱        ╲                          
              approved     rejected                    
                 │            │                         
              posted       archived                    
                                                       
  ★ qualquer estado → failed (com retry_count)         
  ★ failed com retries → retryable → re-enqueue        
  ★ failed sem retries → dead_letter                   
═══════════════════════════════════════════════════════
```

### 2.3 Estratégia de Hash/Fingerprint

| Tipo | Algoritmo | Quando | Objetivo |
|------|-----------|--------|----------|
| `content_hash` | SHA-256 dos bytes brutos | Ao baixar/ler arquivo | Detectar arquivo exatamente igual |
| `canonical_fingerprint` | SHA-256 do texto extraído normalizado (lowercase, sem espaços extras, sem metadados) | Após parsing | Detectar documento semanticamente equivalente |
| `origin_key` | Chave composta de proveniência | Ao descobrir | Vincular proveniência sem depender de hash |

**Política de Idempotência:**
1. Ao ingerir, calcular `content_hash`
2. Se `content_hash` existe para o mesmo user → **duplicata exata** → rejeitar (ou atualizar metadados sem reprocessar)
3. Se `content_hash` é novo, mas `canonical_fingerprint` existe → **duplicata semântica** → alertar e vincular
4. Se `origin_key` existe → **mesmo documento da mesma fonte** → atualizar se bytes mudaram, ignorar se iguais
5. Reprocessamento forçado: flag `force_reprocess=true` → recria parsed_version, não duplica source_document

### 2.4 Worker Local Inicial

**Runtime:** Bun  
**Localização:** `workers/ingestion/`

**Fluxo básico:**
```
1. Buscar próximo job na fila (status = 'queued')
2. Lock otimista (UPDATE ... WHERE status = 'queued' RETURNING)
3. Processar conforme estado atual
4. Atualizar status
5. Se houver falha, incrementar retry_count
6. Loop
```

**Modo de execução:**
- `bun run workers/ingestion/src/index.ts` — processo standalone
- Configurável por env vars: `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY`, `POLL_INTERVAL_MS`

### 2.5 Scanner Local

**Localização:** `workers/local-scanner/`

**Fluxo:**
```
1. Receber caminho de pasta como parâmetro
2. Listar arquivos recursivamente (filtro por extensão: .pdf, .png, .jpg, .csv)
3. Para cada arquivo:
   a. Calcular content_hash
   b. Verificar se já existe no banco
   c. Se novo: criar source_document + fazer upload para Storage + criar ingestion_job
   d. Se duplicata: logging e skip
4. Registrar ingestion_run com stats
```

### Critérios de Aceite
- [ ] Todas as tabelas criadas com RLS
- [ ] Um documento local pode ser ingerido via scanner
- [ ] O mesmo documento é detectado como duplicado na segunda execução
- [ ] Reprocessamento forçado funciona sem criar duplicata de source_document
- [ ] Worker local processa fila e atualiza estados
- [ ] Logs registrados no banco
- [ ] Todos os documentos rastreáveis (origem, hash, estado)

---

## Fase 3 — Integração Inicial com Gmail

### Objetivo
Leitura real de e-mails e anexos do Gmail por polling/scan manual.

### 3.1 Configurações Necessárias (CEO)

| Passo | Ação | Ambiente |
|-------|------|----------|
| 1 | Criar projeto no Google Cloud Console | console.cloud.google.com |
| 2 | Habilitar Gmail API | APIs & Services |
| 3 | Configurar OAuth consent screen | OAuth consent |
| 4 | Criar credenciais OAuth (Web Application) | Credentials |
| 5 | Adicionar redirect URIs | Credentials |
| 6 | Gerar e salvar Client ID + Client Secret | Credentials |
| 7 | Armazenar segredos no Supabase Vault | Supabase Dashboard |

### 3.2 Escopos Mínimos

```
https://www.googleapis.com/auth/gmail.readonly
```

Apenas leitura. Nenhuma escrita, envio ou modificação de e-mails.

### 3.3 Gmail Scanner

**Localização:** `workers/gmail-scanner/`

**Funcionalidades:**
- Scan por label (`Comprovantes`, `Faturas`)
- Scan por período (`after:2026/01/01 before:2026/03/31`)
- Scan por query livre (`from:cemig.com.br has:attachment`)
- Backfill: varredura de data X até data Y
- Detecção de mensagens já processadas (via `gmail_message_id` em `source_documents`)

**Fluxo:**
```
1. Autenticar via OAuth (refresh token do Vault)
2. Executar query no Gmail
3. Para cada mensagem:
   a. Verificar se gmail_message_id já existe → skip
   b. Listar anexos
   c. Para cada anexo:
      - Baixar bytes
      - Calcular content_hash
      - Criar source_document (com gmail_*, filename, mime_type)
      - Upload para Storage (bucket: ingestion-originals)
      - Criar ingestion_job
4. Registrar ingestion_run com stats
```

**Modos de operação:**
- `--dry-run` — lista o que seria processado sem gravar
- `--label "Comprovantes"` — filtra por label
- `--after 2026-01-01 --before 2026-03-31` — filtra por período
- `--query "from:cemig.com.br"` — query livre

### 3.4 Armazenamento de Tokens OAuth

| Segredo | Onde | Como |
|---------|------|------|
| Client ID | Supabase Vault | `select vault.create_secret('gmail_client_id', '...')` |
| Client Secret | Supabase Vault | `select vault.create_secret('gmail_client_secret', '...')` |
| Refresh Token | Supabase Vault | `select vault.create_secret('gmail_refresh_token', '...')` |

O access token é gerado em runtime a partir do refresh token e **nunca é persistido**.

### Critérios de Aceite
- [ ] Varrer uma label do Gmail gera ingestion_jobs
- [ ] Anexos reais são baixados e armazenados no Storage
- [ ] Mensagens já processadas são detectadas e ignoradas
- [ ] Modo dry-run funciona
- [ ] Backfill por período funciona
- [ ] Tokens armazenados de forma segura no Vault
- [ ] Falhas registradas por mensagem/anexo

---

## Fase 4 — Parsing Documental, Segredos e Geração de Drafts

### Objetivo
Transformar documentos ingeridos em drafts financeiros auditáveis.

### 4.1 Pipeline de Parsing

```
source_document
     │
     ▼
 ┌─────────────┐
 │ Decifragem  │ ← senha via user_secrets (por fornecedor/contrato)
 │ (se PDF     │
 │  protegido) │
 └──────┬──────┘
        │
        ▼
 ┌─────────────┐
 │ Extração de │ ← pdf-parse (texto), sharp (imagens)
 │ texto bruto │
 └──────┬──────┘
        │
        ▼
 ┌─────────────┐
 │ Parsing     │ ← regex patterns por fornecedor conhecido
 │ determinís- │   (CEMIG, Nubank, SAAE, etc.)
 │ tico (local)│
 └──────┬──────┘
        │
        ▼
 ┌─────────────┐
 │ Estruturação│ ← extraction_result com campos identificados
 └──────┬──────┘
        │
        ▼
 ┌─────────────┐
 │ Geração de  │ ← draft_record(s) por extraction_result
 │ Drafts      │
 └─────────────┘
```

### 4.2 Parsing Determinístico (Fase 4 — sem IA)

Para fornecedores conhecidos, criar **parsers específicos**:

| Fornecedor | Tipo | Campos extraídos |
|------------|------|------------------|
| CEMIG | Conta de luz | UC, consumo kWh, bandeira, breakdown, competência, vencimento |
| SAAE/COPASA | Conta de água | hidrômetro, consumo m³, breakdown, competência, vencimento |
| Nubank | Fatura cartão | itens, total, vencimento, cartão |
| Genérico | Boleto/comprovante | valor, vencimento, código de barras |

Para documentos sem parser específico, extração genérica de texto + campos básicos (valor, data).

### 4.3 Segredos para PDFs Protegidos

Fluxo de busca de senha:
```
1. Identificar fornecedor provável (pelo nome do arquivo, origem do e-mail, etc.)
2. Buscar em user_secrets: WHERE type = 'pdf_password' AND (supplier_id = X OR metadata->>'pattern' LIKE ...)
3. Tentar abrir PDF com senha encontrada
4. Se falhar: marcar job como 'failed' com motivo 'password_not_found' ou 'password_wrong'
5. Encaminhar para fila de revisão manual
```

### 4.4 Geração de Drafts

Um `extraction_result` pode gerar um ou mais `draft_records`:

| Tipo de draft | Quando |
|---------------|--------|
| `transaction` | Conta paga, comprovante de pagamento |
| `recurring_template` | Primeira ocorrência de despesa recorrente |
| `liability` | Empréstimo, financiamento detectado |
| `consumption_metric` | Conta de concessionária com dados de consumo |

Cada draft carrega:
- Referência ao `source_document` e `extraction_result`
- `confidence_score` (herdado da extração)
- `draft_data` (JSON conforme tipo: campos de transaction, recurring, etc.)
- Status `pending_review` por padrão

### 4.5 Score de Confiança

| Faixa | Significado | Ação padrão |
|-------|-------------|-------------|
| > 0.85 | Alta confiança | Sugerir aprovação rápida |
| 0.50 – 0.85 | Média confiança | Revisão com campos destacados |
| < 0.50 | Baixa confiança | Revisão manual obrigatória |

### Critérios de Aceite
- [ ] PDF protegido por senha pode ser processado usando segredo do Vault
- [ ] Fornecedor e período sugeridos corretamente para fornecedores conhecidos
- [ ] Drafts criados sem poluir o ledger principal
- [ ] Documentos com baixa confiança enviados para revisão
- [ ] Consumo/métrica capturado quando disponível
- [ ] Versão de extração persistida para auditoria
- [ ] Reprocessamento gera nova versão, não sobrescreve

---

## Fase 5 — MCP Local + Copilot/VS Code

### Objetivo
MCP próprio para operação assistida via VS Code/GitHub Copilot.

### 5.1 Estrutura

```
apps/mcp-server/
├── package.json
├── tsconfig.json
└── src/
    ├── index.ts              — entrypoint (stdio transport)
    ├── server.ts             — setup do MCP server
    └── tools/
        ├── scan-gmail-label.ts
        ├── scan-gmail-period.ts
        ├── scan-local-folder.ts
        ├── list-unparsed-documents.ts
        ├── reprocess-document.ts
        ├── resolve-supplier-candidates.ts
        ├── list-draft-batches.ts
        ├── approve-draft-batch.ts
        ├── recompute-financial-periods.ts
        └── find-documents-without-password.ts
```

### 5.2 Tools Mínimas

| Tool | Tipo | Descrição |
|------|------|-----------|
| `scan_gmail_label` | write-safe | Varrer label do Gmail e criar jobs |
| `scan_gmail_period` | write-safe | Varrer período do Gmail |
| `scan_local_folder` | write-safe | Varrer pasta local e criar jobs |
| `list_unparsed_documents` | read-only | Listar documentos sem parsing |
| `reprocess_document` | write-safe | Reprocessar documento específico |
| `resolve_supplier_candidates` | read-only | Sugerir fornecedores para nome |
| `list_draft_batches` | read-only | Listar lotes de drafts |
| `approve_draft_batch` | write-risky | Aprovar lote (com dry-run) |
| `recompute_financial_periods` | write-safe | Recalcular períodos financeiros |
| `find_documents_without_password` | read-only | Documentos que falharam por senha |

### 5.3 Configuração VS Code

```jsonc
// .vscode/mcp.json
{
  "servers": {
    "sbf": {
      "type": "stdio",
      "command": "bun",
      "args": ["run", "apps/mcp-server/src/index.ts"],
      "env": {
        "SUPABASE_URL": "${env:SUPABASE_URL}",
        "SUPABASE_SERVICE_ROLE_KEY": "${env:SUPABASE_SERVICE_ROLE_KEY}"
      }
    }
  }
}
```

### 5.4 Autenticação Local

- O MCP roda localmente com service_role_key do Supabase
- A key já tem acesso total ao banco (bypass RLS)
- Para segurança: o MCP só deve ser invocado em ambiente de desenvolvimento
- Todas as operações logadas na tabela `ingestion_logs`

### Critérios de Aceite
- [ ] MCP roda localmente via `bun run`
- [ ] Copilot no VS Code descobre e lista as tools
- [ ] Pelo menos um fluxo completo de backfill funciona via MCP
- [ ] A mesma tool não duplica registros ao ser executada duas vezes
- [ ] Documentação de uso com exemplos reais

---

## Fase 6 — Interface Web de Revisão

### Objetivo
Telas para tornar o fluxo de ingestão visível e operável.

### 6.1 Novas Rotas

```
/dashboard/ingestion              — visão geral do pipeline
/dashboard/ingestion/runs         — histórico de execuções do worker
/dashboard/ingestion/documents    — documentos ingeridos (com filtros)
/dashboard/ingestion/drafts       — drafts pendentes de revisão
/dashboard/ingestion/drafts/[id]  — detalhe de draft (split-view)
/dashboard/ingestion/conflicts    — duplicidades e conflitos
/dashboard/ingestion/batches      — lotes para aprovação em massa
```

### 6.2 Tela de Revisão de Draft (Core)

**Layout:** Split-view
- **Esquerda:** Preview do documento original (PDF viewer ou imagem)
- **Direita:** Formulário com campos extraídos, editáveis

**Informações exibidas:**
- Documento original (visualizável)
- Status do job
- Hash/fingerprint
- Origem (Gmail label/local path)
- Fornecedor sugerido (com link para resolver)
- Período financeiro sugerido
- Score de confiança (visual + textual para A11y)
- Ação recomendada

**Ações:**
- Aprovar (individual ou lote)
- Corrigir (editar campos e re-submeter)
- Rejeitar (com motivo)
- Vincular manualmente (quando IA errar)
- Reprocessar (disparar novo parsing)

### 6.3 Atalhos de Teclado

| Atalho | Ação |
|--------|------|
| `A` | Aprovar draft atual |
| `R` | Rejeitar draft atual |
| `E` | Editar/corrigir draft |
| `→` | Próximo draft |
| `←` | Draft anterior |
| `Shift+A` | Aprovar todos do lote |

### Critérios de Aceite
- [ ] Fluxo visível: ingestão → draft → revisão → aprovação
- [ ] Usuário pode povoar o sistema sem cadastro manual
- [ ] Dados aprovados alimentam telas financeiras existentes
- [ ] Filtros por: ciclo financeiro, fornecedor, categoria/tag, instituição, tipo
- [ ] Aprovação em lote funciona
- [ ] Atalhos de teclado funcionam
- [ ] Acessibilidade validada (navegação por teclado, leitores de tela)

---

## Fase 7 — Agentes OpenAI

### Objetivo
IA disciplinada sobre pipeline controlado.

### 7.1 Arquitetura de Agentes

```
packages/operations/
  src/
    ai/
      agents/
        document-parser.ts       — agente de parsing documental
        supplier-resolver.ts     — agente de resolução de fornecedor
        financial-classifier.ts  — agente de classificação
        reconciler.ts            — agente de reconciliação
      prompts/
        system-prompts.ts        — system prompts por agente
      tools/
        domain-tools.ts          — tools determinísticas expostas aos agentes
      client.ts                  — client OpenAI configurado
      logger.ts                  — logging de chamadas IA
```

### 7.2 Agentes Propostos

| Agente | Input | Output | Tools |
|--------|-------|--------|-------|
| Document Parser | Texto bruto + imagem | Dados estruturados | — |
| Supplier Resolver | Nome raw | supplier_id + confidence | search_suppliers, list_supplier_aliases |
| Financial Classifier | Dados extraídos | categoria, tags[], prioridade | list_categories, list_tags |
| Reconciler | Draft + registros existentes | match_id + confidence | search_transactions, search_recurring |

### 7.3 Auditoria de Chamadas IA

Cada chamada registra:
- Model utilizado (gpt-4o, gpt-4o-mini, etc.)
- Input (prompt + context)
- Output (resposta completa)
- Tokens: prompt_tokens, completion_tokens
- Custo estimado
- Latência
- Confidence score retornado
- Versão do system prompt

### 7.4 Integração com Pipeline

O pipeline da Fase 4 ganha **upgrade opcional** com IA:
```
Parsing local → (se confiança < threshold) → Parsing IA → Comparação → Melhor resultado
```

A IA é **fallback e complemento**, não substituta do parsing determinístico.

### Critérios de Aceite
- [ ] Usuário pode pedir varredura, classificação e reconciliação por linguagem natural
- [ ] IA usa ferramentas do domínio (não inventa dados)
- [ ] Nenhuma escrita no ledger sem trilha e revisão
- [ ] Todas as chamadas IA auditáveis
- [ ] Custo por chamada rastreável

---

## Fase 8 — Preparação para ChatGPT (Documentação)

### Objetivo
Documentar a preparação para futura exposição remota do MCP.

### 8.1 Avaliações

| Item | Decisão |
|------|---------|
| Exposição remota do MCP | Documentar; implementar apenas quando necessário |
| Autenticação remota | OAuth2 via Supabase Auth + API keys |
| Tools disponíveis externamente | Apenas read-only e write-safe |
| Gmail push notifications | Avaliar após pipeline estabilizado |
| Worker remoto | Avaliar hospedagem em Supabase Edge Functions ou Vercel Functions |

### 8.2 Classificação de Tools para Exposição

| Classificação | Critério | Exemplo |
|--------------|----------|---------|
| `read-only` | Sem efeitos colaterais | list_draft_batches, resolve_supplier_candidates |
| `write-safe` | Cria registros em staging/draft, idempotente | scan_gmail_label, reprocess_document |
| `write-risky` | Modifica dados no ledger principal | approve_draft_batch, merge_suppliers |

### Critérios de Aceite
- [ ] Documento de avaliação de exposição remota criado
- [ ] Classificação de tools documentada
- [ ] Requisitos de autenticação remota documentados
- [ ] Evolução não exige reescrever a camada operacional

---

## Riscos e Dependências

### Riscos

| ID | Risco | Probabilidade | Impacto | Mitigação |
|----|-------|---------------|---------|-----------|
| R1 | Falha na configuração Google Cloud/OAuth | Média | Alto (bloqueia F3) | Passo-a-passo detalhado com screenshots |
| R2 | Supabase Queues instável/indisponível | Baixa | Médio | Fallback: polling com lock otimista |
| R3 | Custo OpenAI excede budget | Média | Médio | Budget cap + uso de gpt-4o-mini primeiro |
| R4 | PDFs com formatos inesperados | Alta | Baixo | Fila de revisão manual como fallback |
| R5 | Vercel free tier insuficiente | Baixa | Alto | Avaliar Pro ($20/mês) |
| R6 | GitLab → Vercel sem integração direta | Média | Médio | Vercel CLI no pipeline como alternativa |
| R7 | Timeout em Edge Functions | Média | Baixo | Processing pesado no worker local |
| R8 | Token refresh do Gmail expirar | Baixa | Médio | Monitoring + alerta + re-auth manual |

### Dependências Externas

| Dep | Fase | Responsável | Bloqueante |
|-----|------|-------------|------------|
| Conta Vercel | F1 | CEO | Sim |
| Supabase staging project | F1 | CEO | Sim |
| Supabase production project | F1 | CEO | Sim |
| Domínio seubolsofeliz.com.br | F1 | CEO | Não (pode apontar depois) |
| Google Cloud Project | F3 | CEO | Sim para F3 |
| Gmail OAuth credentials | F3 | CEO | Sim para F3 |
| API Key OpenAI | F7 | CEO | Sim para F7 |
| Budget mensal OpenAI | F7 | CEO | Sim para F7 |

---

## Cronograma Sugerido

```
Sprint 0  ░░  [Gaps pendentes]
Sprint 1  ████████  [Fase 1 — Ambientes]
Sprint 2  ████████████  [Fase 2 — Ingestão + Worker]
Sprint 3  ████████  [Fase 3 — Gmail]
Sprint 4  ████████████  [Fase 4 — Parsing + Drafts]
Sprint 5  ████████  [Fase 5 — MCP]
Sprint 6  ████████████  [Fase 6 — Interface Web]
Sprint 7  ████████████  [Fase 7 — OpenAI]
Sprint 8  ████  [Fase 8 — Doc ChatGPT]
```

**Nota:** A duração de cada sprint depende da disponibilidade do CEO para ações manuais e da resposta dos serviços externos.
