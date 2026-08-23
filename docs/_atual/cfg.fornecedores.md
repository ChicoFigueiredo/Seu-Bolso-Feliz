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

> O `.env.example` listava 8 variáveis enquanto o código lê **17** — faltavam
> `LOCAL_USER_ID` e `SUPABASE_URL` (sem as quais nenhum worker sobe), as três do Gmail,
> as três da OpenAI e as de OCR. **Já foi corrigido**; as tabelas abaixo detalham cada
> uma e onde é lida.

### 1.1 As chaves de API — use as novas

O Supabase tem dois modelos de chave convivendo. **Este projeto usa o novo.**

| Modelo     | Formato              | Substitui      | Rotação                        |
| ---------- | -------------------- | -------------- | ------------------------------ |
| Publicável | `sb_publishable_...` | `anon`         | independente, sem downtime     |
| Secreta    | `sb_secret_...`      | `service_role` | independente, uma por serviço  |
| Legacy     | `eyJ...` (JWT)       | —              | **impossível** — vence em 2026 |

**Se um valor começa com `eyJ`, é chave legacy e está errado.** Elas derivam do JWT
secret do projeto, e é por isso que não podem ser rotacionadas: mexer numa mexeria em
tudo que o secret assina. Não existe botão "Rotate" para elas — o caminho é criar as
novas e desativar as antigas, em **Settings → API Keys**.

As chaves secretas novas ainda ganham uma proteção que a `service_role` não tinha:
respondem **HTTP 401 se usadas a partir de um navegador**, detectado pelo `User-Agent`.

#### Onde cada uma vive

| Variável                               | Onde é lida                   | Formato              |
| -------------------------------------- | ----------------------------- | -------------------- |
| `NEXT_PUBLIC_SUPABASE_URL`             | `apps/web/src/lib/supabase/*` | URL do projeto       |
| `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | idem                          | `sb_publishable_...` |
| `SUPABASE_SECRET_KEY`                  | workers, MCP, scripts         | `sb_secret_...`      |
| `SUPABASE_URL`                         | workers, MCP                  | mesma URL            |
| `SUPABASE_DB_PASSWORD`                 | CLI                           | senha do Postgres    |
| `SUPABASE_ACCESS_TOKEN`                | CLI e CI                      | token da conta       |
| `SUPABASE_PROJECT_ID`                  | CLI e CI                      | ref do projeto       |

> `SUPABASE_URL` e `NEXT_PUBLIC_SUPABASE_URL` têm o **mesmo valor** e ambas são
> necessárias: o Next só expõe ao cliente variáveis com o prefixo `NEXT_PUBLIC_`,
> e os workers não rodam dentro do Next.

> ⚠️ `SUPABASE_SECRET_KEY` **nunca** vai para o Vercel nem para o GitHub. Ela mora só na
> sua máquina, onde rodam os workers e o MCP. O GitHub guarda `SUPABASE_ACCESS_TOKEN`,
> que é outra coisa: credencial da sua conta, usada pelo CLI para fazer deploy.

#### Ambiente local

O Supabase local tem chaves novas fixas, **embutidas no binário do CLI e iguais em
qualquer máquina** — não são segredo e já estão no `.env.example`:

```bash
supabase status   # mostra Publishable e Secret do stack local
```

#### As chaves nas Edge Functions

Aqui os nomes são **diferentes**, e é onde a migração costuma quebrar. A plataforma
injeta as chaves novas no **plural**, e o valor é um JSON indexado por nome:

```
SUPABASE_SECRET_KEYS      = {"default":"sb_secret_..."}
SUPABASE_PUBLISHABLE_KEYS = {"default":"sb_publishable_..."}
```

Não adianta contornar criando uma variável no singular: **o CLI recusa qualquer secret
com prefixo `SUPABASE_`** (`Env name cannot start with SUPABASE_, skipping`). Ou seja,
uma função que leia `SUPABASE_SECRET_KEY` sobe com a chave `undefined` — foi exatamente
o defeito encontrado nas quatro funções deste projeto, corrigido em
`supabase/functions/_shared/keys.ts`:

```ts
import { getPublishableKey, getSecretKey } from "../_shared/keys.ts";

