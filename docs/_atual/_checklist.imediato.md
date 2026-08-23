# Checklist imediato — subir Vercel e Supabase de produção

> Verificado em 2026-07-27. **Não há staging**: ambiente único de produção.
> Detalhe de cada parâmetro em [`cfg.fornecedores.md`](cfg.fornecedores.md).
>
> Marcação: 🔒 só você pode fazer · ⚙️ comando · ✅ já feito e verificado

---

## Estado do que já está pronto

| Item                                      | Estado                                 |
| ----------------------------------------- | -------------------------------------- |
| Workflows de CI e deploy                  | ✅ `.github/workflows/{ci,deploy}.yml` |
| `.gitlab-ci.yml` e `.env.staging.example` | ✅ removidos                           |
| Migrations (36)                           | ✅ aplicam do zero                     |
| Bucket `ingestion-originals`              | ✅ criado por migration                |
| `bun run build`                           | ✅ **passa**                           |
| `bun run typecheck`                       | ✅ **zero erros**                      |
| Testes                                    | ✅ 505/505                             |

> O build **falhava** até hoje. Três dos cinco erros de typecheck eram consultas a
> colunas inexistentes (`transactions.transaction_date`, `.category`,
> `ingestion_jobs.step`), e o `next build` roda type check — logo, **o deploy no Vercel
> teria falhado**. Corrigidos; as tools de IA afetadas também estavam quebradas em
> runtime.

---

## 1. Supabase de produção

### 1.1 🔒 Migrar para as chaves novas e desativar as legacy — **antes de tudo**

As chaves vazaram no commit `38b8126`. Enquanto valerem, qualquer pessoa com acesso ao
histórico do repositório **público** entra no banco.

> **Não existe botão "Rotate" para elas, e não é falta de procurar.** As suas chaves são
> as **legacy** (`anon` e `service_role`). Elas derivam do JWT secret do projeto — é
> justamente isso que as torna irrotacionáveis, porque mexer nelas mexeria em tudo que o
> secret assina. A documentação de migração diz, textualmente: _"as chaves legacy `anon`
> e `service_role` continuam funcionando até o fim de 2026"_ — e o caminho oferecido não
> é rotação, é substituição.
>
> **Criar chaves novas e desativar as antigas** tem o mesmo efeito de segurança: as
> vazadas param de valer. E a partir daí passa a existir rotação de verdade, chave por
> chave, sem downtime.

#### O que já foi feito no código (não precisa refazer)

| Feito                                                          | Onde                                      |
| -------------------------------------------------------------- | ----------------------------------------- |
| Edge Functions leem `SUPABASE_SECRET_KEYS` / `..._KEYS` (JSON) | `supabase/functions/_shared/keys.ts`      |
| `verify_jwt = false` declarado por função                      | `supabase/config.toml`                    |
| `--no-verify-jwt` global removido do deploy                    | `.github/workflows/deploy.yml`            |
| `.env.local` e fixtures de teste em `sb_secret_...`            | `.env.local`, `__tests__/`                |
| `NEXT_PUBLIC_SUPABASE_ANON_KEY` órfã removida do CI            | `.github/workflows/ci.yml`                |
| `.env.example` documentando o formato exigido                  | `.env.example`, `.env.production.example` |

Prova: 505/505 testes, typecheck e build verdes com as chaves novas.

#### O que só você pode fazer

**a. Criar as chaves novas** — **Settings → API Keys** → aba **Publishable and secret
API keys** → **Create new API keys**. É seguro: as novas nascem ao lado das legacy, que
continuam funcionando. Vêm com o nome `default` — é esse nome que o código procura.

**b. Trocar os valores.** Nenhum nome de variável muda; só o valor.

| Onde                           | Variável                               | De       | Para                 |
| ------------------------------ | -------------------------------------- | -------- | -------------------- |
| `.env.production`              | `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | `eyJ...` | `sb_publishable_...` |
| `.env.production`              | `SUPABASE_SECRET_KEY`                  | `eyJ...` | `sb_secret_...`      |
| `.env` (cofre)                 | `PRD_PUBLISHABLE_KEY`                  | `eyJ...` | `sb_publishable_...` |
| `.env` (cofre)                 | `PRD_SECRET_KEY`                       | `eyJ...` | `sb_secret_...`      |
| Vercel → Environment Variables | `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | `eyJ...` | `sb_publishable_...` |

