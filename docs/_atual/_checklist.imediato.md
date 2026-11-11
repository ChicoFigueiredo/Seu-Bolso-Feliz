# Checklist imediato — subir Vercel e Neon de produção

> Verificado em 2026-07-27; **reescrito em 2026-08-30** depois da saída completa do
> Supabase (ADR-009) — banco Neon/Drizzle, storage Vercel Blob, autenticação própria,
> fila em `ingestion_jobs`. As seções abaixo que falavam de chaves de API do Supabase,
> RLS/`GRANT`, `supabase db push` e Edge Functions foram substituídas pelo equivalente
> real de hoje; onde não deu para confirmar contra o código, está marcado `⚠️ verificar`.
>
> **Não há staging**: ambiente único de produção (decisão anterior à saída do Supabase,
> continua valendo — ver `.github/workflows/deploy.yml`).
> Detalhe de cada parâmetro em [`cfg.fornecedores.md`](cfg.fornecedores.md).
>
> ⚠️ Este documento é o mais antigo dos dois checklists — na dúvida entre este e
> [`_checklist.md`](_checklist.md), **aquele vence**.
>
> Marcação: 🔒 só você pode fazer · ⚙️ comando · ✅ já feito e verificado · ⚠️ verificar

---

## Estado do que já está pronto

| Item                     | Estado                                                                                                 |
| ------------------------ | ------------------------------------------------------------------------------------------------------ |
| Workflows de CI e deploy | ✅ `.github/workflows/{ci,deploy}.yml`, hoje contra Neon/Vercel                                        |
| Banco                    | ✅ Neon (Postgres) — `packages/db/schema.sql` (baseline) + `packages/db/migrations/*.sql` (3 arquivos) |
| Storage                  | ✅ Vercel Blob (`BLOB_READ_WRITE_TOKEN`), bucket `ingestion-originals`                                 |
| Autenticação             | ✅ própria — JWT em cookie httpOnly + Google OAuth + senha de reserva                                  |
| `bun run build`          | ⚠️ verificar — reconfirme depois de qualquer mudança de schema                                         |
| `bun run typecheck`      | ⚠️ verificar                                                                                           |
| Testes                   | ⚠️ verificar — contagem de referência é anterior à saída do Supabase                                   |

> O build **falhava** antes de uma rodada de correções em 2026-08-05: três dos cinco
> erros de typecheck eram consultas a colunas inexistentes, e o `next build` roda type
> check — logo, o deploy no Vercel teria falhado. Ficou corrigido naquele momento; como
> o banco e a autenticação mudaram desde então, reconfirme antes de assumir que segue
> limpo.

---

## 1. Neon (banco de produção)

### 1.1 ✅ Não há mais chave de API para rotacionar

As seções antigas deste checklist (versões antes de 2026-08-30) giravam em torno de
migrar chaves do Supabase (`anon`/`service_role` legacy → `sb_publishable_...`/
`sb_secret_...`) e depois desativar as legacy — inclusive as que vazaram no commit
`38b8126`. Isso não existe mais: **não há camada de chave de API para o banco**. A
única credencial de dados é a connection string do Neon (`DATABASE_URL`/
`DATABASE_URL_UNPOOLED`), que nunca teve — e não ganhou — um formato de chave rotável
à parte.

> ⚠️ **Verificar**: as chaves do Supabase antigo (`38b8126`) continuam existindo num
> projeto Supabase que talvez ainda não tenha sido desativado. A aplicação não fala
> mais com elas (confirmado: zero credencial Supabase em código vivo), mas isso não
> desliga o projeto Supabase em si. Se ele ainda existir, apagá-lo ou girar as chaves
> lá é o que fecha o assunto de vez.

### 1.2 🔒 Projeto Neon e connection strings

