# Configuração — parâmetros fundamentais para o sistema funcionar

> Verificado contra o código em 2026-07-27. Cada parâmetro aqui foi conferido no
> arquivo que o lê. Onde algo **não existe ainda**, está marcado como tal em vez de
> descrito como se existisse.

---

## Ordem de execução

Faça nesta ordem. Cada passo depende do anterior.

| #   | Passo                   | Sem isto                               |
| --- | ----------------------- | -------------------------------------- |
| 1   | Variáveis de ambiente   | Nada sobe                              |
| 2   | Banco e usuário         | Nenhuma tela abre                      |
| 3   | Ciclo financeiro        | Relatórios usam mês-calendário         |
| 4   | Instituições e produtos | **Nenhum documento pode ser lançado**  |
| 5   | Categorias e tags       | Lançamentos sem classificação          |
| 6   | Fornecedores e aliases  | Cada documento vira um fornecedor novo |
| 7   | Senhas de PDF           | Faturas protegidas não abrem           |
| 8   | Gmail                   | Sem ingestão por e-mail                |
| 9   | OpenAI                  | Sem enriquecimento por IA              |
| 10  | OCR                     | PDFs escaneados não extraem texto      |

---

## 1. Variáveis de ambiente

Copie `.env.example` para `.env` e complete.

> O Supabase saiu inteiramente do projeto (ADR-009, 2026-08-30). Não existe mais
> camada de chave de API para o banco, RLS, Edge Function nem Supabase Auth — o que
> segue é o mapa de "por onde cada peça do backend anterior entra hoje":
>
> | Antes (Supabase)                                        | Hoje                                                                                                                 |
> | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
> | `SUPABASE_URL` + `SUPABASE_SECRET_KEY`/`sb_secret_...`  | `DATABASE_URL` / `DATABASE_URL_UNPOOLED` (Neon, Postgres puro) — ver §1.1                                            |
> | RLS (`auth.uid() = user_id`)                            | `WHERE user_id = ownerId` explícito em cada query — não há mais política de banco segurando isto                     |
> | Supabase Auth (`auth.users`)                            | autenticação própria: JWT em cookie httpOnly + Google OAuth + senha de reserva — ver §1.2                            |
> | Supabase Storage (bucket)                               | Vercel Blob (`BLOB_READ_WRITE_TOKEN`) — ver §1.3 e `docs/integrations/storage-vercel-blob.md`                        |
> | Edge Function (`trigger-ingestion`)                     | fila na própria tabela `ingestion_jobs` (`LISTEN`/`NOTIFY`) — ver ADR-010                                            |
> | `LOCAL_USER_ID` obrigatório em todo processo sem sessão | opcional para os workers (`resolveOwnerId()` acha o dono sozinho); ainda obrigatório só para o MCP server — ver §1.4 |

### 1.1 Banco — `DATABASE_URL` / `DATABASE_URL_UNPOOLED`

| Variável                | Obrigatória                        | Onde é lida                                                             | Formato                                             |
| ----------------------- | ---------------------------------- | ----------------------------------------------------------------------- | --------------------------------------------------- |
| `DATABASE_URL`          | sim                                | `getDb()` (`@sbf/db`) — web, workers, MCP, scripts                      | connection string Neon, **pooled** (host `-pooler`) |
| `DATABASE_URL_UNPOOLED` | sim, para a fila e para migrations | worker de ingestão (`LISTEN`/`NOTIFY`), `scripts/aplicar-migrations.sh` | connection string Neon, **direta** (sem `-pooler`)  |

O pooler do Neon não entrega `LISTEN`/`NOTIFY` de forma confiável, e DDL pelo pooler
pode cair num backend diferente no meio da transação — por isso as duas URLs existem e
não são intercambiáveis. `packages/db/README.md` documenta as pegadinhas do driver
(`neon-http` não tem `transaction()`; para atomicidade use `batch()`).

