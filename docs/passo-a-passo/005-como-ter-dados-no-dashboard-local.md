# 005 — Como Ter Dados no Dashboard (Localhost)

> Guia executivo: do zero até ver dados reais no dashboard local.

---

## Visão Geral: O Fluxo Completo

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        SEU BOLSO FELIZ — PIPELINE                          │
│                                                                             │
│  ╔══════════════════╗                                                       │
│  ║  FONTES DE DOCS  ║                                                       │
│  ╠══════════════════╣                                                       │
│  ║  📧 Gmail        ║──┐                                                    │
│  ║  📁 Pasta local  ║──┤                                                    │
│  ║  📤 Upload web   ║──┘  (futuro)                                          │
│  ╚══════════════════╝                                                       │
│         │                                                                   │
│         ▼                                                                   │
│  ┌──────────────────────────────────────────┐                               │
│  │  PASSO 1: SCANNER (Descobrir documentos) │                               │
│  │  bun run pipeline:passo01:scan-gmail     │                               │
│  │                                          │                               │
│  │  • Conecta no Gmail via OAuth2           │                               │
│  │  • Baixa anexos da label "Comprovantes"  │                               │
│  │  • Verifica duplicatas (origin_key)      │                               │
│  │  • Upload → Supabase Storage             │                               │
│  │  • Cria source_documents + jobs          │                               │
│  └──────────────┬───────────────────────────┘                               │
│                 │                                                            │
│                 ▼                                                            │
│  ┌────────────────────────────────────────────────────┐                     │
│  │  PASSO 2: INGESTION WORKER (Processar documentos)  │                     │
│  │  bun run pipeline:passo02:ingest                   │                     │
│  │                                                    │                     │
│  │  Para cada documento:                              │                     │
│  │                                                    │                     │
│  │  DISCOVERED ──→ DOWNLOADED ──→ HASHED ──→ QUEUED   │                     │
│  │       │              │            │          │      │                     │
│  │       │         Baixa PDF    SHA-256     Pronto     │                     │
│  │       │         do Storage    dedup     p/ parse    │                     │
│  │       │                                             │                     │
│  │  QUEUED ──→ PARSING ──→ PARSED ──→ CLASSIFIED      │                     │
│  │                │           │            │           │                     │
│  │          Extrai texto   Regex      Identifica:     │                     │
│  │          do PDF         CEMIG/     transação?      │                     │
│  │          (pdf-parse)    boleto     recorrência?    │                     │
│  │                                    consumo?        │                     │
│  │                                                    │                     │
│  │  CLASSIFIED ──→ DRAFTED ──→ PENDING_REVIEW 🔔      │                     │
│  │                    │              │                 │                     │
│  │              Gera drafts    Aguardando             │                     │
│  │              (rascunhos)    SUA APROVAÇÃO!         │                     │
│  │              no banco                              │                     │
│  └────────────────────┬───────────────────────────────┘                     │
│                       │                                                     │
│                       ▼                                                     │
│  ┌──────────────────────────────────────────────────────┐                   │
│  │  PASSO 3: DASHBOARD (Ver + Aprovar)                  │                   │
│  │  bun run pipeline:passo03:dev                        │                   │
│  │                                                      │                   │
│  │  http://localhost:3105/dashboard                      │                   │
│  │                                                      │                   │
│  │  ┌────────────────────────────────────────────────┐  │                   │
│  │  │  📊 Receitas      📊 Despesas     📊 Saldo    │  │                   │
│  │  │  R$ 8.500         R$ 4.230       R$ 4.270     │  │                   │
│  │  ├────────────────────────────────────────────────┤  │                   │
│  │  │  🔔 Drafts Pendentes (do worker):              │  │                   │
│  │  │  ┌─ CEMIG Mar/2026    R$ 547,89  [Aprovar] ──┐│  │                   │
│  │  │  │  COPASA Mar/2026   R$ 189,50  [Aprovar]   ││  │                   │
│  │  │  │  Boleto XYZ        R$ 1.200   [Aprovar]   ││  │                   │
│  │  │  └───────────────────────────────────────────┘│  │                   │
│  │  ├────────────────────────────────────────────────┤  │                   │
│  │  │  📋 Próximos Vencimentos                       │  │                   │
│  │  │  📋 Transações Recentes                        │  │                   │
│  │  │  📋 Faturas em Aberto                          │  │                   │
│  │  │  📋 Dívidas Ativas                             │  │                   │
│  │  └────────────────────────────────────────────────┘  │                   │
│  └──────────────────────────────────────────────────────┘                   │
│                       │                                                     │
│                       ▼                                                     │
│  ┌──────────────────────────────────────────────────────┐                   │
│  │  APÓS APROVAÇÃO:                                     │                   │
│  │                                                      │                   │
│  │  Draft APPROVED ──→ POSTED                           │                   │
│  │       │                  │                            │                   │
│  │  Você aprovou       Worker insere na tabela          │                   │
│  │  o rascunho         `transactions` real              │                   │
│  │                     → aparece no dashboard!          │                   │
│  └──────────────────────────────────────────────────────┘                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Como a IA Vai Ler o Documento?

