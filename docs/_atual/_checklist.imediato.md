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

### 1.1 🔒 Neutralizar as chaves expostas — **antes de qualquer outra coisa**

As chaves vazaram no commit `38b8126`. Enquanto valerem, qualquer pessoa com acesso ao
histórico do repositório **público** entra no banco.

> **Não existe botão "Rotate" para elas, e não é falta de procurar.** As suas chaves são
> as **legacy** (`anon` e `service_role`), que derivam do JWT secret do projeto. O
> Supabase **removeu a capacidade de rotacioná-las** — a documentação diz textualmente
> que _"não é mais possível rotacionar as legacy anon, service e JWT secrets"_.
>
> O caminho hoje é outro: **criar chaves novas e desativar as antigas**. O efeito de
> segurança é o mesmo — as chaves vazadas param de funcionar — e passa a existir rotação
> de verdade daqui em diante.

**Como estão as suas hoje** (verificado): os _nomes_ das variáveis já são os novos
(`NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY`, `SUPABASE_SECRET_KEY`), mas os _valores_ são
JWTs legacy (começam com `eyJ...`). Só os valores mudam — **nenhum código precisa ser
alterado**.

| Chave legacy   | Substituta  | Quem usa                      |
| -------------- | ----------- | ----------------------------- |
| `anon`         | publishable | navegador (`NEXT_PUBLIC_...`) |
| `service_role` | secret      | workers, MCP, Edge Functions  |

#### Passo a passo

**a. Criar as chaves novas** — **Settings → API Keys** → aba **Publishable and secret
API keys**. Se aparecer **Create new API keys**, clique. É seguro: as novas nascem ao
lado das legacy, e as legacy continuam funcionando. Vêm com o nome `default`.

**b. Trocar os valores.** Em cada lugar, substitua o `eyJ...` pelo novo:

| Onde                                    | Variável                               | Valor novo                                          |
| --------------------------------------- | -------------------------------------- | --------------------------------------------------- |
| `.env`, `.env.local`, `.env.production` | `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | `sb_publishable_...`                                |
| idem                                    | `SUPABASE_SECRET_KEY`                  | `sb_secret_...`                                     |
| Vercel → Environment Variables          | `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | `sb_publishable_...`                                |
| GitHub → Secrets                        | —                                      | não guarda chave de API, só `SUPABASE_ACCESS_TOKEN` |

**c. Edge Functions.** Você tem quatro (`merge-suppliers`,
`refresh-mv-supplier-spending`, `retroactive-supplier-association`,
`trigger-ingestion`), e todas leem `Deno.env.get("SUPABASE_SECRET_KEY")`. O Supabase
injeta as novas num formato diferente — um JSON indexado por nome:

```ts
// antes
const secretKey = Deno.env.get("SUPABASE_SECRET_KEY")!;

// depois
const secretKey = JSON.parse(Deno.env.get("SUPABASE_SECRET_KEYS")!)["default"];
```

> ⚠️ **A armadilha que faz tudo dar `Invalid JWT`:** as chaves novas **não são JWT**.
> Se forem enviadas no header `Authorization: Bearer`, a plataforma tenta interpretá-las
> como token e rejeita. Elas vão no header `apikey`. O `supabase-js` já faz isso
> sozinho, mas o `verify_jwt` embutido das Edge Functions só entende chave legacy —
> então cada função precisa de `verify_jwt = false` no `supabase/config.toml` e
> autorização no próprio código:
>
> ```toml
> [functions.merge-suppliers]
> verify_jwt = false
> ```
>
> Hoje o seu `config.toml` **não define `verify_jwt` para nenhuma função**.

> Nota: `trigger-ingestion` está quebrada contra o schema real (insere quatro colunas
> que não existem) e o plano prevê deletá-la. Não gaste tempo migrando essa.

**d. Conferir que nada mais usa as legacy.** Não há indicador automático de uso — é
conferência manual. Além do óbvio: MCP na sua máquina, scripts, e qualquer `.env` que
você tenha em outro lugar.

**e. Desativar as legacy** — mesma tela **Settings → API Keys**. **É este passo que
neutraliza o vazamento.** É reversível: se descobrir um cliente esquecido, dá para
reativar.

#### Depois: JWT signing keys (migração separada)

As chaves de API novas não tocam mais o JWT secret, mas os tokens que o Supabase Auth
emite para os seus usuários **ainda são assinados por ele**. Migrar para
**Settings → JWT Keys → JWT Signing Keys** tira o projeto inteiro do segredo
compartilhado e habilita rotação sem downtime (aí sim com botão **Rotate Keys**, e um
**Revoke** para invalidar a chave anterior).

Não é urgente como (e), mas é o que fecha o assunto de vez.

> As legacy funcionam até o **fim de 2026**. Para uso normal isso daria tempo de sobra —
> mas as suas **vazaram**, então o passo (e) é para agora.

→ [cfg §1 — variáveis](cfg.fornecedores.md#1-variáveis-de-ambiente)
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

| Variável                               | Valor                                |
| -------------------------------------- | ------------------------------------ |
| `NEXT_PUBLIC_SUPABASE_URL`             | `https://<ref>.supabase.co`          |
| `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | chave publicável **rotacionada**     |
| `NEXT_PUBLIC_APP_URL`                  | `https://seudominio.com.br`          |
| `OPENAI_API_KEY`                       | só se quiser chat e sugestões na web |

> ⚠️ **`SUPABASE_SECRET_KEY` NÃO vai no Vercel.** É a `service_role`, que ignora RLS.
> Ela pertence à sua máquina, onde rodam os workers e o MCP.

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

1. **Neutralizar as chaves vazadas** (1.1) — criar publishable/secret novas e **desativar** as legacy. Não existe "Rotate" para chave legacy: o Supabase removeu essa capacidade
2. Fazer **backup da chave de criptografia** (1.4) — perdê-la é irreversível
3. URLs de Auth no Supabase (1.5)
4. Projeto, variáveis e domínio no Vercel (2.1–2.3)
5. Seis secrets e o environment com aprovação no GitHub (3.1–3.2)
6. Decidir se o repositório continua **público**