**A remover, não substituir:**

| O quê                                                 | Por quê                                                       |
| ----------------------------------------------------- | ------------------------------------------------------------- |
| `STAGING_*` no `.env` (6 variáveis)                   | Staging foi eliminado. As chaves ainda valem — são um passivo |
| O arquivo `.env.staging`                              | Idem                                                          |
| O projeto Supabase `seu-bolso-feliz-staging`          | Enquanto existir, as chaves dele continuam abrindo um banco   |
| `.env.local.bak-*` (backup criado durante a migração) | Contém as chaves locais antigas                               |

> `.env.local` aponta para `127.0.0.1:54321` e já foi migrado. As chaves de lá **não são
> segredo**: estão embutidas no binário do CLI e são iguais em toda máquina.
> **Nada do que vazou de `.env.local` importa** — o que importa é `.env.production` e o
> `PRD_*` do `.env`.

**c. Conferir que nada mais usa as legacy.** Não há indicador automático de uso — é
conferência manual, e é o passo que as pessoas pulam. Além do óbvio: o MCP configurado
na sua máquina, `~/.config`, scripts avulsos, e qualquer `.env` fora deste repositório.

⚙️ Uma varredura que pega a maior parte:

```bash
grep -rn "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9" ~ --include=".env*" --include="*.json" 2>/dev/null
```

**d. Desativar as legacy** — mesma tela **Settings → API Keys**. **É este passo que
neutraliza o vazamento**; tudo antes dele é preparação. É reversível: se descobrir um
cliente esquecido, dá para reativar.

**e. Redeploy das Edge Functions.** Elas leem a chave em tempo de execução, mas o
`verify_jwt = false` só vale a partir de um deploy novo:

```bash
supabase functions deploy
```

#### Depois: JWT signing keys (migração separada)

As chaves de API novas não tocam mais o JWT secret, mas os tokens que o Supabase Auth
emite para os seus usuários **ainda são assinados por ele**. Migrar para
**Settings → JWT Keys** tira o projeto inteiro do segredo compartilhado (aí sim com
botão **Rotate Keys** e um **Revoke** para invalidar a anterior).

Não é urgente como (d), mas é o que fecha o assunto de vez.