```
┌─────────────────────────────────────────────────────────────────┐
│                     LEITURA DE DOCUMENTOS                        │
│                                                                  │
│  📄 PDF (conta CEMIG)                                           │
│      │                                                           │
│      ▼                                                           │
│  ┌─────────────────────────────────────────┐                    │
│  │  1. EXTRAÇÃO DE TEXTO                    │                    │
│  │     • pdf-parse (biblioteca local)       │                    │
│  │     • PDF protegido? → busca senha       │                    │
│  │       no Supabase Vault                  │                    │
│  │     • Resultado: texto bruto (string)    │   ← HOJE (local)  │
│  └──────────────┬──────────────────────────┘                    │
│                 │                                                │
│                 ▼                                                │
│  ┌─────────────────────────────────────────┐                    │
│  │  2. PARSER (Regex local)                 │                    │
│  │     • cemig-parser.ts                    │                    │
│  │       → detecta "CEMIG" no texto         │                    │
│  │       → regex extrai: valor, vencimento, │                    │
│  │         competência, kWh, nº documento   │                    │
│  │     • boleto-parser.ts                   │   ← HOJE (local)  │
│  │       → extrai: valor, código barras,    │                    │
│  │         vencimento, beneficiário         │                    │
│  └──────────────┬──────────────────────────┘                    │
│                 │                                                │
│                 ▼                                                │
│  ┌─────────────────────────────────────────┐                    │
│  │  3. GERAÇÃO DE RASCUNHO (Draft)          │                    │
│  │     • Cria draft_record com:             │                    │
│  │       - tipo: transação / recorrência    │                    │
│  │       - valor: R$ 547,89                 │                    │
│  │       - fornecedor: CEMIG                │                    │
│  │       - vencimento: 15/04/2026           │                    │
│  │       - categoria sugerida: moradia      │                    │
│  │       - tags sugeridas: essencial, casa  │   ← HOJE (local)  │
│  │       - prioridade: alta                 │                    │
│  │       - confiança: 95%                   │                    │
│  │     • VINCULADO ao source_document_id    │                    │
│  │       (link direto com o PDF original)   │                    │
│  └──────────────┬──────────────────────────┘                    │
│                 │                                                │
│                 ▼                                                │
│  ┌─────────────────────────────────────────┐                    │
│  │  4. REVISÃO HUMANA                       │                    │
│  │     • Você vê o rascunho no dashboard    │                    │
│  │     • Pode clicar no documento original  │                    │
│  │       (PDF) para conferir                │                    │
│  │     • Corrige se necessário              │                    │
│  │     • Aprova → vira transação real       │   ← HOJE          │
│  └──────────────┬──────────────────────────┘                    │
│                 │                                                │
│                 ▼                                                │
│  ┌─────────────────────────────────────────┐                    │
│  │  5. IA (FUTURO — Fase 4)                 │                    │
│  │     • OpenAI Vision: lê imagem do PDF    │                    │
│  │     • OpenAI Text: classifica texto      │                    │
│  │     • Sugere categoria, tags, prioridade │                    │
│  │     • Concilia com extrato bancário      │   ← FUTURO        │
│  │     • Assistente conversacional          │                    │
│  │     • Aprende com suas correções         │                    │
│  └─────────────────────────────────────────┘                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Vínculo Documento ↔ Registros

```
┌──────────────────┐     1:N      ┌───────────────────┐
│ source_documents │─────────────▶│  ingestion_jobs   │
│                  │              │  (processamento)  │
│  • origin_key    │              └───────────────────┘
│  • filename      │                       │
│  • storage_path ─┼─── link p/ ───▶ Supabase Storage (PDF original)
│  • status        │                       │
└────────┬─────────┘                       ▼
         │                    ┌────────────────────────┐
         │               1:N │ parsed_document_versions│
         │                   │ (texto extraído)        │
         │                   └────────────────────────┘
         │                              │
         │                              ▼
         │                   ┌────────────────────────┐
         │               1:N │ extraction_results      │
         │                   │ (dados estruturados)    │
         │                   │ • valor, vencimento     │
         │                   │ • fornecedor, kWh       │
         │                   └────────────────────────┘
         │                              │
         │                              ▼
         │                   ┌────────────────────────┐
         └──────────────────▶│ draft_records           │
            source_document_id│ (rascunhos p/ aprovar)  │
                             │ • draft_data JSON       │
                             │ • confidence_score      │
                             │ • status: pending_review│
                             └───────────┬────────────┘
                                         │ Após aprovação
                                         ▼
                             ┌────────────────────────┐
                             │ transactions            │
                             │ (registro REAL)         │
                             │ • source_document_id ◀──── link de volta!
                             │   (permite revisão)     │
                             └────────────────────────┘