Não há mais "ambiente local do Supabase" com chaves fixas embutidas no CLI: todo
ambiente — local ou produção — é um Postgres real endereçado por `DATABASE_URL`. Para
testes de integração/E2E, a prática é um Postgres **descartável** (nunca a base de
trabalho): receita completa em `__tests__/helpers/db.ts` (resumo: `docker run` com
`postgres:17-alpine`, aplicar `packages/db/schema.sql` + `packages/db/migrations/*.sql`,
apontar `INTEGRATION_TEST_DATABASE_URL` para ele).

### 1.2 Autenticação própria — `AUTH_SECRET`, Google OAuth, senha de reserva

| Variável                           | Obrigatória                        | Efeito                                                                                                                                          |
| ---------------------------------- | ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `AUTH_SECRET`                      | sim (≥32 caracteres)               | assina o cookie de sessão (`sbf_session`, JWT via `jose`); trocar invalida toda sessão em aberto                                                |
| `AUTH_ALLOWED_EMAILS`              | não (default `fran.fig@gmail.com`) | única lista de e-mails que o login por Google aceita                                                                                            |
| `GOOGLE_OAUTH_CLIENT_ID`/`_SECRET` | sim, para login por Google         | credenciais OAuth do Google Cloud Console — URL de retorno autorizada precisa incluir `<origem>/auth/callback`                                  |
| `ADMIN_INITIAL_PASSWORD`           | sim (≥12 caracteres)               | senha inicial do usuário `admin@` (chave de reserva, para quando o Google falhar); abaixo de 12 caracteres o login por senha fica **desligado** |

Não existe mais `auth.users` nem Supabase Auth. A identidade vive em `public.users`
(`role`: `owner` | `admin`; `owner_id` — o admin opera sobre os dados do dono, nunca
sobre os próprios). Implementação completa em `apps/web/src/lib/auth/` (JWT com `jose`,
Google OAuth escrito à mão, senha com `scrypt`) e semente inicial em
`packages/db/migrations/0002_crypto_key_e_usuarios.sql`.

### 1.3 Storage — `BLOB_READ_WRITE_TOKEN`

Único segredo do Vercel Blob — carrega o id da loja, sem endpoint, região nem par de
chaves para configurar. Detalhe completo, inclusive onde pegar o token e por que
"bucket" aqui é só prefixo de caminho, em
[`docs/integrations/storage-vercel-blob.md`](../integrations/storage-vercel-blob.md).

### 1.4 Identidade local — obrigatória só para o MCP server

| Variável        | Obrigatória                                           | Onde é lida                                                                                                             |
| --------------- | ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `LOCAL_USER_ID` | **sim, para o MCP server** · opcional para os workers | `apps/mcp-server/src/user.ts` (exige e lança se faltar) · `resolveOwnerId()` em `@sbf/db` (workers — override opcional) |

Os workers descobrem o dono sozinhos: existe exatamente uma linha com `role = 'owner'`
em `public.users`, e é dela que sai o id (`resolveOwnerId()`, `packages/db/src/owner.ts`).
`LOCAL_USER_ID` continua aceito ali como override, mas agora é **validado** — se
apontar para alguém que não existe, o processo recusa subir em vez de gravar linhas
órfãs em silêncio.

O MCP server é diferente: roda como processo stdio, sem sessão nenhuma, e
`apps/mcp-server/src/user.ts` **exige** `LOCAL_USER_ID` explicitamente — não usa
`resolveOwnerId()` nem cai em automático.

Para descobrir o id do dono:

```sql
select id, email, role from public.users;
```

### 1.5 Limpando um `.env` antigo (se a máquina vem de antes da ADR-009)

Estas variáveis não são lidas por nenhum código vivo e podem sair do `.env`:
`NEXT_PUBLIC_SUPABASE_URL`, `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY`, `SUPABASE_SECRET_KEY`,
`SUPABASE_URL`, `SUPABASE_DB_PASSWORD`, `SUPABASE_ACCESS_TOKEN`, `SUPABASE_PROJECT_ID`, e
qualquer `STAGING_*` (o staging foi eliminado — ver `.github/workflows/deploy.yml` —
antes mesmo da saída do Supabase). Confirme que sumiram:

```bash
grep -E "SUPABASE|STAGING_" .env* 2>/dev/null   # não deve imprimir nada
```

Fora do repositório: **⚠️ verificar** se o projeto Supabase original (o de produção, e o
de staging `seu-bolso-feliz-staging`, se ainda existir) foi desativado. Remover a
variável do `.env` não desativa nada do lado do Supabase — enquanto o projeto existir
lá, as chaves antigas continuam abrindo aquele banco.

### Gmail — só se for usar ingestão por e-mail

| Variável                    | Onde é lida                                 |
| --------------------------- | ------------------------------------------- |
| `GOOGLE_MAIL_CLIENT_ID`     | `workers/gmail-scanner/src/gmail-client.ts` |
| `GOOGLE_MAIL_CLIENT_SECRET` | idem                                        |
| `GMAIL_REFRESH_TOKEN`       | idem                                        |

### OpenAI — só se for usar enriquecimento por IA

| Variável            | Padrão        | Onde é lida                                              |
| ------------------- | ------------- | -------------------------------------------------------- |
| `OPENAI_API_KEY`    | —             | `parsers/ai-*-enricher.ts`, `api/chat`, `api/ai-suggest` |
| `OPENAI_LITE_MODEL` | `gpt-4o-mini` | `ai-lite-enricher.ts`                                    |
| `OPENAI_FULL_MODEL` | `gpt-4o`      | `ai-full-enricher.ts`                                    |

### Operação dos workers — opcionais

| Variável                    | Padrão     | Efeito                                                             |
| --------------------------- | ---------- | ------------------------------------------------------------------ |
| `INGESTION_ENABLE_OCRMYPDF` | `false`    | **Desligado por padrão.** Sem isto, PDF escaneado não extrai texto |
| `OCRMYPDF_BIN`              | `ocrmypdf` | Caminho do binário                                                 |
| `POLL_INTERVAL_MS`          | 5000       | Intervalo do worker de ingestão                                    |
| `BATCH_SIZE`                | 10         | Jobs por rodada                                                    |
| `WATCH_DIR`                 | `./inbox`  | Pasta vigiada pelo scanner local                                   |
| `SCAN_INTERVAL_MS`          | 30000      | Intervalo do scanner local em modo watch                           |

---

## 2. Banco e usuário

Num Neon **já provisionado com o schema base** (o caso comum — produção, ou um banco
que alguém já rodou antes):

```bash
bash scripts/aplicar-migrations.sh    # aplica packages/db/migrations/*.sql pendentes
bun run generate-types                # regenera packages/shared-types
```

Num Postgres **do zero** (ex.: banco de teste descartável), aplique primeiro o
baseline — `packages/db/schema.sql` não é histórico, é o estado atual congelado, e as
migrations são só o delta desde ali (ver `packages/db/README.md`):

```bash
psql "$DATABASE_URL_UNPOOLED" -f packages/db/schema.sql
bash scripts/aplicar-migrations.sh
```

O dono (`fran.fig@gmail.com`) e o admin de emergência (`admin@seubolsofeliz.com.br`) já
nascem semeados por `packages/db/migrations/0002_crypto_key_e_usuarios.sql` — não é
preciso criar usuário nenhum. O primeiro login pelo Google casa por e-mail e preenche
`google_sub`; o login por senha usa `ADMIN_INITIAL_PASSWORD` (§1.2). Para achar o `id`
de alguém (uso em `LOCAL_USER_ID`, se for forçar um usuário específico):

```sql
select id, email, role from public.users;
```

---

## 3. Ciclo financeiro

Tela: **`/dashboard/settings`**. Tabela: `user_financial_preferences`.