→ [cfg §1.1 — chaves de API](cfg.fornecedores.md#11-as-chaves-de-api--use-as-novas)
→ [Migrar para publishable/secret](https://supabase.com/docs/guides/getting-started/migrating-to-new-api-keys)
→ [JWT Signing Keys](https://supabase.com/docs/guides/auth/signing-keys)

### 1.2 🔒 Projeto e vínculo

Anote o **project ref** (Settings → General). Vai em `SUPABASE_PROJECT_ID`.

⚙️ Local, uma vez:

```bash
supabase login
supabase link --project-ref <seu-ref>
```

### 1.3 ⚙️ Aplicar as migrations

```bash
supabase db push
```

São 36 migrations. Isso cria tabelas, RLS, RPCs, o bucket `ingestion-originals` **e**
gera a chave de criptografia dos segredos.

→ [cfg §2 — Banco e usuário](cfg.fornecedores.md#2-banco-e-usuário)

### 1.4 🔒 **Fazer backup da chave de criptografia** — não óbvio, e irreversível

A migration gera uma chave aleatória em `private.crypto_keys` e ela **nunca sai do
banco**. Essa é justamente a propriedade que a torna segura — e é também o que faz um
dump que exclua o schema `private` tornar **todas as senhas de PDF indecifráveis para
sempre**.

⚙️ Guarde o valor num gerenciador de senhas:

```sql
select version, key from private.crypto_keys;
```

Confirme que o backup automático do deploy inclui o schema:

```bash
supabase db dump --file backup.sql && grep -c "crypto_keys" backup.sql
```

→ [cfg §7 — Senhas de documentos protegidos](cfg.fornecedores.md#7-senhas-de-documentos-protegidos)

### 1.5 🔒 Auth: URLs de produção

`supabase/config.toml` aponta para `127.0.0.1:3105` — é a configuração **local**. No
Supabase hospedado isso se configura no dashboard, não no arquivo.

Em **Authentication → URL Configuration**:

| Campo         | Valor                                     |
| ------------- | ----------------------------------------- |
| Site URL      | `https://seudominio.com.br`               |
| Redirect URLs | `https://seudominio.com.br/auth/callback` |

Sem isso, o login redireciona para localhost e falha.

### 1.6 ⚙️ Criar o usuário e pegar o `LOCAL_USER_ID`

Cadastre-se em `https://seudominio.com.br/login` e depois:

```sql
select id, email from auth.users;
```

O `id` vai em `LOCAL_USER_ID` no `.env` da sua máquina — os workers rodam localmente e
não têm sessão de autenticação.

→ [cfg §1 — Identidade local](cfg.fornecedores.md#identidade-local--obrigatória-para-workers-e-mcp)

### 1.7 ⚙️ Regenerar os tipos

`packages/shared-types/src/database.types.ts` foi editado à mão para as colunas e RPCs
criadas em P0, porque o `supabase gen types` precisa de docker acessível de dentro do
container e o stack local não publica a porta do Postgres. Contra o projeto hospedado
isso não é problema:

```bash
bun run generate-types
git diff packages/shared-types/  # confirmar que bate com o schema real
```

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

Distinto dos secrets do GitHub: estas são as que a aplicação lê em runtime.

Em **Settings → Environment Variables**, ambiente Production:

| Variável                               | Valor                                        |
| -------------------------------------- | -------------------------------------------- |
| `NEXT_PUBLIC_SUPABASE_URL`             | `https://<ref>.supabase.co`                  |
| `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | `sb_publishable_...` — **nunca um `eyJ...`** |
| `NEXT_PUBLIC_APP_URL`                  | `https://seudominio.com.br`                  |
| `OPENAI_API_KEY`                       | só se quiser chat e sugestões na web         |

> ⚠️ **`SUPABASE_SECRET_KEY` NÃO vai no Vercel.** Ignora RLS. Ela pertence à sua
> máquina, onde rodam os workers e o MCP. A chave secreta nova ainda tem uma proteção a
> mais que a `service_role` não tinha: responde **HTTP 401 se for usada a partir de um
> navegador**. Rede de segurança, não substituto de cuidado.

→ [cfg §1 — variáveis](cfg.fornecedores.md#1-variáveis-de-ambiente)

### 2.3 🔒 Domínio

**Settings → Domains**. O valor precisa bater com o Site URL do passo 1.5.

---

## 3. GitHub

### 3.1 🔒 Secrets

**Settings → Secrets and variables → Actions → New repository secret**. Seis:

| Secret                  | Onde obter                |
| ----------------------- | ------------------------- |
| `SUPABASE_ACCESS_TOKEN` | Account → Access Tokens   |
| `SUPABASE_PROJECT_ID`   | passo 1.2                 |
| `SUPABASE_DB_PASSWORD`  | Settings → Database       |
| `VERCEL_TOKEN`          | Vercel → Account → Tokens |
| `VERCEL_ORG_ID`         | passo 2.1                 |
| `VERCEL_PROJECT_ID`     | passo 2.1                 |

O repositório é **público**: por isso os workflows referenciam `${{ secrets.X }}` em vez
de conter valor algum.

### 3.2 🔒 Environment `production` com aprovação manual

**Settings → Environments → New environment** → nome `production` → marcar **Required
reviewers** e adicionar você.

Sem staging, esse clique é a última barreira entre uma migration ruim e o banco de
verdade. O deploy pausa e te notifica antes de aplicar qualquer coisa.

### 3.3 ⚙️ Primeira execução

O CI nunca rodou — a paridade com o pipeline antigo do GitLab só se confirma na
primeira execução.

```bash
git push origin feat/api-key
```

Acompanhe em **Actions**. Espere: lint, format, typecheck, 505 testes e build verdes.

### 3.4 ⚙️ Deploy

**Actions → Deploy → Run workflow**. A sequência é: dump do banco como artifact →
`db push` → deploy das edge functions → build e deploy no Vercel. Vai pausar esperando
sua aprovação.

---

## 4. Depois de subir

| #   | O quê                                                      | Onde                                                            |
| --- | ---------------------------------------------------------- | --------------------------------------------------------------- |
| 4.1 | Ciclo financeiro (dia do seu mês) + gerar períodos por SQL | [cfg §3](cfg.fornecedores.md#3-ciclo-financeiro)                |
| 4.2 | **Ao menos uma instituição e um produto financeiro**       | [cfg §4](cfg.fornecedores.md#4-instituições-produtos-e-cartões) |
| 4.3 | Categorias e tags                                          | [cfg §5](cfg.fornecedores.md#5-categorias-e-tags)               |
| 4.4 | Fornecedores e aliases                                     | [cfg §6](cfg.fornecedores.md#6-fornecedores)                    |
| 4.5 | Cartões — **por SQL, não há tela**                         | [cfg §4.3](cfg.fornecedores.md#43-cartões--️-não-há-tela)        |
| 4.6 | Senhas de PDF, se tiver fatura protegida                   | [cfg §7](cfg.fornecedores.md#7-senhas-de-documentos-protegidos) |
| 4.7 | Gmail: OAuth com redirect de produção                      | [cfg §8](cfg.fornecedores.md#8-gmail)                           |

> **4.2 é o que trava tudo.** `transactions.financial_product_id` é `NOT NULL` e o
> pipeline não tem como adivinhar a conta. Sem um produto cadastrado, nenhum documento
> pode ser lançado.

---

## 5. Confirmar que funcionou

```bash
# Aplicação responde
curl -sI https://seudominio.com.br | head -1

# Migrations aplicadas
supabase migration list

# A chave de criptografia existe (deve retornar 1)
psql "$DATABASE_URL" -tAc "select count(*) from private.crypto_keys;"
```

Na aplicação: fazer login → cadastrar instituição e produto → subir um documento em
`/dashboard/ingestion` → aprovar e lançar em `/dashboard/ingestion/review` → confirmar
que a transação aparece em `/dashboard/transactions`.

→ [cfg — Verificar se está tudo certo](cfg.fornecedores.md#verificar-se-está-tudo-certo)

---

## 6. O que **não** vai funcionar mesmo com tudo acima

Não são bloqueadores de subida, mas evitam surpresa:

| Limitação                         | Detalhe                                                                                                                                |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Cartões só por SQL                | Rota `/dashboard/cards` não existe — [issue #4](https://github.com/ChicoFigueiredo/Seu-Bolso-Feliz/issues/4)                           |
| Senhas de PDF sem tela            | A action existe; a interface não                                                                                                       |
| Só PDF tem parser                 | XLSX, OFX, DOCX e QIF são anunciados na UI e não são lidos — [issue #13](https://github.com/ChicoFigueiredo/Seu-Bolso-Feliz/issues/13) |
| Imagens sem OCR                   | Só PDF escaneado, e ainda assim desligado por padrão — [issue #15](https://github.com/ChicoFigueiredo/Seu-Bolso-Feliz/issues/15)       |
| Sem agenda consolidada            | A home mostra três listas separadas                                                                                                    |
| Sem limite comprometido           | `cards.credit_limit` não é lido por nenhum código                                                                                      |
| Upload acima de ~4,5 MB falha     | Server Action do Vercel tem esse teto; corrigido em F6.4                                                                               |
| MCP `recompute_financial_periods` | Nunca funcionou; use o SQL de [cfg §3](cfg.fornecedores.md#3-ciclo-financeiro)                                                         |

---

## Resumo: o que só depende de você

1. **Neutralizar as chaves vazadas** (1.1) — criar as `sb_publishable_...` / `sb_secret_...`, trocar os valores e **desativar as legacy**. Não existe "Rotate" para chave legacy; a substituição é o caminho. O código já está pronto para as novas
2. Fazer **backup da chave de criptografia** (1.4) — perdê-la é irreversível
3. URLs de Auth no Supabase (1.5)
4. Projeto, variáveis e domínio no Vercel (2.1–2.3)
5. Seis secrets e o environment com aprovação no GitHub (3.1–3.2)
6. Decidir se o repositório continua **público**