Resultado: TODA transação sabe de qual documento veio.
           Você pode clicar e abrir o PDF original para conferir.
```

---

## Passo-a-Passo Executivo (Copiar e Colar)

### Pré-requisitos (uma vez só)

```bash
# 1. Supabase CLI instalado
brew install supabase/tap/supabase  # macOS
# ou: npm i -g supabase             # qualquer OS

# 2. Docker rodando (Supabase local precisa)
docker info  # deve funcionar sem erro

# 3. Dependências do projeto
bun install

# 4. Token do Gmail (se ainda não fez)
bun run get:gmail-token
```

### Passo 0: Subir o Banco Local

```bash
# Sobe Supabase local (Postgres + Auth + Storage + Studio)
bun run pipeline:passo00:db-start

# Confirma que está rodando
bun run pipeline:passo00:db-status

# Se precisar resetar (aplica migrations + seed com dados de teste):
bun run pipeline:passo00:db-reset
```

Após o reset, você já terá dados de seed:

- 3 bancos (Caixa, Nubank, C6)
- Cartões, contas, fornecedores
- Categorias e tags
- Períodos financeiros
- Templates recorrentes

Acesse o **Supabase Studio**: http://127.0.0.1:54323

### Passo 1: Escanear Documentos

**Opção A — Gmail (produção real):**

```bash
# Dry-run primeiro (não grava nada, só mostra o que faria)
bun run pipeline:passo01:scan-gmail:dry

# Se tudo ok, escaneia de verdade (10 emails)
bun run pipeline:passo01:scan-gmail:10

# Ou escaneia 50 de uma vez
bun run pipeline:passo01:scan-gmail
```

**Opção B — Pasta local (para teste):**

```bash
# Coloque PDFs na pasta ./inbox
mkdir -p inbox
cp ~/Downloads/conta-cemig.pdf inbox/
cp ~/Downloads/boleto-internet.pdf inbox/

