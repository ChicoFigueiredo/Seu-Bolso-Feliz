# Guia Prático: Tecnologias do Ciclo de Ingestão, Automação, MCP e Agentes

> Este documento explica, de forma acessível, as tecnologias que serão usadas no próximo ciclo do Seu Bolso Feliz. O objetivo é que qualquer membro da equipe (ou o CEO) entenda **o que é**, **por que usar** e **como funciona** cada peça.

---

## Índice

1. [Vercel — Deploy e Hospedagem Web](#1-vercel)
2. [Supabase Vault — Cofre de Segredos](#2-vault)
3. [Supabase Queues — Filas de Trabalho](#3-queues)
4. [Supabase Cron — Tarefas Agendadas](#4-cron)
5. [Supabase Storage — Armazenamento de Arquivos](#5-storage)
6. [Edge Functions — Funções Serverless do Supabase](#6-edge-functions)
7. [MCP — Model Context Protocol](#7-mcp)
8. [OpenAI Responses API e Agents SDK](#8-openai)
9. [Gmail API e OAuth 2.0](#9-gmail)
10. [Workers — Processos em Background](#10-workers)
11. [Hashing e Fingerprinting — Deduplicação de Documentos](#11-hashing)
12. [Máquina de Estados — Controle de Fluxo de Ingestão](#12-state-machine)

---

## 1. Vercel — Deploy e Hospedagem Web {#1-vercel}

### O que é
Vercel é uma plataforma de hospedagem otimizada para frameworks como Next.js. Ela publica sua aplicação web automaticamente a cada push no repositório.

### Por que usar
- **Zero configuração de servidor** — não precisa gerenciar VPS, nginx, etc.
- **Deploy automático** — cada push no GitLab gera um deploy
- **Preview Deployments** — cada branch/MR ganha uma URL temporária para testar
- **CDN global** — conteúdo servido do ponto mais próximo ao usuário
- **Integração nativa com Next.js** — SSR, ISR, API routes funcionam sem ajustes
- **Gratuito** para projetos pessoais (plano Hobby)

### Como funciona
```
Push no GitLab → Vercel detecta → Build automático → Deploy
                                                      ↓
                                            URL de preview (branch)
                                            URL de produção (main)
```

- O Vercel clona o repositório, roda `bun install` e `next build`
- O resultado é distribuído em edge nodes ao redor do mundo
- As **environment variables** são injetadas durante o build (staging vs. production)

### Conceitos-chave
- **Production Deployment:** deploy da branch principal, servido no domínio real
- **Preview Deployment:** deploy de qualquer outra branch, com URL temporária
- **Environment Variables:** variáveis injetadas no build (ex: URL do Supabase)
- **Serverless Functions:** as API routes do Next.js rodam como funções serverless

---

## 2. Supabase Vault — Cofre de Segredos {#2-vault}

### O que é
O Vault é um cofre criptografado dentro do Supabase para guardar segredos: senhas, tokens, chaves de API. Os dados ficam criptografados em repouso e só podem ser lidos por funções que rodam no servidor (Edge Functions, SQL).

### Por que usar
- Senhas de PDFs, tokens do Gmail e chaves da OpenAI **não devem** ficar em tabelas comuns
- O Vault usa **criptografia transparente** — você grava e lê, mas o dado fica criptografado no disco
- Acessível via SQL, sem precisar de serviço externo

### Como funciona
```sql
-- Gravar um segredo
SELECT vault.create_secret('nome_do_segredo', 'valor_secreto', 'descrição');

-- Ler um segredo (somente disponível para service_role ou funções server-side)
SELECT * FROM vault.decrypted_secrets WHERE name = 'nome_do_segredo';
```

- O acesso à view `vault.decrypted_secrets` é restrito por **RLS** e **roles**
- Nunca acessível diretamente do browser/frontend
- Ideal para: tokens OAuth, senhas de PDFs, chaves de API

### Diferença entre Vault e a tabela user_secrets
O projeto já tem uma tabela `user_secrets` com criptografia via `pgcrypto`. O Vault do Supabase é uma camada adicional, nativa e gerenciada. A decisão de usar um ou outro será feita na implementação, mas ambos servem ao mesmo propósito: **guardar segredos de forma segura**.

---

## 3. Supabase Queues — Filas de Trabalho {#3-queues}

### O que é
Supabase Queues (via extensão `pgmq`) é um sistema de filas de mensagens **dentro do Postgres**. Permite enfileirar "jobs" para processamento assíncrono.

### Por que usar
- O pipeline de ingestão é **assíncrono**: escanear Gmail → enfileirar documentos → processar um a um
- Filas garantem que cada documento é processado **exatamente uma vez**
- Se o processamento falhar, a mensagem volta para a fila com contagem de retries
- Tudo dentro do Postgres — sem precisar de Redis, RabbitMQ ou SQS

### Como funciona
```
Worker de scan →  [Fila: ingestion_jobs]  → Worker de processamento
  (produz)           (armazena)              (consome)
```

```sql
-- Criar uma fila
SELECT pgmq.create('ingestion_jobs');

-- Enviar mensagem para a fila
SELECT pgmq.send('ingestion_jobs', '{"document_id": "abc", "action": "parse"}'::jsonb);

-- Consumir mensagem (reserva por 60 segundos)
SELECT * FROM pgmq.read('ingestion_jobs', 60, 1);

-- Confirmar processamento (remove da fila)
SELECT pgmq.delete('ingestion_jobs', <msg_id>);
```

### Conceitos-chave
- **Mensagem:** um job com payload JSON (ex: `{document_id, action}`)
- **Visibilidade:** uma mensagem consumida fica "invisível" por N segundos para evitar processamento duplicado
- **DLQ (Dead Letter Queue):** mensagens que falharam muitas vezes vão para uma fila separada para análise manual

---

## 4. Supabase Cron — Tarefas Agendadas {#4-cron}

### O que é
O Supabase Cron (via extensão `pg_cron`) permite agendar execução de funções SQL ou chamadas a Edge Functions em intervalos regulares, direto dentro do Postgres.

### Por que usar
- O scan do Gmail precisa rodar **periodicamente** (ex: a cada 15 minutos)
- Limpeza de jobs antigos e arquivamento devem ser automáticos
- Sem precisar de cron externo, servidor dedicado ou serviço de scheduling

### Como funciona
```sql
-- Agendar scan de Gmail a cada 15 minutos
SELECT cron.schedule(
  'gmail-scan',              -- nome do job
  '*/15 * * * *',            -- cron expression (a cada 15 min)
  $$SELECT net.http_post(
    'https://xxx.supabase.co/functions/v1/trigger-gmail-scan',
    '{}',
    'application/json',
    ARRAY[http_header('Authorization', 'Bearer ' || current_setting('app.service_role_key'))]
  )$$
);
```

### Cron Expressions (mini-guia)
| Expressão | Significado |
|-----------|-------------|
| `*/15 * * * *` | A cada 15 minutos |
| `0 * * * *` | A cada hora (no minuto 0) |
| `0 8 * * *` | Todo dia às 8h |
| `0 0 * * 1` | Toda segunda às 0h |
| `0 0 1 * *` | Todo dia 1 do mês |

---

## 5. Supabase Storage — Armazenamento de Arquivos {#5-storage}

### O que é
O Supabase Storage é um serviço de armazenamento de arquivos (similar ao S3 da AWS). Permite fazer upload/download de arquivos com controle de acesso via RLS.

### Por que usar
- Armazenar **anexos de e-mail**, **PDFs de faturas**, **comprovantes**, **documentos escaneados**
- Controle de acesso — cada usuário só vê seus próprios arquivos
- Integrado ao Supabase — sem precisar de bucket S3 separado
- Suporta políticas de acesso granulares

### Como funciona
```
Upload: App → Supabase Storage → /documents/user_id/arquivo.pdf
                                        ↓
                                  RLS verifica permissão
                                        ↓
                                  Arquivo armazenado com metadados
```

### Organização de Buckets

O Supabase Storage organiza arquivos em **buckets** (como pastas raiz):

| Bucket | Uso |
|--------|-----|
| `documents` | Documentos financeiros (faturas, comprovantes) |
| `attachments` | Anexos de e-mail brutos |
| `exports` | Relatórios e exportações do usuário |

Dentro de cada bucket, organizamos por `user_id/ano/mês/`:
```
documents/
  user_abc123/
    2026/
      03/
        fatura-nubank-mar-2026.pdf
        comprovante-pix-20260315.pdf
```

---

## 6. Edge Functions — Funções Serverless do Supabase {#6-edge-functions}

### O que é
Edge Functions são funções TypeScript/Deno que rodam nos servidores do Supabase. São equivalentes a "API routes" do Next.js, mas rodam no lado do Supabase.

### Por que usar
- Lógica que precisa acessar o banco **com privilégios elevados** (service_role)
- Operações que não devem passar pelo frontend
- Ponto de integração para webhooks, triggers e cron jobs
- O projeto já tem 2 Edge Functions: `merge-suppliers` e `retroactive-supplier-association`

### Como funciona
```typescript
// supabase/functions/minha-funcao/index.ts
import { serve } from "https://deno.land/std/http/server.ts";
import { createClient } from "https://esm.sh/@supabase/supabase-js";

serve(async (req) => {
  const supabase = createClient(
    Deno.env.get("SUPABASE_URL")!,
    Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!
  );
  
  // Lógica da função...
  
  return new Response(JSON.stringify({ ok: true }));
});
```

### Deploy
```bash
supabase functions deploy nome-da-funcao
```

---

## 7. MCP — Model Context Protocol {#7-mcp}

### O que é
MCP (Model Context Protocol) é um protocolo aberto criado pela Anthropic que permite a um assistente de IA (como o GitHub Copilot no VS Code) invocar **ferramentas externas** de forma estruturada. Pense nele como uma "API para o Copilot chamar suas funções".

### Por que usar
- Permite que o Copilot **interaja diretamente com o sistema** — consultar dados, criar registros, aprovar rascunhos
- Reduz atrito operacional: em vez de abrir o browser, navegar até o dashboard, encontrar o dado e colar no chat, o Copilot faz isso sozinho
- Transforma o Copilot em um **assistente financeiro integrado** ao projeto

### Como funciona

```
Você (no VS Code) → "Liste os drafts pendentes"
       ↓
Copilot Chat (modo agent)
       ↓
Identifica que precisa da tool "list_drafts"
       ↓
Envia request via stdio para o MCP Server local
       ↓
MCP Server (Bun) → consulta Supabase → retorna dados
       ↓
Copilot exibe resultado formatado
```

### Transporte stdio
O MCP usa **stdio** (standard input/output) — o VS Code inicia o processo do MCP server e se comunica por stdin/stdout em formato JSON-RPC. Não precisa de porta HTTP, não precisa de ngrok, não precisa de deploy.

### Exemplo de Tool MCP

```typescript
// Uma tool que lista drafts pendentes
server.tool("list_drafts", {
  description: "Lista rascunhos de transações pendentes de revisão",
  parameters: z.object({
    status: z.enum(["pending_review", "approved", "rejected"]).optional(),
    limit: z.number().default(20),
  }),
  handler: async ({ status, limit }) => {
    const { data } = await supabase
      .from("draft_records")
      .select("*")
      .eq("status", status ?? "pending_review")
      .limit(limit);
    return data;
  },
});
```

### Configuração no VS Code
O arquivo `.vscode/mcp.json` diz ao VS Code como iniciar o servidor:

```json
{
  "servers": {
    "sbf": {
      "type": "stdio",
      "command": "bun",
      "args": ["run", "apps/mcp-server/src/index.ts"]
    }
  }
}
```

### Exemplos de tools planejadas
| Tool | O que faz |
|------|-----------|
| `list_drafts` | Lista rascunhos pendentes |
| `approve_draft` | Aprova um rascunho e cria a transação |
| `reject_draft` | Rejeita um rascunho com justificativa |
| `scan_local_folder` | Varre uma pasta local e enfileira documentos |
| `query_financial_summary` | Consulta resumo financeiro de um período |
| `list_suppliers` | Lista fornecedores cadastrados |
| `suggest_category` | Sugere categoria/tags para uma transação |

---

## 8. OpenAI Responses API e Agents SDK {#8-openai}

### O que é
A OpenAI fornece modelos de linguagem (GPT-4o, GPT-4o-mini) acessíveis via API. A **Responses API** é a interface principal para enviar prompts e receber respostas. O **Agents SDK** é um framework para criar agentes que podem usar ferramentas.

### Por que usar
- **Classificação automática** de transações (categoria, tags)
- **Extração de dados** de documentos (leitura de PDFs, comprovantes)
- **Conciliação** de lançamentos (comparar dados importados com existentes)
- **Assistente conversacional** para o usuário (futuro)

### Conceitos-chave

#### Tokens
Tokens são pedaços de texto (~4 caracteres em inglês, ~3 em português). O custo da API é por token.

| Modelo | Custo Input | Custo Output | Uso recomendado |
|--------|-------------|--------------|-----------------|
| `gpt-4o-mini` | ~$0.15/1M | ~$0.60/1M | Classificação simples, tags |
| `gpt-4o` | ~$2.50/1M | ~$10/1M | Extração complexa, PDFs, visão |

#### Function Calling
O modelo pode "chamar funções" — na prática, ele retorna um JSON estruturado dizendo "quero chamar a função X com os parâmetros Y". Seu código então executa a função e retorna o resultado.

```
Prompt: "Classifique esta transação: PIX para Padaria Pão Quente, R$ 15,00"
                                    ↓
GPT-4o retorna: { "function": "classify_transaction",
                   "args": { "category": "alimentacao",
                             "tags": ["refeicao", "essencial"],
                             "priority": "media" }}
                                    ↓
Seu código executa a classificação e salva no banco
```

#### Agents SDK
O Agents SDK permite criar "agentes" que:
1. Recebem um objetivo
2. Decidem quais tools usar
3. Executam as tools
4. Avaliam o resultado
5. Repetem até completar

Para o Seu Bolso Feliz, o agente financeiro teria tools como:
- `extract_from_pdf` — extrair dados de um PDF
- `classify_transaction` — classificar uma transação
- `find_matching_supplier` — encontrar fornecedor correspondente
- `suggest_category` — sugerir categoria e tags

---

## 9. Gmail API e OAuth 2.0 {#9-gmail}

### O que é
A Gmail API permite ler e-mails programaticamente (sem precisar de IMAP). OAuth 2.0 é o protocolo de autorização usado para dar acesso seguro.

### Por que usar
- Muitos comprovantes, faturas e boletos chegam **por e-mail**
- A Gmail API é mais confiável e segura que IMAP
- OAuth garante acesso **somente leitura** sem expor sua senha do Google

### Como funciona o OAuth 2.0

```
1. Primeira vez (setup):

   Você → Abre URL de consentimento → Login Google → Autoriza → Código
                                                                  ↓
   Script troca código por: Access Token (15 min) + Refresh Token (permanente)
                                                                  ↓
   Refresh Token → armazenado no Vault do Supabase

2. Uso normal (automático):

   Worker → Pega Refresh Token do Vault
         → Troca por novo Access Token (15 min)
         → Usa Access Token para ler e-mails
         → Se Access Token expirar → Troca novamente (automático)
```

### Conceitos-chave
- **Access Token:** token temporário (~15 min) que autoriza chamadas à API
- **Refresh Token:** token "permanente" que permite gerar novos access tokens
- **Scopes:** permissões solicitadas (usaremos `gmail.readonly` — somente leitura)
- **Consent Screen:** tela onde você autoriza o app a acessar seu Gmail

### Segurança
- O app **nunca** tem acesso à sua senha do Google
- O escopo é **somente leitura** — não pode enviar, excluir ou modificar e-mails
- O refresh token pode ser **revogado** a qualquer momento em https://myaccount.google.com/permissions
- O refresh token fica criptografado no Vault do Supabase

---

## 10. Workers — Processos em Background {#10-workers}

### O que é
Workers são processos que rodam em background, consumindo jobs de uma fila. No nosso caso, são scripts Bun/TypeScript que processam documentos.

### Por que usar
- O processamento de documentos (download, parsing, extração) pode levar tempo
- Não queremos que o usuário fique esperando — o worker processa em background
- Se um job falhar, o worker pode tentar novamente (retry)
- Permite processar vários documentos em paralelo

### Arquitetura dos Workers

```
┌─────────────────┐     ┌───────────────┐     ┌─────────────────┐
│  Gmail Scanner  │────→│  Fila (pgmq)  │←────│  Local Scanner  │
│  (cron: 15min)  │     │               │     │  (MCP trigger)  │
└─────────────────┘     └───────┬───────┘     └─────────────────┘
                                │
                        ┌───────▼───────┐
                        │   Ingestion   │
                        │   Worker      │
                        │               │
                        │ 1. Download   │
                        │ 2. Hash       │
                        │ 3. Deduplica  │
                        │ 4. Parse      │
                        │ 5. Extrai     │
                        │ 6. Gera Draft │
                        └───────────────┘
```

### Modelo de Execução
- **Fase dev/MVP:** Worker roda localmente na máquina (`bun run workers/ingestion/src/index.ts`)
- **Fase produção:** Worker pode rodar como Edge Function invocada por cron, ou como serviço Bun em container
- **Concorrência:** Um worker consome um job por vez. Para mais paralelismo, sobe mais instâncias

---

## 11. Hashing e Fingerprinting — Deduplicação {#11-hashing}

### O que é
Hashing é uma técnica para gerar uma "impressão digital" única de um arquivo ou dado. Usamos para evitar processar o mesmo documento duas vezes.

### Por que usar
- O mesmo comprovante pode chegar por e-mail e ser importado manualmente
- Um scan periódico do Gmail vai reencontrar e-mails já processados
- Sem deduplicação, o sistema criaria transações duplicadas

### Estratégia de 3 Hashes

| Hash | O que identifica | Como é calculado |
|------|-----------------|------------------|
| **content_hash** | O arquivo exato (byte a byte) | SHA-256 do conteúdo do arquivo |
| **canonical_fingerprint** | O "documento lógico" (mesmo conteúdo em formatos diferentes) | SHA-256 do texto extraído, normalizado |
| **origin_key** | A origem específica (ex: "Gmail message abc, attachment 2") | Hash do identificador de origem |

### Exemplo prático

```
Fatura Nubank Março 2026 (PDF original por e-mail)
  → content_hash: sha256(bytes_do_pdf) = "abc123..."
  → canonical_fingerprint: sha256(normalize("Nubank Fatura Mar 2026 R$1234")) = "def456..."
  → origin_key: sha256("gmail:msg_id_xyz:attachment_0") = "ghi789..."

Mesma fatura Nubank (baixada do app e importada manualmente)
  → content_hash: sha256(bytes_do_pdf) = "xxx999..." (DIFERENTE — arquivo diferente)
  → canonical_fingerprint: sha256(normalize("Nubank Fatura Mar 2026 R$1234")) = "def456..." (IGUAL!)
  → origin_key: sha256("local:/docs/fatura-nubank.pdf") = "zzz111..." (DIFERENTE)
```

No exemplo acima, o `content_hash` é diferente (são PDFs gerados em momentos diferentes), mas o `canonical_fingerprint` é igual (o conteúdo financeiro é o mesmo). O sistema detecta a duplicata.

---

## 12. Máquina de Estados — Controle de Fluxo {#12-state-machine}

### O que é
Uma máquina de estados define os **estados possíveis** de um job de ingestão e as **transições válidas** entre eles. Impede que um documento "pule" etapas ou fique preso em estado inconsistente.

### Por que usar
- Um documento passa por várias etapas: download → hash → parse → extração → draft → revisão
- Cada etapa pode falhar e precisa de tratamento diferente
- A máquina de estados garante que o fluxo é previsível e auditável

### Estados do Pipeline de Ingestão

```
                                    ┌──────────┐
                                    │ received │ (entrada no sistema)
                                    └────┬─────┘
                                         ↓
                                ┌────────────────┐
                                │ queued_for_hash │
                                └───────┬────────┘
                                        ↓
                                ┌───────────────┐
                            ┌───│   hashing      │───┐
                            │   └───────────────┘   │
                            ↓                        ↓
                    ┌──────────────┐        ┌────────────┐
                    │  duplicate   │        │  hashed    │
                    │  (fim)       │        └─────┬──────┘
                    └──────────────┘              ↓
                                        ┌─────────────────┐
                                        │ queued_for_parse │
                                        └────────┬────────┘
                                                 ↓
                                        ┌──────────────┐
                                    ┌───│   parsing     │───┐
                                    │   └──────────────┘   │
                                    ↓                       ↓
                            ┌────────────┐          ┌────────────┐
                            │  parsed    │          │  failed    │
                            └─────┬──────┘          │  (retry?)  │
                                  ↓                 └────────────┘
                        ┌──────────────────┐
                        │ pending_review   │
                        └───────┬──────────┘
                           ↙         ↘
                    ┌──────────┐  ┌──────────┐
                    │ approved │  │ rejected │
                    └────┬─────┘  └──────────┘
                         ↓
                    ┌──────────┐
                    │ committed│ (transação criada)
                    └──────────┘
```

### Transições válidas
Cada transição é explícita — não existe "pular" do `received` direto para `approved`. Isso garante rastreabilidade e facilita debug quando algo dá errado.

### Estados de erro
- **failed:** o job falhou mas pode ser retentado (retry)
- **retryable:** falha temporária, será reenfileirado automaticamente
- **dead_letter:** falhou muitas vezes, precisa de intervenção manual

---

## Glossário Rápido

| Termo | Significado |
|-------|-------------|
| **Serverless** | Código que roda sem servidor dedicado — você paga por execução |
| **Edge** | Servidores distribuídos próximos ao usuário |
| **RLS** | Row Level Security — regras no banco que controlam quem vê o quê |
| **Bucket** | Container de arquivos no Storage |
| **pgmq** | Extensão Postgres para filas de mensagens |
| **pg_cron** | Extensão Postgres para agendamento de tarefas |
| **stdio** | Standard Input/Output — comunicação por texto via terminal |
| **JSON-RPC** | Protocolo de chamada remota usando JSON |
| **SHA-256** | Algoritmo de hash criptográfico (gera string de 64 caracteres) |
| **DLQ** | Dead Letter Queue — fila de mensagens que falharam |
| **SSR** | Server-Side Rendering — página renderizada no servidor |
| **CDN** | Content Delivery Network — rede de cache global |
