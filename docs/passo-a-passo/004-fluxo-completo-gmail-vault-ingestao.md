# 004 — Fluxo Completo: Gmail Token → Vault → Scan → Ingestão Manual

> **Criado em:** 2026-03-25  
> **Atualizado em:** 2026-03-25 (resultado do scan real de 200 msgs + correções de bugs)  
> **Objetivo:** Guia prático e sequencial para o CEO executar o ciclo completo de ingestão manual a partir do zero — desde verificar o refresh token até ver os jobs processados.

---

## Índice

1. [Visão geral do fluxo](#1-visao-geral)
2. [Pré-requisitos](#2-pre-requisitos)
3. [Passo 1 — Verificar/gerar o refresh token do Gmail](#3-refresh-token)
4. [Passo 2 — Garantir variáveis no `.env.local`](#4-env-local)
5. [Passo 3 — Gravar token no Supabase Vault (local)](#5-vault-local)
6. [Passo 4 — Gravar token no Supabase Vault (staging)](#6-vault-staging)
7. [Passo 5 — Executar o Gmail Scanner](#7-gmail-scanner)
8. [Passo 6 — Executar o worker de Ingestão](#8-ingestion-worker)
9. [Passo 7 — Verificar resultados](#9-verificar)
10. [Troubleshooting](#10-troubleshooting)
11. [Resultados do primeiro scan real (2026-03-25)](#11-resultados-scan-real)

---

## 1. Visão geral do fluxo {#1-visao-geral}

```
.env / .env.local (GMAIL_REFRESH_TOKEN)
        ↓
[worker] gmail-scanner     →  Storage  →  ingestion_jobs (DISCOVERED)
        ↓                                        ↓
   Gmail API                          [worker] ingestion
   (busca e-mails, baixa anexos)         (parse → draft)
                                                 ↓
                                     draft_batches / draft_items
                                                 ↓
                                    CEO aprova via MCP ou Web UI
```

**Para local:** as variáveis ficam em `.env.local`.  
**Para staging/prod:** as variáveis ficam no **Supabase Vault** (lidas por Edge Functions).

---

## 2. Pré-requisitos {#2-pre-requisitos}

- [ ] Supabase local rodando: `supabase status` deve mostrar URL `http://127.0.0.1:54321`
- [ ] `.env` com `GOOGLE_MAIL_CLIENT_ID`, `GOOGLE_MAIL_CLIENT_SECRET`, `GMAIL_REFRESH_TOKEN`
- [ ] `.env.local` com `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY`, `LOCAL_USER_ID`
- [ ] Label "Comprovantes" criada no Gmail
- [ ] Bun instalado (`bun --version`)

Se o Supabase local não estiver rodando:

```bash
supabase start
```

---

## 3. Passo 1 — Verificar/gerar o refresh token do Gmail {#3-refresh-token}

### 3.1 Verificar se já existe

```bash
grep GMAIL_REFRESH_TOKEN .env
```

Se retornar uma linha com valor (começa com `1//`), o token já existe — **pule para o Passo 2**.

### 3.2 Gerar novo token (se não existir)

> Só é necessário se nunca gerou antes ou se o token foi revogado.

```bash
bun run workers/gmail-scanner/get-token.ts
```

O script vai:

1. Ler `GOOGLE_MAIL_CLIENT_ID` e `GOOGLE_MAIL_CLIENT_SECRET` do `.env`
2. Abrir o navegador na página de autorização do Google
3. Após você autorizar, exibir o token no terminal

Copie a linha impressa `GMAIL_REFRESH_TOKEN=1//03...` e adicione ao `.env`:

```bash
echo 'GMAIL_REFRESH_TOKEN=<VALOR_COPIADO>' >> .env
```

### 3.3 Testar se o token funciona (dry-run)

```bash
bun run workers/gmail-scanner/src/index.ts --dry-run --label Comprovantes --limit 3
```

Saída esperada:

```
✅ Label encontrada: "Comprovantes" (ID: Label_11)
📨 [1] ...
  🔍 Anexo: ...pdf
Modo dry-run — nenhuma operação foi executada
```

Se aparecer `❌ Erro ao autenticar Gmail`, o token é inválido → volte ao 3.2.

---

## 4. Passo 2 — Garantir variáveis no `.env.local` {#4-env-local}

O arquivo `.env.local` é o que os workers usam para dados sensíveis locais. Verifique:

```bash
cat .env.local
```

Deve conter **todas** estas variáveis:

```dotenv
NEXT_PUBLIC_SUPABASE_URL=http://127.0.0.1:54321
NEXT_PUBLIC_SUPABASE_ANON_KEY=<anon_key_local>

# Workers (Supabase local)
SUPABASE_URL=http://127.0.0.1:54321
SUPABASE_SERVICE_ROLE_KEY=<service_role_key_local>
LOCAL_USER_ID=<seu_user_id>

# Gmail
GOOGLE_MAIL_CLIENT_ID=<client_id>
GOOGLE_MAIL_CLIENT_SECRET=<client_secret>
GMAIL_REFRESH_TOKEN=<refresh_token>
```

> **Nota:** As variáveis `GOOGLE_MAIL_*` e `GMAIL_REFRESH_TOKEN` já estão no `.env` raiz — o Bun carrega ambos os arquivos. Mas é boa prática tê-las também no `.env.local` para explicitar o que cada worker precisa.

Se precisar adicionar as variáveis do Gmail ao `.env.local`:

```bash
# Copia as 3 linhas do .env para o .env.local
grep -E "GOOGLE_MAIL_|GMAIL_REFRESH_TOKEN" .env >> .env.local
```

### Verificar se as keys do Supabase local ainda são válidas

As keys do Supabase local podem mudar após `supabase stop && supabase start`. Verifique:

```bash
supabase status -o env | grep -E "ANON_KEY|SERVICE_ROLE_KEY"
```

Compare com o que está em `.env.local`. Se forem diferentes, atualize:

```bash
# Pegar as keys atuais
NEW_ANON=$(supabase status -o env | grep ANON_KEY | cut -d= -f2)
NEW_SRK=$(supabase status -o env | grep SERVICE_ROLE_KEY | cut -d= -f2)
echo "ANON: $NEW_ANON"
echo "SRK:  $NEW_SRK"
# Depois edite .env.local manualmente com os valores acima
```

---

## 5. Passo 3 — Gravar token no Supabase Vault (local) {#5-vault-local}

> O vault local serve para testar a integração antes de ir para staging.

> ⚠️ **ATENÇÃO:** A assinatura é `vault.create_secret(valor_secreto, nome_da_chave, descrição)` — o **VALOR vem primeiro**, o nome da chave vem segundo! Inverter os argumentos grava o token como nome e o nome como valor.

Conecte via Supabase Studio (http://127.0.0.1:54323 → SQL Editor) ou psql:

```bash
psql postgresql://postgres:postgres@127.0.0.1:54322/postgres
```

Execute:

```sql
-- Habilitar vault se não estiver habilitado
CREATE EXTENSION IF NOT EXISTS supabase_vault;

-- Gravar os segredos do Gmail
-- Assinatura: vault.create_secret(VALOR, NOME, DESCRIÇÃO)

SELECT vault.create_secret(
  '<SEU_CLIENT_ID>',            -- valor secreto (1º arg)
  'google_oauth_client_id',     -- nome da chave (2º arg)
  'Google OAuth Client ID para Gmail API'
);

SELECT vault.create_secret(
  '<SEU_CLIENT_SECRET>',
  'google_oauth_client_secret',
  'Google OAuth Client Secret para Gmail API'
);

SELECT vault.create_secret(
  '<SEU_REFRESH_TOKEN>',
  'gmail_refresh_token',
  'Gmail OAuth Refresh Token — conta fran.fig@gmail.com'
);
```

**Verificar que foram gravados:**

```sql
SELECT name, description, created_at
FROM vault.decrypted_secrets
ORDER BY created_at DESC;
```

Saída esperada:

```
             name              |           description           |         created_at
-------------------------------+---------------------------------+----------------------------
 gmail_refresh_token           | Gmail OAuth Refresh Token - ... | 2026-03-25 ...
 google_oauth_client_secret    | Google OAuth Client Secret ...  | 2026-03-25 ...
 google_oauth_client_id        | Google OAuth Client ID ...      | 2026-03-25 ...
```

Para sair do psql: `\q`

### Alternativa via SQL Editor do Supabase Studio

Acesse http://127.0.0.1:54323 → SQL Editor e cole o SQL acima.

---

## 6. Passo 4 — Gravar token no Supabase Vault (staging) {#6-vault-staging}

> Necessário para que o scanner funcione em staging/produção via Edge Functions.

Acesse o SQL Editor do projeto staging:
**https://supabase.com/dashboard/project/dcljzgjgnkmxdvhybvpt/sql/new**

Execute o mesmo SQL do Passo 3 (com os valores reais):

```sql
-- Assinatura: vault.create_secret(VALOR, NOME, DESCRIÇÃO)
SELECT vault.create_secret(
  '<SEU_CLIENT_ID>',
  'google_oauth_client_id',
  'Google OAuth Client ID para Gmail API'
);

SELECT vault.create_secret(
  '<SEU_CLIENT_SECRET>',
  'google_oauth_client_secret',
  'Google OAuth Client Secret para Gmail API'
);

SELECT vault.create_secret(
  '<SEU_REFRESH_TOKEN>',
  'gmail_refresh_token',
  'Gmail OAuth Refresh Token — conta fran.fig@gmail.com'
);
```

**Verificar:**

```sql
SELECT name, description FROM vault.decrypted_secrets ORDER BY created_at DESC LIMIT 5;
```

---

## 7. Passo 5 — Executar o Gmail Scanner {#7-gmail-scanner}

### Modo dry-run (sem gravar nada — só inspeciona)

```bash
bun run workers/gmail-scanner/src/index.ts --dry-run --label Comprovantes --limit 5
```

### Scan real — pequeno lote inicial (recomendado para validar)

```bash
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 10
```

### Scan maior

```bash
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 100
```

### Scan completo (todos da label)

```bash
bun run workers/gmail-scanner/src/index.ts --label Comprovantes
```

### Capturar saída em arquivo

```bash
mkdir -p tmp
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 200 2>&1 | tee tmp/gmail-scan-$(date +%Y%m%d-%H%M%S).log
```

> A pasta `tmp/` está no `.gitignore` — os logs não serão commitados.

**Saída esperada:**

```
📋 Run criado: <uuid>
📨 [1] 24/03/2026 — NF Nubank
   De: noreply@nubank.com.br | Anexos: 1
  ✅ fatura_nubank_202603.pdf (120 KB)
...
Mensagens listadas: 10 | com anexos: 7 | enviados ao Storage: 7 | jobs criados: 7 | erros: 0
```

---

## 8. Passo 6 — Executar o worker de Ingestão {#8-ingestion-worker}

O worker processa os jobs criados pelo scanner (status `DISCOVERED`) e os leva até `DRAFT_GENERATED`.

### Executar uma vez (processa o que estiver na fila e para)

```bash
# Ctrl+C após processar — o worker fica em loop por padrão
bun run workers/ingestion/src/index.ts
```

> O worker faz polling a cada 5 segundos. Após processar todos os jobs pendentes, aguarda novas entradas. Use `Ctrl+C` quando a saída mostrar `[WORKER] Processing 0 jobs...` repetidamente.

### Variáveis opcionais para o worker

```bash
# Processar em lotes de 5 com polling a cada 2s
BATCH_SIZE=5 POLL_INTERVAL_MS=2000 bun run workers/ingestion/src/index.ts
```

**Saída esperada:**

```
[WORKER] Ingestion worker starting...
[WORKER] Poll interval: 5000ms, batch size: 10
[WORKER] Processing 7 jobs...
[JOB ff193df6] fatura_nubank_202603.pdf → HASHED
[JOB ff193df6] fatura_nubank_202603.pdf → PARSED
[JOB ff193df6] fatura_nubank_202603.pdf → DRAFT_GENERATED
...
[WORKER] Processing 0 jobs...
^C
[WORKER] SIGINT received, shutting down...
[WORKER] Worker stopped.
```

---

## 9. Passo 7 — Verificar resultados {#9-verificar}

### Via SQL (Supabase Studio ou SQL Editor do VS Code)

Acesse http://127.0.0.1:54323 → SQL Editor

```sql
-- Situação dos jobs
SELECT status, count(*) FROM ingestion_jobs GROUP BY status ORDER BY count DESC;

-- Últimos runs (colunas reais da tabela)
SELECT id, status, source_type, stats, created_at
FROM ingestion_runs
ORDER BY created_at DESC
LIMIT 5;

-- Documentos por tipo de arquivo
SELECT mime_type, count(*) as total, pg_size_pretty(sum(file_size_bytes)) as size
FROM source_documents GROUP BY mime_type ORDER BY total DESC;

-- Remetentes mais frequentes
SELECT gmail_from, count(*) FROM source_documents GROUP BY gmail_from ORDER BY count DESC LIMIT 10;
```

### Via Supabase Studio (local)

- URL: http://127.0.0.1:54323
- Tables: `ingestion_jobs`, `ingestion_runs`, `draft_batches`, `source_documents`
- Storage: bucket `ingestion-originals` → arquivos enviados

### Via MCP no VS Code Copilot

Pergunte ao Copilot (modo Agent):

```
Liste os rascunhos pendentes de aprovação
```

```
Aprove o batch <id>
```

---

## 10. Troubleshooting {#10-troubleshooting}

### ❌ `SUPABASE_SERVICE_ROLE_KEY is required`

A key não está no `.env.local`. Verifique e atualize:

```bash
supabase status -o env | grep SERVICE_ROLE_KEY
# Cole o valor no .env.local
```

### ❌ `No suitable key or wrong key type`

A key do `.env.local` está desatualizada (Supabase foi reiniciado). Mesma solução acima.

### ❌ `GMAIL_REFRESH_TOKEN is required`

Verifique se o token está no `.env`:

```bash
grep GMAIL_REFRESH_TOKEN .env
```

Se não estiver, siga o Passo 1 (seção 3.2).

### ❌ `Token has been expired or revoked`

O refresh token foi revogado. Gere um novo (Passo 1, seção 3.2).  
Antes de rodar, revogue o acesso antigo em https://myaccount.google.com/permissions → "Seu Bolso Feliz" → Remover acesso.

### ❌ `Label não encontrada`

```
❌ Label "Comprovantes" não encontrada
```

O nome da label é case-sensitive. Verifique exatamente como está no Gmail:

```bash
bun run workers/gmail-scanner/src/index.ts --dry-run --list-labels
```

### ❌ Vault `ERROR: function vault.create_secret does not exist`

A extensão não está habilitada:

```sql
CREATE EXTENSION IF NOT EXISTS supabase_vault;
-- Depois tente novamente
```

### ❌ Token duplicado no vault (`unique constraint`)

Se tentar criar o mesmo segredo duas vezes, use `update` em vez de `create`:

```sql
UPDATE vault.decrypted_secrets
SET secret = 'NOVO_VALOR'
WHERE name = 'gmail_refresh_token';
```

---

## Resumo dos comandos em sequência

```bash
# 1. Verificar token
grep GMAIL_REFRESH_TOKEN .env

# 2. Dry-run para validar
bun run workers/gmail-scanner/src/index.ts --dry-run --label Comprovantes --limit 3

# 3. Vault local (via Supabase Studio http://127.0.0.1:54323 → SQL Editor)
# SELECT vault.create_secret('<SEU_TOKEN>', 'gmail_refresh_token', 'Gmail Refresh Token');

# 4. Scan real (com log)
mkdir -p tmp
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 10 2>&1 | tee tmp/gmail-scan.log

# 5. Ingestão
bun run workers/ingestion/src/index.ts
# (Ctrl+C após processar)

# 6. Verificar (via Supabase Studio → SQL Editor)
# SELECT status, count(*) FROM ingestion_jobs GROUP BY status;
```

---

## 11. Resultados do primeiro scan real (2026-03-25) {#11-resultados-scan-real}

### Comando executado

```bash
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 200 2>&1 | tee tmp/gmail-scan-200-20260325-215718.log
```

### Resumo do Scan

| Métrica                    | Valor |
| -------------------------- | ----- |
| Mensagens listadas         | 200   |
| Mensagens com anexos       | 46    |
| Anexos encontrados         | 69    |
| Anexos enviados ao Storage | 55    |
| Jobs de ingestão criados   | 37    |
| Já processados (skip)      | 0     |
| Erros                      | 32    |

### Estado do banco após o scan

| Tabela             | Métrica           | Valor                     |
| ------------------ | ----------------- | ------------------------- |
| `ingestion_runs`   | Runs totais       | 3 (2 completed, 1 failed) |
| `source_documents` | Documentos        | 56 (todos `active`)       |
| `source_documents` | Tamanho total     | 3.4 MB                    |
| `source_documents` | Faixa de datas    | 21/01/2026 — 24/03/2026   |
| `ingestion_jobs`   | Jobs `discovered` | 56                        |

### Distribuição por tipo de arquivo

| Tipo                     | Qtd | Tamanho |
| ------------------------ | --- | ------- |
| application/pdf          | 33  | 2.6 MB  |
| application/octet-stream | 7   | 408 KB  |
| image/jpeg               | 6   | 276 KB  |
| text/xml                 | 6   | 49 KB   |
| application/xml          | 4   | 28 KB   |

### Top 10 remetentes

| Remetente                           | Anexos |
| ----------------------------------- | ------ |
| Canaã Telecom (faturamento.df)      | 12     |
| Francisco Figueiredo (caixa.gov.br) | 11     |
| NFe-cidades (noreply)               | 6      |
| GitHub (noreply)                    | 5      |
| Be Honest / HOPE                    | 4      |
| Canaã Telecom (contato)             | 3      |
| DigitalOcean Support                | 3      |
| Focus Gestão Condominial            | 3      |
| COOPERFORTE                         | 2      |
| Funcef (auto atendimento)           | 2      |

### Bugs encontrados e corrigidos

#### Bug 1: `invalid input syntax for type timestamp with time zone` (18 erros)

**Causa:** O header `Date` do e-mail vem em formato RFC 2822 com comentário entre parênteses:

```
Tue, 17 Mar 2026 17:06:02 +0000 (UTC)
Mon, 26 Jan 2026 04:53:01 -0800 (PST)
```

O PostgreSQL não aceita o sufixo `(UTC)`, `(PST)` etc no `TIMESTAMPTZ`.

**Correção:** Função `parseEmailDate()` adicionada em `workers/gmail-scanner/src/index.ts` que:

1. Remove o comentário entre parênteses
2. Converte para `Date` do JavaScript
3. Retorna ISO 8601 (`toISOString()`) que o PostgreSQL aceita

#### Bug 2: `Invalid key` no Storage (14 erros)

**Causa:** Nomes de arquivo com espaços e caracteres especiais:

```
Anexo sem título 12091.png
CAIXA RESIDENCIAL - Proposta de Seguro nº 80630750026831.pdf
```

O Supabase Storage rejeita keys com espaços e caracteres UTF-8 não-ASCII.

**Correção:** Função `sanitizeFilename()` adicionada que:

1. Remove acentos via `normalize("NFD")`
2. Substitui caracteres inválidos por `_`
3. Colapsa underscores consecutivos

#### Bug 3: Ordem dos argumentos do `vault.create_secret` na documentação

**Causa:** Docs 002 e 004 tinham a ordem invertida: `(nome, valor, descrição)` em vez de `(valor, nome, descrição)`.

**Correção:** Ambos os documentos foram atualizados com a assinatura correta.

### Próximo passo

Reexecutar o scan para processar as 32 mensagens que falharam (agora com os bugs corrigidos):

```bash
bun run workers/gmail-scanner/src/index.ts --label Comprovantes --limit 200 2>&1 | tee tmp/gmail-scan-200-retry.log
```

Depois rodar o worker de ingestão:

```bash
bun run workers/ingestion/src/index.ts
```