No painel do Neon: anote as duas connection strings do projeto/branch de produção —
**pooled** (host com `-pooler`, para `DATABASE_URL`) e **direta** (sem `-pooler`, para
`DATABASE_URL_UNPOOLED`, usada por `LISTEN`/`NOTIFY` e por migrations). Detalhe em
[cfg §1.1](cfg.fornecedores.md#11-banco--database_url--database_url_unpooled).

### 1.3 ⚙️ Aplicar schema e migrations

Banco Neon **do zero** (schema ainda não existe lá):

```bash
psql "$DATABASE_URL_UNPOOLED" -f packages/db/schema.sql
bash scripts/aplicar-migrations.sh
```

Banco Neon que **já tem o schema base** (o caso comum depois do primeiro deploy):

```bash
bash scripts/aplicar-migrations.sh
```

Isto substitui o antigo `supabase db push`. As migrations em `packages/db/migrations/`
são idempotentes — rodar de novo não quebra nem duplica (ver `packages/db/README.md`).

### 1.4 🔒 Fazer backup da chave de criptografia — não óbvio, e irreversível

Isto **continua valendo** depois da saída do Supabase: a migration
`packages/db/migrations/0002_crypto_key_e_usuarios.sql` porta o mesmo desenho
(`private.crypto_keys` + `private.get_crypto_key`) para o Neon. A chave nasce dentro do
banco e **nunca sai dele** — essa é a propriedade que a torna segura, e é também o que
torna um dump que exclua o schema `private` capaz de tornar toda senha de PDF
indecifrável para sempre.

⚙️ Guarde o valor num gerenciador de senhas:

```sql
select version, key from private.crypto_keys;
```

Confirme que o backup do deploy inclui o schema `private` — o workflow de deploy já
tira um dump completo antes de qualquer migration (`.github/workflows/deploy.yml`,
job `database`, passo "Backup antes de migrar"), publicado como artifact do GitHub
Actions:

```bash
grep -c "crypto_keys" backup-*.sql
```

→ [cfg §7 — Senhas de documentos protegidos](cfg.fornecedores.md#7-senhas-de-documentos-protegidos)

### 1.5 🔒 Login: Google OAuth e e-mails permitidos

Não existe mais "Auth → URL Configuration" do Supabase. A autenticação é própria
(`apps/web/src/lib/auth/`):

| Onde                               | O quê                                                                                                                 |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Google Cloud Console               | credencial OAuth 2.0 (Web application); URI de redirecionamento autorizada: `https://seudominio.com.br/auth/callback` |
| `GOOGLE_OAUTH_CLIENT_ID`/`_SECRET` | as credenciais acima, na Vercel (produção) e no `.env` local                                                          |
| `AUTH_ALLOWED_EMAILS`              | lista de e-mails que podem entrar pelo Google (default: só `fran.fig@gmail.com`)                                      |
| `AUTH_SECRET`                      | `openssl rand -base64 48` — assina o cookie de sessão                                                                 |
| `ADMIN_INITIAL_PASSWORD`           | senha da conta `admin@` de reserva, ≥12 caracteres                                                                    |

Sem a URI de redirecionamento correta cadastrada no Google Cloud Console, o login
falha no callback — o equivalente de hoje ao antigo "login redireciona para
localhost".

### 1.6 ✅ Usuário já existe — não precisa criar

O dono (`fran.fig@gmail.com`) e o admin de emergência (`admin@seubolsofeliz.com.br`) já
nascem semeados pela migration `0002_crypto_key_e_usuarios.sql` (§1.3, acima). O
primeiro login pelo Google casa por e-mail e preenche `google_sub` — não há tela de
cadastro nem `LOCAL_USER_ID` a descobrir para a aplicação web (a sessão resolve o
usuário pelo cookie). `LOCAL_USER_ID` só importa para os workers e para o MCP server —
ver [cfg §1.4](cfg.fornecedores.md#14-identidade-local--obrigatória-só-para-o-mcp-server).

### 1.7 ⚙️ Regenerar os tipos

```bash
bun run generate-types
git diff packages/shared-types/  # confirmar que bate com o schema real
```

> ⚠️ **Verificar antes de confiar no comando**: `scripts/generate-types.sh` ainda
> invoca `npx supabase gen types` internamente — não foi atualizado na saída do
> Supabase e provavelmente falha ou produz saída vazia sem o CLI do Supabase
> configurado. Até esse script ser corrigido (fora do escopo deste documento — é
> código, não texto), trate `packages/shared-types/src/database.types.ts` como
> mantido à mão contra o schema real, do jeito que já foi feito para as colunas e
> funções criadas depois do dump congelado em `packages/db/schema.sql`.

---

## 2. Vercel

### 2.1 🔒 Vincular o projeto

No dashboard do Vercel: **Add New → Project** → importar `Seu-Bolso-Feliz`.

| Configuração   | Valor      |
| -------------- | ---------- |
| Framework      | Next.js    |
| Root Directory | `apps/web` |
| Build Command  | padrão     |

Anote em **Settings → General**: `VERCEL_PROJECT_ID` e `VERCEL_ORG_ID` (team id).

### 2.2 🔒 Variáveis de ambiente **no Vercel**

Distinto dos secrets do GitHub: estas são as que a aplicação lê em runtime, ambiente
Production. Não existe mais `NEXT_PUBLIC_SUPABASE_*` — o banco não expõe nada ao
cliente.

| Variável                           | Valor                                                                                                       |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `DATABASE_URL`                     | connection string Neon, **pooled**                                                                          |
| `AUTH_SECRET`                      | `openssl rand -base64 48`                                                                                   |
| `AUTH_ALLOWED_EMAILS`              | e-mails autorizados a entrar pelo Google                                                                    |
| `GOOGLE_OAUTH_CLIENT_ID`/`_SECRET` | credenciais OAuth do Google Cloud Console                                                                   |
| `ADMIN_INITIAL_PASSWORD`           | senha inicial do `admin@` (≥12 caracteres)                                                                  |
| `BLOB_READ_WRITE_TOKEN`            | token do Vercel Blob (a Vercel injeta sozinha se a loja estiver conectada ao projeto — confira mesmo assim) |
| `NEXT_PUBLIC_APP_URL`              | `https://seudominio.com.br`                                                                                 |
| `OPENAI_API_KEY`                   | só se quiser chat e sugestões na web                                                                        |

> ⚠️ **`DATABASE_URL_UNPOOLED` NÃO precisa ir no Vercel** — a web (serverless) usa só a
> pooled. A unpooled é para os workers (`LISTEN`/`NOTIFY`) e para migrations, que
> rodam fora da Vercel.

→ [cfg §1 — variáveis](cfg.fornecedores.md#1-variáveis-de-ambiente)

### 2.3 🔒 Domínio

**Settings → Domains**. O valor precisa bater com a URI de redirecionamento do Google
OAuth (§1.5) e com `NEXT_PUBLIC_APP_URL`.

---

## 3. GitHub

### 3.1 🔒 Secrets

**Settings → Secrets and variables → Actions → New repository secret**. Os que
`.github/workflows/deploy.yml` de fato lê hoje — bem menos que antes, porque não há
mais projeto Supabase para autenticar:

| Secret                  | Onde obter                      |
| ----------------------- | ------------------------------- |
| `DATABASE_URL_UNPOOLED` | Neon — connection string direta |
| `VERCEL_TOKEN`          | Vercel → Account → Tokens       |
| `VERCEL_ORG_ID`         | passo 2.1                       |
| `VERCEL_PROJECT_ID`     | passo 2.1                       |

O repositório é **público**: por isso o workflow referencia `${{ secrets.X }}` em vez
de conter valor algum.

### 3.2 🔒 Environment `production` com aprovação manual

**Settings → Environments → New environment** → nome `production` → marcar **Required
reviewers** e adicionar você.

Sem staging, esse clique é a última barreira entre uma migration ruim e o banco de
verdade. O deploy pausa e te notifica antes de aplicar qualquer coisa — o workflow só
dispara por `workflow_dispatch`, nunca sozinho num push (ver o cabeçalho de
`.github/workflows/deploy.yml`).

### 3.3 ⚙️ CI

```bash
git push origin <sua-branch>
```

Acompanhe em **Actions**. O `ci.yml` sobe um Postgres de verdade como serviço
(`postgres:17-alpine`), aplica `packages/db/schema.sql` + migrations do zero, e roda
lint, format, typecheck, testes (unit/integração/E2E) e build — sem depender de nada
do Supabase.

### 3.4 ⚙️ Deploy

**Actions → Deploy → Run workflow**. A sequência hoje (`deploy.yml`): dump do banco via
`pg_dump` como artifact → `bash scripts/aplicar-migrations.sh` → build e deploy no
Vercel (`vercel pull/build/deploy --prod`). Não há mais passo de Edge Functions. Vai
pausar esperando sua aprovação (§3.2).

---

## 4. Depois de subir

| #   | O quê                                                                                     | Onde                                                                                                                      |
| --- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| 4.1 | Ciclo financeiro (dia do seu mês) + gerar períodos                                        | [cfg §3](cfg.fornecedores.md#3-ciclo-financeiro) — via tool MCP `recompute_financial_periods`, não há mais RPC SQL direta |
| 4.2 | **Ao menos uma instituição e um produto financeiro**                                      | [cfg §4](cfg.fornecedores.md#4-instituições-produtos-e-cartões)                                                           |
| 4.3 | Categorias e tags                                                                         | [cfg §5](cfg.fornecedores.md#5-categorias-e-tags)                                                                         |
| 4.4 | Fornecedores e aliases                                                                    | [cfg §6](cfg.fornecedores.md#6-fornecedores)                                                                              |
| 4.5 | Cartões — **pela tela**, `/dashboard/cards`                                               | [cfg §4.3](cfg.fornecedores.md#43-cartões--️-não-há-tela)                                                                  |
| 4.6 | Senhas de PDF, se tiver fatura protegida — **pela tela**, `/dashboard/settings/passwords` | [cfg §7](cfg.fornecedores.md#7-senhas-de-documentos-protegidos)                                                           |
| 4.7 | Gmail: OAuth com redirect de produção                                                     | [cfg §8](cfg.fornecedores.md#8-gmail)                                                                                     |

> **4.2 é o que trava tudo.** `transactions.financial_product_id` é `NOT NULL` e o
> pipeline não tem como adivinhar a conta. Sem um produto cadastrado, nenhum documento
> pode ser lançado.

---

## 5. Confirmar que funcionou

```bash
# Aplicação responde
curl -sI https://seudominio.com.br | head -1

# Migrations aplicadas — não há mais `supabase migration list`; confira direto no banco
psql "$DATABASE_URL" -tAc "select count(*) from private.crypto_keys;"  # deve retornar 1
```

Na aplicação: fazer login pelo Google (ou por senha, com `admin@`) → cadastrar
instituição e produto → subir um documento em `/dashboard/ingestion` → aprovar e
lançar em `/dashboard/ingestion/review` → confirmar que a transação aparece em
`/dashboard/transactions`.

→ [cfg — Verificar se está tudo certo](cfg.fornecedores.md#verificar-se-está-tudo-certo)

---

## 6. O que **não** vai funcionar mesmo com tudo acima

Não são bloqueadores de subida, mas evitam surpresa:

| Limitação                               | Detalhe                                                                                                                         |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Imagens sem OCR                         | Só PDF escaneado, e ainda assim desligado por padrão — issue #15                                                                |
| Sem agenda consolidada                  | A home mostra três listas separadas                                                                                             |
| Sem limite comprometido                 | `cards.credit_limit` não é lido por nenhum código de inteligência de fatura (a tela de cadastro já lê e grava; falta o cálculo) |
| Upload acima de ~4,5 MB falha           | Teto de Server Action da Vercel — ⚠️ verificar se isso foi corrigido depois de 2026-08-05 (F6.4)                                |
| `generate-types` possivelmente quebrado | Ainda chama `supabase gen types` internamente — ver §1.7                                                                        |

---

## Resumo: o que só depende de você

1. ⚠️ Verificar se o projeto Supabase antigo (chaves vazadas em `38b8126`) ainda existe e, se sim, desativá-lo ou girar as chaves lá — a aplicação não depende mais disso, mas o passivo de segurança só fecha quando o projeto for desligado
2. Fazer **backup da chave de criptografia** (1.4) — perdê-la é irreversível
3. Credencial OAuth do Google + `AUTH_ALLOWED_EMAILS`/`AUTH_SECRET`/`ADMIN_INITIAL_PASSWORD` (1.5)
4. Projeto, variáveis e domínio no Vercel (2.1–2.3), incluindo `BLOB_READ_WRITE_TOKEN`
5. Quatro secrets e o environment com aprovação no GitHub (3.1–3.2)
6. Decidir se o repositório continua **público**
