# Guia Unificado de Operações — CEO

> Este guia consolida todas as ações que o CEO precisa executar manualmente para que o sistema funcione em staging e produção. Referencia os guias detalhados existentes e adiciona as ações novas do plano de ingestão + IA.

---

## Índice de Ações Manuais

| #   | Ação                                            | Status      | Guia Detalhado                                             |
| --- | ----------------------------------------------- | ----------- | ---------------------------------------------------------- |
| 1   | Criar projetos Supabase (staging + prod)        | ✅ Feito    | [001](001-supabase.e.outros.md)                            |
| 2   | Criar conta Vercel + integrar GitLab            | ✅ Feito    | [002](002-configuracoes-manuais-ingestao-automacao-mcp.md) |
| 3   | Registrar domínio seubolsofeliz.com.br          | ✅ Feito    | [002](002-configuracoes-manuais-ingestao-automacao-mcp.md) |
| 4   | Configurar Google Cloud Console + Gmail API     | ✅ Feito    | [002](002-configuracoes-manuais-ingestao-automacao-mcp.md) |
| 5   | Criar labels no Gmail                           | ✅ Feito    | [002](002-configuracoes-manuais-ingestao-automacao-mcp.md) |
| 6   | Configurar variáveis protegidas no GitLab       | ✅ Feito    | [002](002-configuracoes-manuais-ingestao-automacao-mcp.md) |
| 7   | **Configurar Google OAuth no Supabase Staging** | 🔴 PENDENTE | Seção abaixo                                               |
| 8   | **Obter API key OpenAI**                        | 🔴 PENDENTE | Seção abaixo                                               |
| 9   | Configurar env vars no Vercel                   | 🔴 PENDENTE | Seção abaixo                                               |
| 10  | Configurar domínio no Vercel                    | 🔴 PENDENTE | Seção abaixo                                               |

---

## Ação 7 — Configurar Google OAuth no Supabase Staging

**Prioridade:** 🔴 BLOQUEADOR — Ninguém faz login sem isso.

### Passos

1. Acesse o [Supabase Dashboard](https://supabase.com/dashboard) → Projeto Staging (dcljzgjgnkmxdvhybvpt)
2. Vá em **Authentication → Providers → Google**
3. Ative o provider Google
4. No Google Cloud Console:
   - Vá em **APIs & Services → Credentials**
   - Crie ou edite um **OAuth 2.0 Client ID** (Web application)
   - Adicione como **Authorized redirect URI**: `https://dcljzgjgnkmxdvhybvpt.supabase.co/auth/v1/callback`
   - Copie o **Client ID** e **Client Secret**
5. Cole o Client ID e Client Secret no Supabase Dashboard (Provider Google)
6. Salve

### Validação

- Abra a URL de staging → Clique em "Login com Google" → Deve redirecionar para Google e voltar autenticado.

---

## Ação 8 — Obter API Key OpenAI

**Prioridade:** 🟡 Necessário antes do Marco 4 (IA)

### Passos

1. Acesse [platform.openai.com](https://platform.openai.com)
2. Crie conta ou faça login
3. Vá em **API Keys → Create new secret key**
4. Nomeie como `seubolsofeliz-staging`
5. Copie a key (começa com `sk-...`)
6. Configure limite de gasto mensal em **Settings → Billing → Usage limits** (sugestão: US$ 20/mês limite hard)
7. Adicione ao `.env.local`:
   ```
   OPENAI_API_KEY=sk-...
   ```

### Segurança

- **Nunca** commite a key no git
- A key será configurada como env var protegida no Vercel (Ação 9)
- Em produção, usar Supabase Vault para a key

---

## Ação 9 — Configurar Variáveis de Ambiente no Vercel

**Prioridade:** 🔴 BLOQUEADOR — Deploy não funciona sem isso.

### Passos

1. Acesse [vercel.com/dashboard](https://vercel.com/dashboard)
2. Selecione o projeto `seu-bolso-feliz`
3. Vá em **Settings → Environment Variables**
4. Adicione as seguintes variáveis para o ambiente **Preview** (staging):

| Variável                        | Valor                                           | Escopo  |
| ------------------------------- | ----------------------------------------------- | ------- |
| `NEXT_PUBLIC_SUPABASE_URL`      | `https://dcljzgjgnkmxdvhybvpt.supabase.co`      | Preview |
| `NEXT_PUBLIC_SUPABASE_ANON_KEY` | (copiar do Supabase Dashboard → Settings → API) | Preview |
| `SUPABASE_SERVICE_ROLE_KEY`     | (copiar do Supabase Dashboard → Settings → API) | Preview |
| `OPENAI_API_KEY`                | `sk-...` (quando disponível)                    | Preview |

5. Para **Production**, repita com os dados do projeto de produção (opwelsgdhksuuewdbefk)

### Validação

- Faça push para a branch de staging → Vercel deve buildar com sucesso → Acesse a URL de preview.

---

## Ação 10 — Configurar Domínio Customizado no Vercel

**Prioridade:** 🟡 Pode ser feito após deploy funcionar.

### Passos

1. No Vercel → projeto → **Settings → Domains**
2. Adicione `seubolsofeliz.com.br`
3. O Vercel vai mostrar registros DNS necessários (geralmente CNAME ou A)
4. No registrador do domínio, configure os registros DNS apontados
5. Aguarde propagação (até 48h, geralmente minutos)

### Validação

- Acesse `https://seubolsofeliz.com.br` → Deve mostrar o app.

---

## Operações Recorrentes

### Rodar Gmail Scanner Localmente

```bash
cd workers/gmail-scanner
cp ../../secrets/client_secret_*.json ./credentials.json
bun run src/index.ts --label "SBF/Contas" --dry-run
# Se satisfeito, sem --dry-run:
bun run src/index.ts --label "SBF/Contas"
```

Guia completo: [004-fluxo-completo-gmail-vault-ingestao.md](004-fluxo-completo-gmail-vault-ingestao.md)

### Rodar Ingestion Worker Localmente

```bash
cd workers/ingestion
bun run src/index.ts
# Processa documentos com status QUEUED
```

### Rodar Local Scanner

```bash
cd workers/local-scanner
bun run src/index.ts --dir ~/Documentos/contas --extensions pdf,png,jpg
```

### Usar MCP Server

```bash
cd apps/mcp-server
bun run src/index.ts
# Disponibiliza 8 tools para agentes IA
```

Guia de configuração: [002](002-configuracoes-manuais-ingestao-automacao-mcp.md) → seção MCP

### Ter Dados no Dashboard Local

Guia completo: [005-como-ter-dados-no-dashboard-local.md](005-como-ter-dados-no-dashboard-local.md)

---

## Referências Cruzadas

| Assunto                          | Documento                                                                                                                                                    |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Setup Supabase completo          | [001](001-supabase.e.outros.md)                                                                                                                              |
| Configurações manuais detalhadas | [002](002-configuracoes-manuais-ingestao-automacao-mcp.md)                                                                                                   |
| Próximas ações CEO (original)    | [003](003-proximas-acoes-manuais-ceo.md)                                                                                                                     |
| Fluxo Gmail → Vault → Ingestão   | [004](004-fluxo-completo-gmail-vault-ingestao.md)                                                                                                            |
| Dashboard local com dados        | [005](005-como-ter-dados-no-dashboard-local.md)                                                                                                              |
| Plano de ação completo           | [../refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md](../refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md) |
| Checklist de tarefas             | [../checklists/005-checklist-implementacao-ingestao-ia-staging.md](../checklists/005-checklist-implementacao-ingestao-ia-staging.md)                         |