| Campo                         | Significado                                                                |
| ----------------------------- | -------------------------------------------------------------------------- |
| `financial_cycle_start_day`   | Dia em que o seu mês financeiro começa (1–31). Se você recebe dia 5, use 5 |
| `financial_cycle_anchor_date` | Data-âncora para gerar os períodos                                         |
| `default_currency`            | `BRL`                                                                      |

Depois de definir, gere os períodos. `generate_financial_periods` era uma RPC do
Postgres — uma das seis descartadas na migração para o Neon (dependia só de
`auth.uid()`, ver nota em `packages/db/schema.sql`) — hoje é a Server Action
`generateFinancialPeriods` (`apps/web/src/app/actions/financial-periods.ts`), que reusa
`generatePeriods` de `@sbf/domain`. Não há tela que a chame diretamente; rode via MCP:

```
recompute_financial_periods { "monthsAhead": 12 }
```

> ⚠️ **Duas ressalvas:**
>
> 1. A tela de settings **não chama** `generateFinancialPeriods` ao salvar o dia do
>    ciclo. É preciso disparar à parte (MCP, acima, ou chamando a Server Action).
> 2. A tool MCP `recompute_financial_periods` **já não tem o defeito antigo**
>    (`auth.uid()`/`service_role` eram conceitos do Supabase; a RPC de que dependia nem
>    existe mais). Foi reimplementada em TypeScript em
>    `apps/mcp-server/src/tools/recompute-financial-periods.ts`, com chamador real
>    (registrada em `apps/mcp-server/src/index.ts`) — mas sem teste automatizado
>    dedicado ainda, então trate como 🟡 e confirme o resultado depois de rodar.

---

## 4. Instituições, produtos e cartões

**Este é o passo que trava tudo.** `transactions.financial_product_id` é `NOT NULL`, e
nada no pipeline consegue adivinhar em que conta um documento entra. **Sem ao menos um
produto financeiro cadastrado, nenhum documento pode ser lançado.**

### 4.1 Instituição — tela `/dashboard/institutions`

| Campo  | Valores                                 |
| ------ | --------------------------------------- |
| `name` | Nome do banco                           |
| `type` | `bank` · `fintech` · `broker` · `other` |

### 4.2 Produto financeiro — tela `/dashboard/products`

| Campo            | Valores                                                                                                                      |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `institution_id` | obrigatório                                                                                                                  |
| `name`           | Ex.: "Conta Corrente", "Cartão Principal"                                                                                    |
| `type`           | `checking_account` · `savings_account` · `credit_card` · `overdraft` · `personal_loan` · `mortgage` · `investment` · `other` |
| `credit_limit`   | Limite, para produtos de crédito                                                                                             |

### 4.3 Cartões — tela `/dashboard/cards`

A rota `/dashboard/cards` existe (`apps/web/src/app/dashboard/cards/page.tsx`), com
lista, criação e edição — cadastre o cartão por ali. Se precisar por SQL mesmo assim
(script, backfill):

```sql
insert into cards (
  user_id, financial_product_id, last_four_digits, card_brand,
  credit_limit, closing_day, due_day, holder_name
) values (
  '<seu-user-id>',
  '<id-do-financial_product-do-tipo-credit_card>',
  '1234',            -- últimos 4 dígitos
  'visa',
  5000.00,           -- limite
  28,                -- dia do fechamento
  10,                -- dia do vencimento
  'SEU NOME'
);
```

| Campo              | Por que importa                              |
| ------------------ | -------------------------------------------- |
| `closing_day`      | Fechamento da fatura (1–31)                  |
| `due_day`          | Vencimento (1–31)                            |
| `credit_limit`     | Base do cálculo de limite comprometido       |
| `last_four_digits` | Usado para casar a fatura com o cartão certo |

> ⚠️ `cards.credit_limit` e `financial_products.credit_limit` coexistem sem regra de
> precedência definida. Para cartão, preencha **`cards.credit_limit`**.

