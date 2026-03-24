# 002 — Passo a Passo: Configurações Manuais para Ingestão, Automação e MCP

> Guia operacional detalhado de tudo que o CEO/dono do projeto precisa fazer manualmente para viabilizar o ciclo de ingestão, automação, MCP e agentes.  
> Cada seção tem pré-requisitos, passos numerados e verificação.

---

## Índice

1. [Vercel — Criação de Conta e Configuração](#1-vercel)
2. [Supabase — Projetos Staging e Production](#2-supabase-staging-e-production)
3. [GitLab — Variáveis de Ambiente Protegidas](#3-gitlab-variaveis)
4. [Domínio — Registro e DNS](#4-dominio)
5. [Google Cloud — Projeto e Gmail API](#5-google-cloud)
6. [Gmail — Labels e Preparação de Dados](#6-gmail-labels)
7. [OpenAI — API Key e Budget](#7-openai)
8. [Supabase Vault — Armazenamento de Segredos](#8-vault)
9. [VS Code — Configuração do MCP Local](#9-vscode-mcp)

---

## 1. Vercel — Criação de Conta e Configuração {#1-vercel}

### Pré-requisitos
- E-mail válido
- Acesso ao repositório GitLab do projeto
- Instalar Vercel CLI (opcional, para deploy via CLI)
- Instalar Supabase CLI (para aplicar migrations via CLI, opcional mas recomendado)

```bash
bun install -g supabase vercel
```

### Passos

#### 1.1 Criar conta no Vercel
1. Acesse https://vercel.com/signup
2. Crie conta com e-mail (não vincular ao GitHub — usaremos GitLab)
3. Escolha o plano **Hobby** (gratuito) ou **Pro** ($20/mês — recomendado para domínio custom e limites maiores)
4. Anote:
   - **Vercel Account/Org ID** — visível em Settings → General → "Your ID"

#### 1.2 Conectar ao GitLab
1. No dashboard Vercel, clique **"Add New Project"**
2. Em "Import Git Repository", selecione **"GitLab"**
3. Autorize o Vercel a acessar seu GitLab
4. Selecione o repositório `seu.bolso.feliz`
5. Configure:
   - **Root Directory:** `apps/web`
   - **Framework Preset:** Next.js
   - **Build Command:** `bun run build` (ou `next build` — confirmar com o time)
   - **Output Directory:** `.next`
   - **Install Command:** `bun install`
6. **NÃO faça deploy ainda** — primeiro configure as variáveis de ambiente
7. Anote:
   - **Vercel Project ID** — visível em Project Settings → General

> **Alternativa (se GitLab direto não funcionar):**  
> Use o Vercel CLI no pipeline do GitLab:
> ```bash
> npm i -g vercel
> vercel --token=$VERCEL_TOKEN --prod
> ```
> Nesse caso, o deploy é feito pelo pipeline, não pelo Vercel diretamente.

#### 1.3 Configurar Environment Variables no Vercel
1. Em Project Settings → Environment Variables
2. Adicione para cada ambiente (**Preview**, **Production**):

| Variável | Preview (staging) | Production |
|----------|-------------------|------------|
| `NEXT_PUBLIC_SUPABASE_URL` | URL do projeto staging | URL do projeto production |
| `NEXT_PUBLIC_SUPABASE_ANON_KEY` | Anon key staging | Anon key production |
| `SUPABASE_SERVICE_ROLE_KEY` | Service role staging | Service role production |

3. Marque service_role_key como **Sensitive** (não visível no log de build)

#### 1.4 Configurar Preview Deployments
1. Em Project Settings → Git
2. Certifique-se de que **"Automatically deploy on push"** está ativado
3. Confirme que branches de MR/PR geram preview deployments

### Verificação
- [ ] Conta Vercel criada
- [ ] Projeto criado e conectado ao GitLab
- [ ] Root directory configurado como `apps/web`
- [ ] Environment variables configuradas
- [ ] Anotados: Vercel Account ID, Project ID

---

## 2. Supabase — Projetos Staging e Production {#2-supabase-staging-e-production}

### Pré-requisitos
- Conta Supabase (já existente para ambiente local)

### Passos

#### 2.1 Criar Projeto STAGING
1. Acesse https://supabase.com/dashboard
2. Clique **"New Project"**
3. Configure:
   - **Name:** `seu-bolso-feliz-staging`
   - **Database Password:** Gere uma senha forte (anote-a!)
   - **Region:** Selecione a região mais próxima (ex: `South America (São Paulo)`)
   - **Pricing Plan:** Free tier (suficiente para staging)
4. Aguarde a criação (pode levar 1-2 minutos)
5. Anote:
   - **Project URL** (ex: `https://xxxxx.supabase.co`)
   - **Anon Key** (Settings → API → `anon` `public`)
   - **Service Role Key** (Settings → API → `service_role` — **NUNCA exponha publicamente**)
   - **Database Password** (a que você definiu)
   - **Project ID** (visível na URL: `supabase.com/dashboard/project/XXXXX`)

#### 2.2 Criar Projeto PRODUCTION
1. Repita os mesmos passos com:
   - **Name:** `seu-bolso-feliz-production`
   - **Database Password:** Senha diferente da staging!
   - **Region:** Mesma região da staging
   - **Pricing Plan:** Free tier (migrar para Pro quando necessário — $25/mês)
2. Anote as mesmas informações

#### 2.3 Aplicar Migrations (via CLI local)

> **ATENÇÃO — Senha do banco vs. outras credenciais**
>
> O `supabase db push` conecta **diretamente ao PostgreSQL** do projeto remoto.  
> Ele precisa da **senha do banco de dados** — aquela que você digitou ao criar o projeto.  
> Essa senha é **diferente** de: anon key, service role key, access token e JWT secret.
>
> Se você não lembrar a senha, vá em: Dashboard → Project Settings → Database → **"Reset database password"** → defina uma nova → anote.
>
> O CLI **não lê** o `.env.local` automaticamente. Use a flag `--password` no `supabase link`.

```bash
# Staging
cd /home/chico/dev/Chico/seu.bolso.feliz

# Linkar ao projeto staging (--password é a senha do PostgreSQL, não anon/service key)
supabase link --project-ref <STAGING_PROJECT_ID> --password <SENHA_DO_BANCO_STAGING>

# Aplicar migrations
supabase db push

# Deploy Edge Functions
supabase functions deploy merge-suppliers
supabase functions deploy retroactive-supplier-association

# Verificar
supabase db diff  # deve retornar vazio (sem diferenças)
```

```bash
# Production (repetir com project-ref de produção)
supabase link --project-ref <PRODUCTION_PROJECT_ID> --password <SENHA_DO_BANCO_PRODUCTION>
supabase db push
supabase functions deploy merge-suppliers
supabase functions deploy retroactive-supplier-association
```

> **IMPORTANTE:** Ao trocar entre projetos com `supabase link`, o Supabase CLI atualiza o `.env` local. Cuidado para não misturar credenciais.

#### 2.4 Habilitar Vault (para segredos)
1. No Dashboard Supabase → Settings → Vault
2. Verifique se o Vault está habilitado (em projetos novos geralmente já vem)
3. Se não estiver disponível, a extensão `supabase_vault` precisa ser habilitada:
   ```sql
   CREATE EXTENSION IF NOT EXISTS supabase_vault;
   ```

### Verificação
- [ ] Projeto staging criado e acessível
- [ ] Projeto production criado e acessível
- [ ] Senhas dos bancos anotadas em local seguro
- [ ] URLs e chaves anotadas
- [ ] Migrations aplicadas nos dois projetos
- [ ] Edge Functions deployadas nos dois projetos
- [ ] Vault habilitado nos dois projetos

---

## 3. GitLab — Variáveis de Ambiente Protegidas {#3-gitlab-variaveis}

### Pré-requisitos
- Projetos Supabase staging e production criados
- Conta Vercel configurada

### Passos

#### 3.1 Configurar Variáveis no GitLab
1. Acesse o repositório no GitLab → Settings → CI/CD → Variables
2. Adicione as seguintes variáveis:

**Para ambiente STAGING (Environment scope: `staging`)**:

| Variável | Valor | Protegida | Masked |
|----------|-------|-----------|--------|
| `SUPABASE_URL` | URL do projeto staging | ✅ | ❌ |
| `SUPABASE_ANON_KEY` | Anon key staging | ✅ | ✅ |
| `SUPABASE_SERVICE_ROLE_KEY` | Service role staging | ✅ | ✅ |
| `SUPABASE_DB_PASSWORD` | Senha do banco staging | ✅ | ✅ |
| `SUPABASE_PROJECT_ID` | Project ID staging | ✅ | ❌ |
| `SUPABASE_ACCESS_TOKEN` | Token pessoal Supabase CLI | ✅ | ✅ |

**Para ambiente PRODUCTION (Environment scope: `production`)**:

| Variável | Valor | Protegida | Masked |
|----------|-------|-----------|--------|
| `SUPABASE_URL` | URL do projeto production | ✅ | ❌ |
| `SUPABASE_ANON_KEY` | Anon key production | ✅ | ✅ |
| `SUPABASE_SERVICE_ROLE_KEY` | Service role production | ✅ | ✅ |
| `SUPABASE_DB_PASSWORD` | Senha do banco production | ✅ | ✅ |
| `SUPABASE_PROJECT_ID` | Project ID production | ✅ | ❌ |

**Para TODOS os ambientes (Environment scope: `*`)**:

| Variável | Valor | Protegida | Masked |
|----------|-------|-----------|--------|
| `VERCEL_TOKEN` | Token da conta Vercel | ✅ | ✅ |
| `VERCEL_ORG_ID` | Org/Account ID | ✅ | ❌ |
| `VERCEL_PROJECT_ID` | Project ID | ✅ | ❌ |

#### 3.2 Gerar Token de Acesso Supabase CLI
1. Acesse https://supabase.com/dashboard/account/tokens
2. Clique "Generate new token"
3. Nome: `gitlab-ci`
4. Copie o token gerado (só aparece uma vez!)
5. Use como `SUPABASE_ACCESS_TOKEN` no GitLab

#### 3.3 Gerar Token Vercel
1. Acesse https://vercel.com/account/tokens
2. Clique "Create Token"
3. Nome: `gitlab-ci`
4. Scope: Full Account
5. Copie o token
6. Use como `VERCEL_TOKEN` no GitLab

### Verificação
- [ ] Todas as variáveis configuradas por ambiente
- [ ] Variáveis sensíveis marcadas como Masked e Protected
- [ ] Token Supabase CLI gerado e configurado
- [ ] Token Vercel gerado e configurado
- [ ] Pipeline de CI/CD roda sem erro de variáveis faltantes

---

## 4. Domínio — Registro e DNS {#4-dominio}

### Pré-requisitos
- Conta Vercel com projeto configurado

### Passos

#### 4.1 Registrar Domínio
1. Acesse https://registro.br (para domínio `.com.br`)
2. Busque `seubolsofeliz.com.br`
3. Registre (custo: ~R$ 40/ano)
4. Aguarde propagação (pode levar até 24h)

> **Alternativa:** Se preferir outro TLD, use qualquer registrador (Cloudflare, Namecheap, etc.) para `.com`, `.dev`, `.app`, etc.

#### 4.2 Configurar DNS no Vercel
1. No Vercel, vá em Project Settings → Domains
2. Adicione `seubolsofeliz.com.br`
3. O Vercel mostrará os registros DNS necessários (geralmente CNAME ou A record)
4. No painel do registro.br (ou seu registrador):
   - Adicione o CNAME ou A record conforme instruções do Vercel
   - Exemplo: `CNAME @ cname.vercel-dns.com`
5. Aguarde propagação DNS
6. O Vercel gera certificado SSL automaticamente

#### 4.3 (Opcional) Configurar subdomínio para staging
- `staging.seubolsofeliz.com.br` apontando para o preview deployment do Vercel

### Verificação
- [ ] Domínio registrado
- [ ] DNS configurado e propagado
- [ ] HTTPS funcionando
- [ ] Site acessível via domínio customizado

---

## 5. Google Cloud — Projeto e Gmail API {#5-google-cloud}

### Pré-requisitos
- Conta Google (a mesma do Gmail que será lido)

### Passos

#### 5.1 Criar Projeto no Google Cloud
1. Acesse https://console.cloud.google.com
2. Clique no seletor de projetos (topo da página) → **"New Project"**
3. Configure:
   - **Project name:** `Seu Bolso Feliz`
   - **Organization:** Deixe vazio (pessoal)
4. Clique **"Create"**
5. Selecione o projeto recém-criado

#### 5.2 Habilitar Gmail API
1. No menu lateral → **APIs & Services** → **Library**
2. Busque **"Gmail API"**
3. Clique na Gmail API → **"Enable"**

#### 5.3 Configurar OAuth Consent Screen
1. No menu lateral → **APIs & Services** → **OAuth consent screen**
2. Selecione **"External"** (se for conta pessoal) ou **"Internal"** (se for Google Workspace)
3. Preencha:
   - **App name:** `Seu Bolso Feliz`
   - **User support email:** seu e-mail
   - **Developer contact info:** seu e-mail
4. Clique **"Save and Continue"**
5. Em **Scopes**, clique **"Add or Remove Scopes"**:
   - Busque e marque: `https://www.googleapis.com/auth/gmail.readonly`
   - Clique **"Update"** → **"Save and Continue"**
6. Em **Test users** (se External):
   - Clique **"Add Users"**
   - Adicione o e-mail da conta Gmail que será lida
   - Clique **"Save and Continue"**
7. Revise e clique **"Back to Dashboard"**

> **IMPORTANTE:** Em modo External/Testing, você terá limite de 100 test users. Para produção futura, seria necessário publicar o app e passar por verificação do Google. Para uso pessoal, o modo teste é suficiente.

#### 5.4 Criar Credenciais OAuth
1. No menu lateral → **APIs & Services** → **Credentials**
2. Clique **"Create Credentials"** → **"OAuth client ID"**
3. Configure:
   - **Application type:** "Web application"
   - **Name:** `Seu Bolso Feliz - Local`
   - **Authorized redirect URIs:**
     - `http://localhost:3000/api/auth/callback/google` (para dev local via web)
     - `http://localhost:8080/oauth/callback` (para worker local)
     - `https://seubolsofeliz.com.br/api/auth/callback/google` (para produção — adicionar depois)
4. Clique **"Create"**
5. **ANOTE IMEDIATAMENTE:**
   - **Client ID** (ex: `123456-xxxxx.apps.googleusercontent.com`)
   - **Client Secret** (ex: `GOCSPX-xxxxx`)

> **SEGURANÇA:** O Client Secret é sensível. Guarde em local seguro (gerenciador de senhas).

#### 5.5 Gerar Primeiro Refresh Token

O refresh token é necessário para que o worker acesse o Gmail sem interação do usuário.

**Método via script local:**

O time vai disponibilizar um script de autenticação. O fluxo será:

1. Executar o script localmente:
   ```bash
   bun run workers/gmail-scanner/src/auth.ts
   ```
2. O script abrirá o navegador para a tela de consentimento do Google
3. Faça login com a conta Gmail desejada
4. Autorize o acesso (escopo `gmail.readonly`)
5. O script receberá o authorization code e trocará por tokens
6. O script exibirá o **refresh token** no terminal
7. **ANOTE o refresh token** — ele será armazenado no Supabase Vault

> **IMPORTANTE:** O refresh token é como uma "chave permanente" para o seu Gmail (somente leitura). Guarde com extremo cuidado.

### Verificação
- [ ] Projeto Google Cloud criado
- [ ] Gmail API habilitada
- [ ] OAuth consent screen configurada
- [ ] Credenciais OAuth criadas (Client ID + Secret anotados)
- [ ] Redirect URIs configuradas
- [ ] Refresh token gerado

---

## 6. Gmail — Labels e Preparação de Dados {#6-gmail-labels}

### Passos

#### 6.1 Criar Labels para Organização
1. No Gmail, crie as seguintes labels (ou ajuste conforme seu padrão):
   - `Comprovantes` — comprovantes de pagamento
   - `Faturas` — faturas de cartão, boletos
   - `Contas` — contas de concessionárias
   - `Financeiro` — label pai para organizar (opcional)

#### 6.2 Preparar Dados de Teste
1. Mova para as labels criadas pelo menos:
   - 3-5 comprovantes de pagamento (transferência, PIX)
   - 3-5 faturas de cartão de crédito
   - 3-5 boletos/contas de concessionárias
   - Pelo menos 1 PDF protegido por senha (ex: contracheque, FGTS)
2. Isso permitirá o primeiro teste real do pipeline

### Verificação
- [ ] Labels criadas no Gmail
- [ ] Pelo menos 10-15 e-mails com anexos nas labels de teste
- [ ] Pelo menos 1 PDF protegido por senha identificado

---

## 7. OpenAI — API Key e Budget {#7-openai}

### Pré-requisitos
- Necessário apenas na **Fase 7** — pode ser feito depois

### Passos

#### 7.1 Criar Conta/API Key
1. Acesse https://platform.openai.com
2. Crie conta ou faça login
3. Vá em **API Keys** → **"Create new secret key"**
4. Nome: `seu-bolso-feliz`
5. **ANOTE a key** (só aparece uma vez!)

#### 7.2 Configurar Budget
1. Vá em **Settings** → **Billing** → **Usage Limits**
2. Configure um **hard limit** mensal (sugestão inicial: $20-50/mês)
3. Configure **alertas** de uso (50%, 80%, 90%)
4. Adicione um meio de pagamento

#### 7.3 Escolher Modelo
- **Para desenvolvimento/teste:** `gpt-4o-mini` (~$0.15/1M input tokens) — barato e rápido
- **Para produção/parsing complexo:** `gpt-4o` (~$2.50/1M input tokens) — mais preciso
- **Para visão/imagens:** `gpt-4o` com vision — para leitura de documentos escaneados

### Verificação
- [ ] API key criada e anotada em local seguro
- [ ] Budget configurado com limite
- [ ] Alertas de uso configurados
- [ ] Meio de pagamento adicionado

---

## 8. Supabase Vault — Armazenamento de Segredos {#8-vault}

### Pré-requisitos
- Projetos Supabase staging/production com Vault habilitado
- Credenciais do Google Cloud e OpenAI em mãos

### Passos

#### 8.1 Armazenar Segredos do Gmail
Via SQL (no Supabase SQL Editor):

```sql
-- Client ID do Google OAuth
SELECT vault.create_secret(
  'google_oauth_client_id',
  '<SEU_CLIENT_ID>',
  'Google OAuth Client ID para Gmail API'
);

-- Client Secret do Google OAuth
SELECT vault.create_secret(
  'google_oauth_client_secret',
  '<SEU_CLIENT_SECRET>',
  'Google OAuth Client Secret para Gmail API'
);

-- Refresh Token do Gmail
SELECT vault.create_secret(
  'gmail_refresh_token',
  '<SEU_REFRESH_TOKEN>',
  'Gmail OAuth Refresh Token para leitura de e-mails'
);
```

#### 8.2 Armazenar Segredos do OpenAI (quando necessário)

```sql
SELECT vault.create_secret(
  'openai_api_key',
  '<SUA_API_KEY>',
  'OpenAI API Key para extração e classificação assistida'
);
```

#### 8.3 Armazenar Senhas de PDFs
Para cada fornecedor/tipo de documento protegido por senha:

```sql
-- Exemplo: Informe de rendimentos do banco
SELECT vault.create_secret(
  'pdf_password_banco_x_informe',
  '<SENHA_DO_PDF>',
  'Senha do PDF de informe de rendimentos do Banco X'
);
```

Ou, via tabela `user_secrets` existente (que já tem criptografia pgcrypto):
```sql
INSERT INTO user_secrets (user_id, name, type, encrypted_value, metadata)
VALUES (
  '<SEU_USER_ID>',
  'Senha PDF CEMIG',
  'pdf_password',
  encrypt_secret('<SENHA>'),
  '{"supplier_pattern": "cemig", "document_type": "conta_luz"}'::jsonb
);
```

> **NOTA:** O mecanismo exato (Vault vs. user_secrets) será definido na implementação. Ambos são seguros com criptografia.

### Verificação
- [ ] Segredos do Gmail armazenados no Vault
- [ ] Segredos do OpenAI armazenados (quando necessário)
- [ ] Senhas de PDFs armazenadas
- [ ] Verificado que os segredos podem ser lidos via SQL (`vault.decrypted_secrets`)

---

## 9. VS Code — Configuração do MCP Local {#9-vscode-mcp}

### Pré-requisitos
- Necessário apenas na **Fase 5**
- VS Code com extensão GitHub Copilot Chat instalada
- MCP server implementado em `apps/mcp-server/`

### Passos

#### 9.1 Configurar `.env` Local para MCP
Crie ou atualize o `.env` na raiz do projeto:

```env
# Supabase Local (desenvolvimento)
SUPABASE_URL=http://localhost:54321
SUPABASE_SERVICE_ROLE_KEY=<sua_service_role_key_local>
SUPABASE_ANON_KEY=<sua_anon_key_local>

# Gmail (para workers)
GOOGLE_OAUTH_CLIENT_ID=<client_id>
GOOGLE_OAUTH_CLIENT_SECRET=<client_secret>
GMAIL_REFRESH_TOKEN=<refresh_token>

# OpenAI (quando necessário)
OPENAI_API_KEY=<api_key>
```

#### 9.2 Configurar MCP no VS Code
O time criará o arquivo `.vscode/mcp.json`:

```jsonc
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

#### 9.3 Testar o MCP
1. Abra o VS Code no projeto
2. Abra o Copilot Chat (Ctrl+Shift+I ou Cmd+Shift+I)
3. Use o modo Agent (@workspace)
4. Digite: "Liste os drafts pendentes" ou "Varra a pasta de documentos locais"
5. O Copilot deve invocar as tools do MCP automaticamente

### Verificação
- [ ] `.env` configurado com credenciais locais
- [ ] MCP server roda localmente sem erros
- [ ] Copilot no VS Code lista as tools do MCP
- [ ] Pelo menos uma tool pode ser invocada com sucesso

---

## Resumo: Ordem das Ações Manuais do CEO

| Ordem | Ação | Fase | Urgência |
|-------|------|------|----------|
| 1 | Criar conta Vercel | F1 | ⚡ Bloqueante |
| 2 | Criar projeto Supabase STAGING | F1 | ⚡ Bloqueante |
| 3 | Criar projeto Supabase PRODUCTION | F1 | ⚡ Bloqueante |
| 4 | Anotar URLs e chaves dos projetos Supabase | F1 | ⚡ Bloqueante |
| 5 | Conectar GitLab ao Vercel | F1 | ⚡ Bloqueante |
| 6 | Gerar token Supabase CLI | F1 | ⚡ Bloqueante |
| 7 | Gerar token Vercel | F1 | ⚡ Bloqueante |
| 8 | Configurar variáveis no GitLab CI/CD | F1 | ⚡ Bloqueante |
| 9 | Registrar domínio seubolsofeliz.com.br | F1 | 🟡 Pode depois |
| 10 | Configurar DNS | F1 | 🟡 Pode depois |
| 11 | Criar projeto Google Cloud | F3 | 🟡 Quando chegar na F3 |
| 12 | Habilitar Gmail API | F3 | 🟡 Quando chegar na F3 |
| 13 | Configurar OAuth consent screen | F3 | 🟡 Quando chegar na F3 |
| 14 | Criar credenciais OAuth | F3 | 🟡 Quando chegar na F3 |
| 15 | Gerar refresh token do Gmail | F3 | 🟡 Quando chegar na F3 |
| 16 | Criar labels no Gmail e mover e-mails de teste | F3 | 🟡 Quando chegar na F3 |
| 17 | Armazenar segredos no Vault | F3 | 🟡 Quando chegar na F3 |
| 18 | Criar conta/API key OpenAI | F7 | ⏳ Pode esperar |
| 19 | Configurar budget OpenAI | F7 | ⏳ Pode esperar |
| 20 | Armazenar senhas de PDFs no Vault | F4 | 🟡 Quando chegar na F4 |
