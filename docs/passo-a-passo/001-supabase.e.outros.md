# Passo a Passo — Supabase, GitLab e Ambiente Local

> Guia operacional completo para configurar Supabase (local e remoto), GitLab (repositório e CI/CD) e o ambiente de desenvolvimento local do projeto **Seu Bolso Feliz**.
>
> **Pré-requisito:** Etapa 0 (Sprint 0: Monorepo, Tooling e CI/CD) concluída — veja `docs/checklists/001-implementacao-geral.md`.

---

## Índice

1. [Supabase — Configuração Local](#1-supabase--configuração-local)
2. [Supabase — Projeto Remoto (Staging + Production)](#2-supabase--projeto-remoto-staging--production)
3. [Supabase — Autenticação](#3-supabase--autenticação)
4. [Supabase — Storage](#4-supabase--storage)
5. [Supabase — RLS (Row Level Security)](#5-supabase--rls-row-level-security)
6. [Supabase — Edge Functions](#6-supabase--edge-functions)
7. [Supabase — Migrations](#7-supabase--migrations)
8. [Supabase — Geração de Tipos TypeScript](#8-supabase--geração-de-tipos-typescript)
9. [GitLab — Criação e Configuração do Repositório](#9-gitlab--criação-e-configuração-do-repositório)
10. [GitLab — Branches e Proteção](#10-gitlab--branches-e-proteção)
11. [GitLab — Merge Requests](#11-gitlab--merge-requests)
12. [GitLab — Pipelines (CI/CD)](#12-gitlab--pipelines-cicd)
13. [GitLab — Ambientes e Variáveis](#13-gitlab--ambientes-e-variáveis)
14. [Teste Local — Página Web (Next.js)](#14-teste-local--página-web-nextjs)
15. [Teste Local — Banco de Dados e Supabase](#15-teste-local--banco-de-dados-e-supabase)
16. [Teste Local — Testes Automatizados](#16-teste-local--testes-automatizados)
17. [Fluxo Completo de Desenvolvimento](#17-fluxo-completo-de-desenvolvimento)
18. [Quando terei algo para ver e testar?](#18-quando-terei-algo-para-ver-e-testar)

---

## 1. Supabase — Configuração Local

O Supabase local roda via Docker e é a base do desenvolvimento. Nenhuma conexão remota é necessária para desenvolver.

### 1.1. Pré-requisitos

```bash
# Docker precisa estar instalado e rodando
docker --version   # Esperado: Docker 24+

# Supabase CLI (já disponível via npx, ou instale globalmente)
npx supabase --version   # Esperado: 2.x
```

### 1.2. Iniciar o Supabase local

```bash
cd /home/chico/dev/Chico/seu.bolso.feliz

# Inicia todos os serviços: PostgreSQL, Auth, Storage, Studio, Edge Runtime
npx supabase start
```

**Primeira execução:** vai baixar as imagens Docker (~2-5 min). Nas seguintes, inicia em segundos.

**Saída esperada (anotar estas URLs e chaves):**

```
         API URL: http://127.0.0.1:54321
     GraphQL URL: http://127.0.0.1:54321/graphql/v1
  S3 Storage URL: http://127.0.0.1:54321/storage/v1/s3
          DB URL: postgresql://postgres:postgres@127.0.0.1:54322/postgres
      Studio URL: http://127.0.0.1:54323
    Inbucket URL: http://127.0.0.1:54324
      JWT secret: super-secret-jwt-token-with-at-least-32-characters-long
        anon key: eyJhbGci...   ← copiar
service_role key: eyJhbGci...   ← copiar (NUNCA expor no frontend)
   S3 Access Key: ...
   S3 Secret Key: ...
```

### 1.3. Configurar variáveis locais

Copie o `.env.example` para `.env.local` e preencha com as chaves do passo anterior:

```bash
cp .env.example .env.local
```

Edite `.env.local`:

```bash
# ── Supabase Local ──
NEXT_PUBLIC_SUPABASE_URL=http://127.0.0.1:54321
NEXT_PUBLIC_SUPABASE_ANON_KEY=<anon key do supabase start>
SUPABASE_SERVICE_ROLE_KEY=<service_role key do supabase start>
SUPABASE_DB_PASSWORD=postgres

# ── App ──
NEXT_PUBLIC_APP_URL=http://localhost:3000
NODE_ENV=development
```

> **Segurança:** `.env.local` está no `.gitignore` — nunca será commitado.

### 1.4. Acessar o Supabase Studio (interface visual)

Abra no navegador: **http://127.0.0.1:54323**

Lá você pode:

- Ver/editar tabelas (Table Editor)
- Rodar SQL direto (SQL Editor)
- Ver logs de autenticação
- Gerenciar Storage buckets
- Testar Edge Functions
- Ver configurações de Auth

### 1.5. Parar e reiniciar

```bash
# Parar tudo (preserva dados)
npx supabase stop

# Parar E destruir dados (reset completo)
npx supabase stop --no-backup

# Status dos serviços
npx supabase status
```

### 1.6. Problemas comuns

| Problema                              | Solução                                                 |
| ------------------------------------- | ------------------------------------------------------- |
| `Cannot connect to the Docker daemon` | Inicie o Docker Desktop / `sudo systemctl start docker` |
| `Port 54321 already in use`           | `npx supabase stop` e tente novamente                   |
| `Timed out waiting for container`     | `docker system prune` e reinicie Docker                 |

---

## 2. Supabase — Projeto Remoto (Staging + Production)

### 2.1. Criar conta e projetos

1. Vá para **https://supabase.com** e crie uma conta (se não tiver)
2. Crie **2 projetos** (ou 1 para staging free e 1 para production depois):

| Projeto    | Nome sugerido    | Região                    | Plano               |
| ---------- | ---------------- | ------------------------- | ------------------- |
| Staging    | `sbf-staging`    | South America (São Paulo) | Free                |
| Production | `sbf-production` | South America (São Paulo) | Pro (quando lançar) |

3. Anote de cada projeto:
   - **Project Reference ID** (Settings → General → Reference ID)
   - **API URL** (`https://<ref>.supabase.co`)
   - **anon key** (Settings → API → `anon` key)
   - **service_role key** (Settings → API → `service_role` key)
   - **DB password** (o que você definiu na criação)

### 2.2. Gerar Access Token (para CLI e CI/CD)

1. Vá para **https://supabase.com/dashboard/account/tokens**
2. Clique em **Generate New Token**
3. Nome: `sbf-ci-cd`
4. Copie o token — ele só aparece uma vez

Este token será usado:

- No GitLab CI/CD (variável `SUPABASE_ACCESS_TOKEN`)
- Localmente para comandos `supabase link`

### 2.3. Vincular projeto local ao remoto

```bash
# Para staging
npx supabase link --project-ref <STAGING_REF_ID>
# Vai pedir a senha do banco (DB password do projeto staging)
```

> **Nota:** `supabase link` configura o contexto local para apontar para o projeto remoto. Necessário para `db push` e `functions deploy` remotos.

---

## 3. Supabase — Autenticação

### 3.1. Configuração local (já pronta)

O `supabase/config.toml` já configura Auth local:

```toml
[auth]
site_url = "http://127.0.0.1:3105"
additional_redirect_urls = [
  "http://127.0.0.1:3105",
  "http://localhost:3105",
  "http://127.0.0.1:3105/auth/callback",
  "http://localhost:3105/auth/callback"
]
jwt_expiry = 3600

[auth.email]
enable_signup = true
enable_confirmations = false   # Desabilitado localmente para conveniência
```

Emails locais vão para o **Inbucket** (http://127.0.0.1:54324) — sem envio real.

Importante: sempre que alterar `site_url` ou `additional_redirect_urls`, reinicie o Supabase local:

```bash
npx supabase stop
npx supabase start
```

### 3.2. Como testar login local sem SMTP externo

Opcao A: Magic link com Inbucket

1. Rode o frontend em `http://127.0.0.1:3105`
2. Abra o Inbucket em `http://127.0.0.1:54324`
3. Na tela de login, informe seu e-mail e clique em "Entrar com Magic Link"
4. No Inbucket, abra o e-mail recebido e clique no link
5. O fluxo deve retornar para `/auth/callback` e depois redirecionar para `/dashboard`

Opcao B: Conta de teste local com e-mail e senha

1. Acesse a tela de login
2. Informe e-mail e uma senha com no minimo 6 caracteres
3. Clique em "Criar Conta de Teste Local"
4. A aplicacao cria o usuario no Supabase local e faz login automaticamente
5. Nos proximos acessos, use "Entrar com E-mail e Senha"

Opcao C: Criar usuario manualmente no Supabase Studio

1. Abra `http://127.0.0.1:54323`
2. Va em Authentication -> Users
3. Crie um usuario manualmente com e-mail e senha
4. Use a opcao "Entrar com E-mail e Senha" na aplicacao

### 3.3. Configuração remota (staging/production)

No dashboard do Supabase (para cada projeto):

1. **Authentication → Providers → Email:** Habilitado
2. **Authentication → URL Configuration:**
   - Site URL: URL do seu frontend (staging ou production)

- Redirect URLs: adicione a URL do ambiente e o callback correspondente

3. **Authentication → Rate Limits:** Mantenha os padrões (ou ajuste conforme necessidade)
4. **Authentication → Email Templates:** Personalize os emails (opcional no MVP)

### 3.4. Integração com Next.js (será feita na Etapa 1)

Pacotes necessários (serão instalados na Etapa 1):

```bash
cd apps/web
bun add @supabase/supabase-js @supabase/ssr
```

Arquivos a criar:

- `apps/web/src/lib/supabase/client.ts` — cliente browser
- `apps/web/src/lib/supabase/server.ts` — cliente server (RSC/Route Handlers)
- `apps/web/src/middleware.ts` — refresh de sessão automático

---

## 4. Supabase — Storage

### 4.1. Para que serve no projeto

O Storage será usado para:

- Documentos financeiros (PDFs de faturas, comprovantes)
- Anexos de transações
- Contratos digitalizados

### 4.2. Configuração local

O Storage local já está configurado no `config.toml`:

```toml
[storage]
file_size_limit = "50MiB"
```

### 4.3. Criar bucket (será feito na Etapa 5)

Via migration SQL:

```sql
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
  'documents',
  'documents',
  false,                              -- privado
  52428800,                           -- 50MB
  ARRAY['application/pdf', 'image/png', 'image/jpeg']
);
```

### 4.4. RLS no Storage

Cada bucket terá policies para que usuários só vejam seus próprios arquivos:

```sql
CREATE POLICY "Users can upload own documents"
ON storage.objects FOR INSERT TO authenticated
WITH CHECK (bucket_id = 'documents' AND auth.uid()::text = (storage.foldername(name))[1]);

CREATE POLICY "Users can view own documents"
ON storage.objects FOR SELECT TO authenticated
USING (bucket_id = 'documents' AND auth.uid()::text = (storage.foldername(name))[1]);
```

> **Nota:** A implementação completa do Storage é da **Etapa 5**. No MVP, o foco é domínio financeiro.

---

## 5. Supabase — RLS (Row Level Security)

### 5.1. Princípio fundamental

**Toda tabela com dados de usuário DEVE ter RLS habilitado.** Sem exceção.

### 5.2. Padrão de RLS do projeto

Todas as tabelas do domínio seguem o mesmo padrão:

```sql
-- Habilitar RLS
ALTER TABLE nome_tabela ENABLE ROW LEVEL SECURITY;

-- Política SELECT: usuário vê apenas seus dados
CREATE POLICY "Users can view own data"
ON nome_tabela FOR SELECT TO authenticated
USING (user_id = auth.uid());

-- Política INSERT: usuário cria apenas seus dados
CREATE POLICY "Users can create own data"
ON nome_tabela FOR INSERT TO authenticated
WITH CHECK (user_id = auth.uid());

-- Política UPDATE: usuário edita apenas seus dados
CREATE POLICY "Users can update own data"
ON nome_tabela FOR UPDATE TO authenticated
USING (user_id = auth.uid())
WITH CHECK (user_id = auth.uid());

-- Política DELETE: usuário deleta apenas seus dados (soft delete preferido)
CREATE POLICY "Users can delete own data"
ON nome_tabela FOR DELETE TO authenticated
USING (user_id = auth.uid());
```

### 5.3. Como testar RLS localmente

```bash
# No Supabase Studio (http://127.0.0.1:54323)
# Vá em SQL Editor e rode:

-- Simular como usuário específico
SET request.jwt.claims = '{"sub": "user-uuid-aqui", "role": "authenticated"}';
SELECT * FROM suppliers;  -- Deve retornar apenas dados do user-uuid
```

Ou nos testes de integração (Etapa 2):

```typescript
// Cria cliente autenticado como user específico
const supabase = createClient(url, anonKey, {
  global: { headers: { Authorization: `Bearer ${userJWT}` } },
});
const { data } = await supabase.from("suppliers").select("*");
// Deve retornar apenas os dados do usuário
```

---

## 6. Supabase — Edge Functions

### 6.1. O que são

Edge Functions são funções serverless (Deno) que rodam no Supabase. Serão usadas para:

- Merge atômico de fornecedores (Etapa 5)
- Retroatividade de associação de fornecedores (Etapa 5)
- Lógica server-side que precisa de `service_role` (acesso admin)

### 6.2. Criar uma Edge Function

```bash
npx supabase functions new nome-da-funcao
```

Isso cria `supabase/functions/nome-da-funcao/index.ts`:

```typescript
import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

Deno.serve(async (req) => {
  const supabaseClient = createClient(
    Deno.env.get("SUPABASE_URL")!,
    Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!,
  );

  // Lógica aqui...

  return new Response(JSON.stringify({ ok: true }), {
    headers: { "Content-Type": "application/json" },
  });
});
```

### 6.3. Testar localmente

```bash
# Serve todas as funções (hot reload)
npx supabase functions serve

# Chamar com curl
curl -i --location --request POST \
  'http://127.0.0.1:54321/functions/v1/nome-da-funcao' \
  --header 'Authorization: Bearer <anon_key>' \
  --header 'Content-Type: application/json' \
  --data '{"key": "value"}'
```

### 6.4. Deploy remoto

```bash
# Deploy de uma função específica
npx supabase functions deploy nome-da-funcao --project-ref <REF_ID>

# Deploy de todas as funções
npx supabase functions deploy --project-ref <REF_ID>
```

### 6.5. Secrets das funções

```bash
# Definir secrets (remoto)
npx supabase secrets set MY_SECRET=valor --project-ref <REF_ID>

# Listar secrets
npx supabase secrets list --project-ref <REF_ID>
```

> **Nota:** Edge Functions serão implementadas a partir da **Etapa 5**. No MVP, o foco é CRUD direto via client.

---

## 7. Supabase — Migrations

### 7.1. Conceito

Migrations são arquivos SQL versionados que evoluem o schema do banco. São o **mecanismo oficial** de alteração de schema — nunca altere o banco manualmente em staging/production.

### 7.2. Criar uma nova migration

```bash
npx supabase migration new nome_descritivo_da_migration
```

Cria: `supabase/migrations/<timestamp>_nome_descritivo_da_migration.sql`

### 7.3. Escrever o SQL

Edite o arquivo criado com o SQL desejado:

```sql
-- supabase/migrations/20260321150000_create_suppliers.sql

CREATE TABLE suppliers (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  -- ...
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE suppliers ENABLE ROW LEVEL SECURITY;
-- ... policies ...
```

### 7.4. Aplicar localmente

```bash
# Aplica todas as migrations pendentes no banco local
npx supabase db reset
```

`db reset` destrói e recria o banco local, aplicando todas as migrations + seed.sql na ordem.

Para aplicar apenas a última:

```bash
npx supabase migration up
```

### 7.5. Verificar diferenças

```bash
# Mostra diff entre banco local e migrations
npx supabase db diff
```

### 7.6. Aplicar no remoto (staging/production)

```bash
# Vincular ao projeto (se ainda não fez)
npx supabase link --project-ref <REF_ID>

# Push das migrations
npx supabase db push
```

### 7.7. Regras de ouro

1. **Migrations são imutáveis** — após merge em `develop`, nunca editar. Correções viram nova migration.
2. **Nomear descritivamente** — `create_suppliers`, `add_supplier_id_to_transactions`, `create_rls_policies_suppliers`
3. **Uma responsabilidade por migration** — não misture criação de tabela com lógica de negócio
4. **Testar localmente** — `db reset` + validar no Studio antes de commitar
5. **Revisar em MR** — migrations devem ser revisadas como código

---

## 8. Supabase — Geração de Tipos TypeScript

### 8.1. Gerar tipos do banco local

```bash
bun run generate-types
```

Isso executa `scripts/generate-types.sh` que gera `packages/shared-types/src/database.types.ts`.

### 8.2. Quando gerar

- Após criar/alterar migrations
- Após rodar `db reset`
- Antes de commitar mudanças de schema

### 8.3. Validação no CI

O pipeline CI compara os tipos gerados com os commitados. Se divergirem, a pipeline falha (job `validate-types`).

---

## 9. GitLab — Criação e Configuração do Repositório

### 9.1. Criar o repositório

1. Vá para **https://gitlab.com** (ou seu GitLab self-hosted)
2. **New Project → Create blank project**
   - Nome: `seu-bolso-feliz`
   - Visibility: Private
   - Initialize with README: **Não** (já temos código)
3. Copie a URL SSH/HTTPS do repositório

### 9.2. Conectar o local ao remoto

```bash
cd /home/chico/dev/Chico/seu.bolso.feliz

# Se já tem remote origin (ex: GitHub), remova ou renomeie
git remote rename origin github  # opcional, se quiser manter GitHub

# Adicionar GitLab como origin
git remote add origin git@gitlab.com:<seu-usuario>/seu-bolso-feliz.git

# Verificar
git remote -v
```

### 9.3. Push inicial

```bash
# Criar branch develop se não existe
git checkout -b develop
git push -u origin develop

# Push da main
git checkout main
git push -u origin main
```

---

## 10. GitLab — Branches e Proteção

### 10.1. Proteção de branches

No GitLab, vá para **Settings → Repository → Protected branches**:

| Branch    | Allowed to merge         | Allowed to push | Allowed to force push |
| --------- | ------------------------ | --------------- | --------------------- |
| `main`    | Maintainers              | No one          | No                    |
| `develop` | Developers + Maintainers | No one          | No                    |

**Resultado:** Ninguém faz push direto para `main` ou `develop`. Tudo passa por Merge Request.

### 10.2. Push Rules (opcional mas recomendado)

Em **Settings → Repository → Push Rules**:

- **Branch name regex:** `^(main|develop|feature\/[a-z0-9-]+|fix\/[a-z0-9-]+|chore\/[a-z0-9-]+|refactor\/[a-z0-9-]+|docs\/[a-z0-9-]+|test\/[a-z0-9-]+|migration\/[a-z0-9-]+)$`
- **Commit message regex:** `^(feat|fix|docs|style|refactor|test|chore|ci|perf|migration)(\(.+\))?!?:\s.+`

### 10.3. Modelo de branches

```
main ←────────────── produção (deploy manual)
  ↑
develop ←──────────── staging (deploy automático)
  ↑
feature/xxx ─────── desenvolvimento diário
fix/xxx
chore/xxx
migration/xxx
```

**Regra:** toda branch nasce de `develop` e volta para `develop` via MR.

Promover `develop → main` = release (MR separado, approval do CEO).

---

## 11. GitLab — Merge Requests

### 11.1. Configurações de MR

Em **Settings → Merge Requests**:

- **Merge method:** Merge commit (para `main`), Squash merges (para `develop`)
- **Merge checks:**
  - ✅ Pipelines must succeed
  - ✅ All threads must be resolved
- **Merge suggestions:**
  - ✅ Delete source branch when merge request is accepted
- **Approvals:** Settings → Merge Requests → Approvals
  - Para `develop`: 1 aprovação mínima
  - Para `main`: 1 aprovação mínima (CEO ou Arquiteta)

### 11.2. Template de MR (opcional)

Crie `.gitlab/merge_request_templates/Default.md`:

```markdown
## O que muda?

<!-- Descreva as mudanças -->

## Tipo

- [ ] feature
- [ ] fix
- [ ] chore/refactor
- [ ] migration

## Checklist

- [ ] Testes passando
- [ ] Lint limpo
- [ ] Tipos gerados/atualizados (se migration)
- [ ] Documentação atualizada (se necessário)
```

---

## 12. GitLab — Pipelines (CI/CD)

### 12.1. O que já temos

O `.gitlab-ci.yml` já está configurado com 6 stages. A pipeline roda automaticamente em:

- **Merge Requests:** validate → install → check → test → build
- **Push em `develop`:** install → check → test → build → deploy (staging)
- **Push em `main`:** install → check → test → build → deploy (production, manual)

### 12.2. Verificar que a pipeline funciona

Após o push inicial para GitLab:

1. Vá para **CI/CD → Pipelines**
2. Deve aparecer a pipeline rodando
3. Se der erro, verifique:
   - Runner disponível (GitLab.com tem shared runners)
   - Variáveis CI/CD configuradas (passo 13)

### 12.3. Jobs da pipeline

| Stage    | Job                   | O que faz                       | Quando roda                            |
| -------- | --------------------- | ------------------------------- | -------------------------------------- |
| validate | `validate-commits`    | Verifica Conventional Commits   | Apenas em MR                           |
| validate | `validate-types`      | Compara tipos TS                | MR com mudanças em migrations/types    |
| install  | `install-deps`        | `bun install --frozen-lockfile` | Sempre                                 |
| check    | `lint`                | `bun run lint`                  | Sempre                                 |
| check    | `typecheck`           | `bun run typecheck`             | Sempre                                 |
| check    | `format-check`        | `bun run format:check`          | Sempre                                 |
| test     | `test-unit`           | `bun run test:unit`             | Sempre                                 |
| test     | `test-integration`    | `bun run test:integration`      | Mudanças em supabase/domain/validation |
| build    | `build-web`           | `bun run build` (Next.js)       | develop/main ou mudanças em apps/web   |
| deploy   | `deploy-*-staging`    | Deploy Supabase + Web           | Automático em `develop`                |
| deploy   | `deploy-*-production` | Deploy Supabase + Web           | Manual em `main`                       |

---

## 13. GitLab — Ambientes e Variáveis

### 13.1. Configurar variáveis CI/CD

Vá para **Settings → CI/CD → Variables** e adicione:

| Variable                          | Value                        | Protected | Masked | Environment |
| --------------------------------- | ---------------------------- | --------- | ------ | ----------- |
| `SUPABASE_ACCESS_TOKEN`           | Token gerado no passo 2.2    | ✅        | ✅     | All         |
| `STAGING_SUPABASE_PROJECT_ID`     | Ref ID do projeto staging    | ✅        | ❌     | staging     |
| `STAGING_SUPABASE_DB_PASSWORD`    | Senha do banco staging       | ✅        | ✅     | staging     |
| `PRODUCTION_SUPABASE_PROJECT_ID`  | Ref ID do projeto production | ✅        | ❌     | production  |
| `PRODUCTION_SUPABASE_DB_PASSWORD` | Senha do banco production    | ✅        | ✅     | production  |

### 13.2. Configurar ambientes

Vá para **Operate → Environments**:

1. Crie `staging`:
   - External URL: URL do frontend staging (quando houver)
   - Auto-stop: opcional
2. Crie `production`:
   - External URL: URL do frontend production (quando houver)
   - Protected: ✅ (apenas branches protegidas podem deployar)

### 13.3. Regra de segredos

- **NUNCA** commitar chaves reais no repositório
- **Protected variables** só são acessíveis em branches protegidas (`main`, `develop`)
- **Masked variables** não aparecem nos logs da pipeline
- `service_role_key` **NUNCA** vai para o frontend — apenas em Edge Functions ou server-side

---

## 14. Teste Local — Página Web (Next.js)

### 14.1. Iniciar o servidor de desenvolvimento

```bash
# Na raiz do monorepo
bun run dev
```

Isso executa `cd apps/web && bun run dev` (Next.js com Turbopack).

Acesse: **http://localhost:3100**

### 14.2. O que você verá agora

Neste momento (pós-Etapa 0), verá a página de placeholder:

- Card centralizado com "Seu Bolso Feliz"
- Texto "MVP em construção. Etapa 0 concluída — monorepo configurado."

### 14.3. Hot reload

Qualquer alteração em `apps/web/src/` reflete automaticamente no navegador (Turbopack HMR).

### 14.4. Build de produção local

```bash
bun run build
# Se quiser servir o build localmente:
cd apps/web && bun run start
```

---

## 15. Teste Local — Banco de Dados e Supabase

### 15.1. Iniciar Supabase local + Web

Terminal 1:

```bash
npx supabase start
```

Terminal 2:

```bash
bun run dev
```

Agora o frontend em http://localhost:3000 pode se conectar ao Supabase em http://127.0.0.1:54321.

### 15.2. Testando o banco direto

#### Via Supabase Studio

- Abra http://127.0.0.1:54323
- Table Editor: visualize, crie e edite dados
- SQL Editor: rode queries manuais

#### Via psql (linha de comando)

```bash
# Conectar direto ao PostgreSQL local do Supabase
psql postgresql://postgres:postgres@127.0.0.1:54322/postgres
```

#### Via script SQL

```bash
# Rodar um arquivo SQL
psql postgresql://postgres:postgres@127.0.0.1:54322/postgres -f meu_script.sql
```

### 15.3. Reset do banco (reaplica todas as migrations + seed)

```bash
npx supabase db reset
```

Útil quando:

- Você alterou uma migration existente (só localmente!)
- Quer limpar todos os dados
- Quer testar o seed.sql

### 15.4. Verificar que migrations funcionam

```bash
# Aplicar migrations pendentes
npx supabase migration up

# Ver status das migrations
npx supabase migration list
```

---

## 16. Teste Local — Testes Automatizados

### 16.1. Testes unitários (rápidos, sem banco)

```bash
# Todos os testes unitários
bun run test:unit

# Um arquivo específico
bunx vitest run __tests__/domain/setup.test.ts

# Com coverage
bun run test:coverage
```

Testes unitários testam lógica pura de domínio (cálculos financeiros, validações, regras de negócio) sem depender de banco ou rede.

### 16.2. Testes de integração (com banco)

```bash
# Requer supabase start ativo!
npx supabase start

# Rodar testes de integração
bun run test:integration
```

Testes de integração se conectam ao Supabase local para validar:

- RLS policies
- Triggers
- Constraints
- Edge Functions

### 16.3. Estrutura de testes

```
__tests__/
├── domain/          ← Testes unitários (sem banco)
│   ├── setup.test.ts
│   ├── supplier.test.ts         (Etapa 2)
│   ├── supplier-metrics.test.ts (Etapa 2)
│   ├── deduplication.test.ts    (Etapa 2)
│   └── alias-governance.test.ts (Etapa 2)
├── integration/     ← Testes de integração (com banco)
│   ├── supplier-crud.test.ts    (Etapa 2)
│   └── rls-policies.test.ts     (Etapa 2)
└── e2e/             ← Testes end-to-end (futuros)
```

### 16.4. Mocks e stubs para testes sem banco

Para testes unitários que não precisam de banco real, use:

```typescript
// __tests__/helpers/mock-supabase.ts
export function createMockSupabaseClient() {
  return {
    from: (table: string) => ({
      select: () => Promise.resolve({ data: [], error: null }),
      insert: (data: unknown) => Promise.resolve({ data, error: null }),
      update: (data: unknown) => Promise.resolve({ data, error: null }),
      delete: () => Promise.resolve({ data: null, error: null }),
    }),
    auth: {
      getUser: () => Promise.resolve({ data: { user: { id: "mock-user-id" } }, error: null }),
    },
  };
}
```

Para testes de integração, use o Supabase local real (não mocks).

---

## 17. Fluxo Completo de Desenvolvimento

Aqui está o fluxo que você seguirá diariamente:

### Passo 1: Atualizar develop

```bash
git checkout develop
git pull origin develop
```

### Passo 2: Criar branch de feature

```bash
git checkout -b feature/suppliers-crud
```

### Passo 3: Desenvolver com banco local

```bash
# Terminal 1: Supabase
npx supabase start

# Terminal 2: Web dev server
bun run dev

# Terminal 3: Desenvolvimento...
```

### Passo 4: Se precisar de migration

```bash
npx supabase migration new create_suppliers
# Editar o arquivo SQL gerado
npx supabase db reset   # Aplicar localmente
bun run generate-types  # Atualizar tipos
```

### Passo 5: Testar

```bash
bun run test          # Testes unitários + integração
bun run lint          # Lint
bun run typecheck     # Tipos
bun run format:check  # Formatação
```

### Passo 6: Commitar (Conventional Commits)

```bash
git add .
git commit -m "feat(supabase): create suppliers table with RLS"
```

O Husky vai rodar lint-staged e commitlint automaticamente.

### Passo 7: Push e MR

```bash
git push -u origin feature/suppliers-crud
```

No GitLab: crie um Merge Request de `feature/suppliers-crud` → `develop`.

A pipeline roda automaticamente. Se verde + aprovado → merge.

### Passo 8: Deploy staging (automático)

Ao fazer merge em `develop`, o pipeline deploya automaticamente para staging.

### Passo 9: Release para produção

Quando staging estiver estável, crie MR `develop → main`. Após merge, faça deploy manual no pipeline.

---

## 18. Quando terei algo para ver e testar?

### Marcos de visibilidade

| Marco               | O que você verá                              | Quando                   |
| ------------------- | -------------------------------------------- | ------------------------ |
| **Agora (Etapa 0)** | Página placeholder em http://localhost:3000  | ✅ Já disponível         |
| **Etapa 1.1**       | Login/cadastro funcionando com email         | Após configurar Auth     |
| **Etapa 1.2**       | Tabelas criadas visíveis no Supabase Studio  | Após migrations          |
| **Etapa 1.4**       | CRUD de fornecedores testável via API/Studio | Após Edge Functions/CRUD |
| **Etapa 3**         | Formulários web para cadastrar fornecedores  | Após UI do CRUD          |
| **Etapa 4**         | Dashboard com relatórios e filtros           | Após Etapa 4             |

### O que testar primeiro (hands-on)

1. **Agora:** `bun run dev` → ver a página no browser
2. **Após Etapa 1:** `npx supabase start` → ver tabelas no Studio (http://127.0.0.1:54323)
3. **Após Etapa 1 completa:** testar CRUD de fornecedores via Studio ou curl
4. **Após Etapa 3:** navegar pela interface web com formulários reais

### Resumo de expectativa

> O primeiro momento "tangível e interativo" será após a **Etapa 1 completa** — quando você poderá:
>
> - Fazer login no sistema
> - Criar fornecedores, aliases e contratos
> - Ver dados no Supabase Studio
> - Rodar testes e ver cobertura
>
> A interface web com formulários e dashboard virá nas **Etapas 3 e 4**.

---

## Referências

| Documento                       | Caminho                                                                       |
| ------------------------------- | ----------------------------------------------------------------------------- |
| Guia de Implementação           | `docs/planejamento/001-guia-implementacao-passo-a-passo.md`                   |
| Guia CI/CD Operacional          | `docs/planejamento/002-guia-cicd-engenharia-operacional.md`                   |
| ADR-004 Arquitetura Operacional | `docs/adrs/ADR-004-arquitetura-operacional-repositorio-cicd.md`               |
| Ata do Refino CI/CD             | `docs/refinos/2026-03/2026-03-21-14-19-refino-arquitetura-engenharia-cicd.md` |
| Checklist de Implementação      | `docs/checklists/001-implementacao-geral.md`                                  |
| Configuração Supabase           | `supabase/config.toml`                                                        |
| Pipeline CI/CD                  | `.gitlab-ci.yml`                                                              |
| Variáveis de Ambiente           | `.env.example`                                                                |