---

## 5. Categorias e tags

Tela: **`/dashboard/settings`**.

**Categorias** aceitam hierarquia (`parent_id`). Ex.: `Moradia` → `Energia`, `Água`.

**Tags** têm um comportamento que vale conhecer:

| Campo                 | Efeito                                               |
| --------------------- | ---------------------------------------------------- |
| `influences_priority` | Se `true`, a tag participa do cálculo de prioridade  |
| `suggested_priority`  | `essential` · `high` · `medium` · `low` · `optional` |

A prioridade efetiva de um item é: prioridade manual, se houver; senão a **maior**
prioridade entre as tags com `influences_priority = true`; senão `medium`. É isso que
ordena a fila de pagamentos na home.

---

## 6. Fornecedores

Tela: **`/dashboard/suppliers`**.

### 6.1 Fornecedor

| Campo             | Observação                                                                                      |
| ----------------- | ----------------------------------------------------------------------------------------------- |
| `name`            | Nome canônico. É o que aparece nos relatórios                                                   |
| `document_number` | **CNPJ. Preencha sempre que souber** — é o casamento mais forte                                 |
| `type`            | `company` · `individual` · `government` · `utility` · `telecom` · `saas` · `platform` · `other` |
| `institution_id`  | Só se o fornecedor for também uma instituição financeira                                        |

### 6.2 Aliases — o que evita fornecedor duplicado

Cada documento escreve o nome do fornecedor de um jeito. A fatura diz `CEMIG
DISTRIBUICAO S.A.`, o extrato diz `CEMIG DISTRIB`, o boleto diz `COMPANHIA ENERGETICA
MG`. **Sem aliases, cada variação vira um fornecedor novo, e o relatório fragmenta o
mesmo gasto em três linhas.**

| `alias_type`   | Quando usar                       |
| -------------- | --------------------------------- |
| `former_name`  | Nome antigo da empresa            |
| `abbreviation` | Como aparece abreviado no extrato |
| `trade_name`   | Nome fantasia                     |
| `billing_name` | Como aparece na cobrança          |
| `other`        | Demais                            |

`valid_from` / `valid_until` servem para trocas de nome com data — o alias antigo
continua casando documentos do período em que valia.

### 6.3 Contratos — para contas com identificador

Use quando o fornecedor tem mais de uma conta sua. Ex.: duas instalações da CEMIG.

| Campo           | Observação                                                                              |
| --------------- | --------------------------------------------------------------------------------------- |
| `identifier`    | **O número da instalação/contrato como aparece no documento**                           |
| `contract_type` | `service` · `subscription` · `utility` · `loan` · `insurance` · `maintenance` · `other` |
| `label`         | Apelido: "Casa", "Escritório"                                                           |

### 6.4 Ferramentas de manutenção

Três RPCs existem para arrumar a base depois que ela cresce:

```sql
-- Buscar fornecedor por nome aproximado ou CNPJ
select * from search_suppliers('cemig');

-- Fundir duplicados: move tudo do segundo para o primeiro
select merge_suppliers('<id-que-fica>', '<id-que-some>');

-- Confirmar associações sugeridas em lote
select confirm_supplier_associations('<supplier_id>');
```

### 6.5 Templates de extração — configuração em código

Além do cadastro no banco, há **templates de extração** em
`workers/ingestion/src/parsers/supplier-templates.ts`. Eles ensinam o parser a achar
valor, vencimento e competência no layout específico daquele emissor.

Hoje existem **dois**: CEMIG e VIVO.

Para adicionar um, acrescente ao array `TEMPLATE_DEFS`:

```ts
{
  id: "tpl-<fornecedor>-<tipo>-v1",
  supplierName: "NOME COMO APARECE NO DOCUMENTO",
  // Como reconhecer que o documento é deste fornecedor
  supplierHints: [/\bNOME\b/i, /RAZAO\s+SOCIAL/i],
  fieldRegex: {
    totalAmount:     [/VALOR\s+(?:A\s+)?PAGAR[:\s]*R?\$?\s*([\d.,]+)/i],
    dueDate:         [/VENCIMENTO[:\s]*(\d{2})[/-](\d{2})[/-](\d{4})/i],
    competenceDate:  [/REFER[EÊ]NCIA[:\s]*(\d{2})[/-](\d{4})/i],
    documentNumber:  [/(?:NF|FATURA)[:\s]*(\S+)/i],
    supplierCnpj:    [/CNPJ[:\s]*(\d{2}\.\d{3}\.\d{3}\/\d{4}-\d{2})/i],
  },
}
```

Cada campo aceita **uma lista** de regex: a primeira que casar vence. Isso permite
cobrir variações de layout do mesmo emissor sem duplicar o template.

> Existe também a tabela `document_patterns`, com `extraction_rules` e `field_mappings`
> em JSONB, e a tela `/dashboard/ingestion/patterns`. A intenção é permitir configurar
> padrões **sem mexer em código** — mas a ligação entre a tabela e o parser ainda não
> fecha. Por ora, o caminho que funciona é o template em código acima.

---

## 7. Senhas de documentos protegidos

A senha nunca fica em texto puro: é criptografada dentro do banco, com chave que não
sai de lá.

### Cadastrar

Pela tela **`/dashboard/settings/passwords`**, que chama a Server Action `setSecret`
(`apps/web/src/app/actions/secrets.ts`).

`fn_set_secret` — a RPC que fazia isto antes, autenticando por `auth.uid()` — foi uma
das seis funções **descartadas** na migração para o Neon (dependia só de identidade
Supabase; nota em `packages/db/schema.sql`). Hoje **não há caminho por SQL direto que
se autentique sozinho**: `setSecret` resolve `userId` da sessão (`requireUser()`) e
chama `assertOwnsEntity()` antes de gravar — é essa checagem em TypeScript, não mais
`auth.uid()` no banco, que impede gravar em nome de outro dono ou apontar para uma
entidade alheia. Quem precisar fazer isso por script (ex.: um worker) usa
`gravarSegredo()` (`packages/db/src/secrets.ts`), que **não** checa posse sozinha — a
responsabilidade de provar que a entidade pertence ao usuário é de quem chama.

| `entity_type`       | Escopo                           |
| ------------------- | -------------------------------- |
| `null`              | Vale para qualquer documento seu |
| `supplier`          | Só documentos daquele fornecedor |
| `institution`       | Só daquela instituição           |
| `financial_product` | Só daquele produto               |
| `card`              | Só daquele cartão                |
| `supplier_contract` | Só daquele contrato              |

Há **um segredo por escopo** (índice único). Regravar o mesmo escopo substitui.

### Como o pipeline usa

No momento em que a senha é necessária, o documento ainda não foi lido — logo o
fornecedor **ainda é desconhecido**. O worker então tenta as senhas cadastradas em
ordem de uso recente, com teto de 20 (`listPdfPasswordCandidates` em
`workers/ingestion/src/parsers/secret-lookup.ts`, que lê pela função `fn_get_secrets`
— esta ainda existe no Neon, mas sem `service_role` nem a checagem de posse que tinha
no Supabase; ver §1), e marca a que funcionou para subir na ordem da próxima vez.

---

## 8. Gmail

```bash
bun run get:gmail-token
```

Antes disso, no Google Cloud Console:

1. Crie um projeto e ative a **Gmail API**.
2. Crie credencial OAuth 2.0 do tipo **Aplicativo para computador**.
3. Adicione `http://localhost:8976` como URI de redirecionamento.
4. Ponha o client id e o secret no `.env`.

O script sobe um servidor local na porta 8976, abre o navegador, e ao autorizar
imprime o `GMAIL_REFRESH_TOKEN` para você colar no `.env`.

### Uso