const admin = createClient(Deno.env.get("SUPABASE_URL")!, getSecretKey());
```

> ⚠️ **A armadilha do `Invalid JWT`:** as chaves novas não são JWT. Em
> `Authorization: Bearer` a plataforma tenta lê-las como token e rejeita — elas vão no
> cabeçalho `apikey`, que é o que o `supabase-js` já faz sozinho. Como o `verify_jwt`
> embutido só entende chave legacy, cada função declara `verify_jwt = false` em
> `supabase/config.toml` e autoriza no próprio código, com `auth.getUser()`.

#### Se você chamar o banco a partir do Postgres

`pg_net` e Database Webhooks costumam mandar a chave em `Authorization: Bearer`. Com
chave nova isso falha. Use `apikey`, e leia do Vault em vez de escrever o valor no SQL:

```sql
headers := jsonb_build_object(
  'Content-Type', 'application/json',
  'apikey', (select decrypted_secret from vault.decrypted_secrets where name = 'secret_key')
);
```

Hoje o projeto **não usa** `pg_net` nem Database Webhooks — fica registrado para quando
usar.

### 1.2 O que alterar e o que remover nos `.env`

Nenhum **nome** de variável muda. O que muda são valores — e um punhado de coisas
precisa sumir.

**Alterar** (só o valor, para o formato novo):

| Arquivo           | Variável                               | Para                 |
| ----------------- | -------------------------------------- | -------------------- |
| `.env.production` | `NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY` | `sb_publishable_...` |
| `.env.production` | `SUPABASE_SECRET_KEY`                  | `sb_secret_...`      |
| `.env` (cofre)    | `PRD_PUBLISHABLE_KEY`                  | `sb_publishable_...` |
| `.env` (cofre)    | `PRD_SECRET_KEY`                       | `sb_secret_...`      |

**Remover:**

| O quê                                                                                                                                                        | Por quê                                                     |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| `STAGING_POSTGRES_PASSWORD`, `STAGING_PROJECT_NAME`, `STAGING_PROJECT_URL`, `STAGING_PUBLISHABLE_KEY`, `STAGING_SECRET_KEY`, `STAGING_SUPABASE_ID` no `.env` | Staging foi eliminado; as chaves continuam abrindo um banco |
| O arquivo `.env.staging`                                                                                                                                     | Idem                                                        |
| `.env.local.bak-*`                                                                                                                                           | Backup da migração; contém as chaves locais antigas         |

E, fora do repositório: o projeto Supabase `seu-bolso-feliz-staging`. Enquanto ele
existir, as chaves dele valem — remover a variável não desativa nada.

**Já migrado, não precisa mexer:** `.env.local` (aponta para `127.0.0.1:54321`, chaves
públicas do CLI), os fallbacks nos testes e o `.vscode/mcp.json`.

### Identidade local — obrigatória para workers e MCP

| Variável        | Onde é lida                                                    | Valor                               |
| --------------- | -------------------------------------------------------------- | ----------------------------------- |
| `LOCAL_USER_ID` | `workers/*/src/supabase.ts`, `apps/mcp-server/src/supabase.ts` | UUID do seu usuário em `auth.users` |

Os workers rodam fora do navegador, sem sessão de autenticação, então precisam saber
de quem são os dados. Para descobrir o seu:

```sql
select id, email from auth.users;
```

Ou, com o Supabase local:

```bash
docker exec -i supabase_db_<projeto> psql -U postgres -d postgres \
  -tAc "select id, email from auth.users;"
```

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

```bash
supabase start          # sobe o Supabase local
bun run db:migrate      # aplica as migrations
bun run generate-types  # regenera packages/shared-types
```

Crie o usuário pela tela de login (`/login`) ou via API admin, e pegue o `id` para
`LOCAL_USER_ID`.

---

## 3. Ciclo financeiro

Tela: **`/dashboard/settings`**. Tabela: `user_financial_preferences`.

| Campo                         | Significado                                                                |
| ----------------------------- | -------------------------------------------------------------------------- |
| `financial_cycle_start_day`   | Dia em que o seu mês financeiro começa (1–31). Se você recebe dia 5, use 5 |
| `financial_cycle_anchor_date` | Data-âncora para gerar os períodos                                         |
| `default_currency`            | `BRL`                                                                      |

Depois de definir, gere os períodos:

```sql
select generate_financial_periods(
  p_start_day    => 5,
  p_months_ahead => 12,
  p_anchor_date  => current_date
);
```

> ⚠️ **Duas ressalvas verificadas:**
>
> 1. A tela de settings **não chama** `generate_financial_periods` ao salvar. Rode o SQL
>    à mão depois de mudar o dia.
> 2. A tool MCP `recompute_financial_periods` **não funciona**: os nomes de parâmetro
>    divergem da RPC e `auth.uid()` é NULL sob `service_role`. Está registrada como
>    defeito conhecido.

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

### 4.3 Cartões — ⚠️ **não há tela**

A tabela `cards` existe e já tem todas as colunas necessárias, mas **a rota
`/dashboard/cards` não existe**. Não é possível cadastrar um cartão pela interface.

Enquanto a tela não existir (issue #4 no GitHub), cadastre por SQL:

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

Pela server action (`apps/web/src/app/actions/secrets.ts`), ou por SQL **autenticado
como o usuário** — `fn_set_secret` usa `auth.uid()` de propósito e não aceita um
parâmetro de usuário, o que elimina a possibilidade de forjar:

```sql
select fn_set_secret(
  p_secret_type => 'pdf_password',
  p_plaintext   => 'senha-do-pdf',
  p_entity_type => 'supplier',              -- ou null para senha global
  p_entity_id   => '<supplier_id>',
  p_label       => 'Fatura Cartão X'
);
```

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
ordem de uso recente, com teto de 20, e marca a que funcionou para subir na ordem da
próxima vez.

> ⚠️ **Ainda não há tela** para gerenciar senhas. A action existe; a interface é a
> issue de F6.2.

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
# 0. Nenhuma chave legacy sobrou (não deve imprimir nada)
grep -l "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9" .env* 2>/dev/null

# 1. Ambiente e banco
supabase start && bun run db:migrate

# 2. Suíte completa
bun run test        # esperado: 505/505

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

| O quê                                    | Alternativa hoje    | Rastreado |
| ---------------------------------------- | ------------------- | --------- |
| Cartões (fechamento, vencimento, limite) | SQL                 | issue #4  |
| Senhas de PDF                            | server action / SQL | F6.2      |
| Templates de extração                    | código              | —         |
| Regeneração de períodos financeiros      | SQL                 | —         |
| Padrões documentais aplicados ao parser  | não fecha           | —         |