# Inicia scanner local (fica rodando, monitorando a pasta)
bun run pipeline:passo01:scan-local
```

### Passo 2: Processar Documentos (Ingestion Worker)

```bash
# Em outro terminal — fica rodando, processando jobs a cada 5s
bun run pipeline:passo02:ingest
```

O worker vai:

1. Pegar cada documento descoberto pelo scanner
2. Baixar o PDF do Storage
3. Extrair texto com pdf-parse
4. Aplicar parsers (CEMIG, boleto genérico)
5. Gerar rascunhos (drafts) com dados extraídos
6. Marcar como `PENDING_REVIEW`

### Passo 3: Ver no Dashboard

```bash
# Em outro terminal — sobe o Next.js
bun run pipeline:passo03:dev
```

Abra: **http://localhost:3105/dashboard**

Você verá:

- Cards de resumo (receitas, despesas, saldo)
- Drafts pendentes para aprovação
- Transações recentes
- Próximos vencimentos

### Resumo: 3 Terminais Simultâneos

```
┌─ Terminal 1 ─────────────────────────────────────────┐
│ $ bun run pipeline:passo01:scan-gmail                │
│ [SCANNER] Conectando ao Gmail...                     │
│ [SCANNER] 12 mensagens encontradas com anexos        │
│ [SCANNER] 10 novos documentos, 2 duplicatas ignoradas│
│ [SCANNER] Upload concluído. Jobs criados: 10         │
└──────────────────────────────────────────────────────┘

┌─ Terminal 2 ─────────────────────────────────────────┐
│ $ bun run pipeline:passo02:ingest                    │
│ [WORKER] Polling ingestion_jobs...                   │
│ [WORKER] Job abc123: DISCOVERED → DOWNLOADED         │
│ [WORKER] Job abc123: DOWNLOADED → PARSED (CEMIG)     │
│ [WORKER] Job abc123: PARSED → DRAFTED                │
│ [WORKER] Job abc123: DRAFTED → PENDING_REVIEW ✅     │
│ [WORKER] 10 jobs processados com sucesso             │
└──────────────────────────────────────────────────────┘

┌─ Terminal 3 ─────────────────────────────────────────┐
│ $ bun run pipeline:passo03:dev                       │
│ ▲ Next.js 15.2.0 (Turbopack)                        │
│ - Local:   http://localhost:3105                     │
│ - Network: http://192.168.1.10:3105                  │
│                                                      │
│ ✓ Ready in 2.1s                                      │
└──────────────────────────────────────────────────────┘
```

---

## FAQ Rápido

### "Posso ter dados sem Gmail?"

**Sim!** Use `bun run pipeline:passo00:db-reset` — o seed.sql já insere dados realistas de teste (transações, fornecedores, categorias). O dashboard funciona imediatamente.

### "E se o PDF tiver senha?"

O worker consulta o **Supabase Vault** automaticamente. Você cadastra senhas de PDF no `user_secrets` via MCP tool `find_documents_without_password`.

### "Onde ficam os PDFs originais?"

No **Supabase Storage**, bucket `ingestion-originals`. Acessível via Studio: http://127.0.0.1:54323/project/default/storage/buckets/ingestion-originals

### "Posso usar o MCP/Copilot ao invés do dashboard?"

**Sim!** O servidor MCP tem 8 ferramentas. Basta rodar no VS Code:

```
@sbf list_draft_batches
@sbf approve_draft_batch <batch-id>
@sbf scan_local_folder ./inbox
```

### "Quando a IA entra nisso?"

**Fase 4** (futuro). Hoje os parsers usam regex local. Quando tivermos API key OpenAI:

- Vision: lê imagem do PDF que o regex não consegue
- Text: classifica texto automaticamente
- Sugere categorias, tags e prioridade com mais precisão