```bash
# Escanear uma label
bun run cli -- --gmail --label Comprovantes

# Varredura por período (funciona a partir de 2026-07-27)
bun run cli -- --gmail --query "from:nubank has:attachment" \
  --from-date 2026-01-01 --to-date 2026-06-30

# Escanear e processar na mesma execução
bun run cli -- --both --process
```

`--to-date` é **inclusivo**: o CLI soma um dia internamente, porque o `before:` do
Gmail é exclusivo.

Crie uma label no Gmail e um filtro que mande para ela os e-mails de cobrança. O
scanner varre a label, não a caixa inteira.

---

## 9. OpenAI

`OPENAI_API_KEY` no `.env`. Os modelos padrão (`gpt-4o-mini` para o passe leve,
`gpt-4o` com visão para o completo) podem ser trocados por `OPENAI_LITE_MODEL` e
`OPENAI_FULL_MODEL`.

Modo por documento, via `--ai-mode`:

| Modo   | Comportamento                                                           |
| ------ | ----------------------------------------------------------------------- |
| `auto` | Só chama IA se faltarem campos críticos após os parsers determinísticos |
| `lite` | Força o passe leve                                                      |
| `full` | Força o passe com visão                                                 |
| `skip` | Só determinístico, sem custo de IA                                      |

---

## 10. OCR

**Desligado por padrão.** Para ligar:

```bash
INGESTION_ENABLE_OCRMYPDF=true
OCRMYPDF_BIN=ocrmypdf
```

Instalação:

```bash
sudo apt install ocrmypdf tesseract-ocr-por   # Debian/Ubuntu/WSL
```

> ⚠️ Isto vale só para **PDF escaneado**. Para **imagens** (`.jpg`, `.png`) não há OCR
> nenhum: o extrator devolve texto vazio. Rastreado como issue #15.

---

## Verificar se está tudo certo

```bash
# 0. Nenhuma variável do Supabase sobrou no seu .env (não deve imprimir nada — §1.5)
grep -E "SUPABASE|STAGING_" .env* 2>/dev/null

# 1. Ambiente e banco
bash scripts/aplicar-migrations.sh

# 2. Suíte completa (contagem de referência é anterior à saída do Supabase — reconfirme)
bun run test

# 3. Só o ciclo ponta a ponta
bunx vitest run --project e2e

# 4. Um documento de verdade
mkdir -p inbox && cp ~/algum-boleto.pdf inbox/
bun run cli -- --local --dir ./inbox --process --verbose
```

Depois, em `/dashboard/ingestion/review`: escolha a conta, aprove e lance. A transação
aparece em `/dashboard/transactions`.

### Checagens rápidas por SQL

```sql
-- Falta produto financeiro? Se der 0, nada pode ser lançado.
select count(*) from financial_products where user_id = '<seu-id>';

-- Fornecedores possivelmente duplicados
select name, count(*) from suppliers where user_id = '<seu-id>'
group by name having count(*) > 1;

-- Documentos travados
select status, count(*) from ingestion_jobs where user_id = '<seu-id>'
group by status;

-- Drafts aguardando revisão
select draft_type, status, count(*) from draft_records where user_id = '<seu-id>'
group by draft_type, status;
```

---

## Resumo do que ainda não é configurável pela interface

| O quê                                    | Alternativa hoje                                                    | Rastreado                           |
| ---------------------------------------- | ------------------------------------------------------------------- | ----------------------------------- |
| Cartões (fechamento, vencimento, limite) | `/dashboard/cards` — já tem tela (issues #4/#5 fechadas por código) | issue #6 (`credit_limit` duplicado) |
| Senhas de PDF                            | `/dashboard/settings/passwords` — já tem tela                       | —                                   |
| Templates de extração                    | código                                                              | —                                   |
| Regeneração de períodos financeiros      | tool MCP `recompute_financial_periods` (sem tela)                   | —                                   |
| Padrões documentais aplicados ao parser  | não fecha                                                           | —                                   |
