# Checklist geral — estado real, o que falta, e quem faz

> **Medido em 2026-08-05**, com o stack local de verdade no ar e a suíte inteira
> reexecutada a partir de um `supabase db reset` limpo. Nada aqui foi copiado de
> documento anterior sem reexecução — onde não deu para medir, está escrito
> "não verificado".
>
> ⚠️ **Atualizado em 2026-08-30** só nos trechos sobre a arquitetura de backend: o
> Supabase saiu inteiramente do projeto (ADR-009) — banco Neon/Drizzle, storage Vercel
> Blob, autenticação própria, fila em `ingestion_jobs`. As seções que descreviam
> chaves de API, RLS/`GRANT`, `supabase db reset`/kong e Edge Functions foram
> corrigidas para o mecanismo real de hoje; o resto (números de teste, histórico de
> commits) é a medição original de 2026-08-05 e **não foi reconfirmado** — trate como
> desatualizado até rodar de novo.
>
> **Toda a fila ⚙️ foi executada** — o §4 é o registro do que cada item mudou.
> O que restou é 🔒: está no §5, e o resumo na ordem em que eu faria está no §10.
>
> Legenda: ✅ feito e verificado · 🟡 parcial · ⬜ não existe · 🔒 **só você pode fazer**
> · ⚙️ **eu executo** (é só mandar) · 🚨 bloqueador · ⚠️ verificar
>
> Complementos: [`cfg.fornecedores.md`](cfg.fornecedores.md) (detalhe de cada parâmetro)
> · [`../specs/00-estado-real.md`](../specs/00-estado-real.md) (mapa por capacidade)
>
> ⚠️ [`_checklist.imediato.md`](_checklist.imediato.md) é de 2026-07-27 (também
> atualizado em 2026-08-30 só na arquitetura) e **está desatualizado** no restante: era
> sobre migrar chaves do Supabase, "cartões só por SQL" e "só PDF tem parser" — tudo
> isso já mudou. O passo-a-passo de deploy que ele traz hoje é o mesmo deste documento,
> só que para Neon/Vercel — na dúvida entre os dois, **este documento vence**.

---

## 0. As duas perguntas diretas

### OpenRouter está funcional? **Agora sim — implementado nesta sessão.**

Quando você perguntou, a resposta era **não existia**: zero ocorrências de
`openrouter` no repositório inteiro. Agora existe, nos quatro pontos de chamada,
e a troca é por variável de ambiente:

```bash
OPENROUTER_API_KEY=sk-or-...     # define isto e o resto se ajusta sozinho
AI_CHAT_MODEL=openai/gpt-4o      # no OpenRouter os modelos levam prefixo
AI_SUGGEST_MODEL=openai/gpt-4o-mini
OPENAI_LITE_MODEL=openai/gpt-4o-mini
OPENAI_FULL_MODEL=openai/gpt-4o
```

| Onde                              | Antes                           | Agora                                 |
| --------------------------------- | ------------------------------- | ------------------------------------- |
| `api/chat/route.ts`               | `openai("gpt-4o")` literal      | `modeloDeConversa()` — modelo por env |
| `api/ai-suggest/route.ts`         | `openai("gpt-4o-mini")` literal | `modeloDeSugestao()` — modelo por env |
| `ai-lite-enricher.ts`             | `fetch` para `api.openai.com`   | `chamarChatCompletions()`             |
| `ai-full-enricher.ts` (2 lugares) | `fetch` para `api.openai.com`   | `chamarChatCompletions()`             |

Regras, com teste para cada uma (`__tests__/domain/llm-endpoint.test.ts`, 7 testes):

- `OPENROUTER_API_KEY` tem **precedência** sobre `OPENAI_API_KEY` — definir a
  primeira é escolha explícita; a segunda costuma ficar no `.env` por inércia, e
  ser ignorada silenciosamente cobraria na conta errada;
- os cabeçalhos `HTTP-Referer` e `X-Title` só vão no OpenRouter, que sem eles
  aplica um teto de requisições menor — detalhe que só apareceria em produção;
- `OPENAI_BASE_URL` sobrepõe tudo: é a porta para Ollama, LiteLLM ou qualquer
  API compatível.

Sem nenhuma dessas variáveis novas, **nada muda**: continua OpenAI direto.

### O worker está funcional? **Sim — e um defeito sério foi encontrado no caminho.**

Rodei o pipeline de ponta a ponta contra o Supabase local, com um PDF real:

```
Parsing v1 concluído: local_regex, confiança 90%
Evidência anexada a obrigação existente por supplier_period_amount
Classificado: transaction, consumption_metric (confiança: 90%)
Gerados 2 drafts → aguardando revisão
```

Mas o E2E falhava de forma reprodutível na **primeira execução após cada
`supabase db reset`**. A investigação achou algo pior que um teste frágil: o
`pdf-parse@1.1.1` embute uma build do `pdf.js` de 2018 que, **no mesmo processo
e com os mesmos bytes**, devolveu erros DIFERENTES em duas tentativas seguidas:

```
tentativa 1 → "Command token too long: 128"
tentativa 2 → "bad XRef entry"
```

Não é corrupção de dados, e isso foi medido: os bytes baixados do Storage foram
comparados com os originais no mesmo processo — mesmo tamanho, mesmo cabeçalho,
`Buffer.compare` = 0 — e a extração falhou mesmo assim. Um estresse de 200
extrações seguidas do mesmo PDF passou 200/200 com a máquina ociosa.

O padrão é o que importa: **falhava quando a máquina estava ocupada**, com os
containers reiniciando. Um parser sensível a pressão de event loop não é
problema de teste — sob um worker processando lote, ele reprova documento de
verdade, em silêncio, e o documento fica `failed` esperando reprocessamento
manual.

**Substituído por `pdfjs-dist`** (que já estava no repositório, via `react-pdf`).
Três `db reset` seguidos, E2E verde em todos — antes era 100% de falha.

A troca exigiu reconstruir linhas a partir das coordenadas do `getTextContent`,
porque os parsers determinísticos casam expressões contra LINHAS
(`Vencimento: 15/04/2026`) e concatenar fragmentos com espaço destruiria isso.

**Ressalva honesta:** o worker continua sem ter visto um documento seu de
verdade. Todos os fixtures são sintéticos.

## 1. ✅ O bloqueador que estava invisível — `GRANT` faltando nas migrations (histórico, era Supabase)

> ⚠️ **Esta seção é história do Supabase, não checklist de hoje.** Descreve um
> incidente com os papéis `service_role`/`authenticated` e RLS — backend que saiu do
> projeto na ADR-009 (2026-08-30). No Neon não existe esse par de papéis nem RLS
> (`row_security = off`, ver `packages/db/schema.sql`): o isolamento é
> `WHERE user_id = ownerId` explícito em cada query. Esta classe de bug não pode se
> repetir no desenho atual, e a migration `supabase/migrations/20260804220000_grant_api_roles.sql`
> citada abaixo não existe no diretório de migrations atual (`packages/db/migrations/`).
> Mantido como registro de auditoria.

**Resolvido.** Era o achado mais importante da sessão anterior e a primeira coisa
executada nesta.

Depois de um `supabase db reset` limpo, **nenhuma** das 47 tabelas e views de
`public` era acessível pelos papéis da API:

- `permission denied for table ingestion_runs` — o worker não escrevia nada;
- `permission denied for function encrypt_secret` — segredos não funcionavam;
- a web, que usa a chave publicável + papel `authenticated`, não lia nada;
- integração: **29 de 60 falhando**.

**Causa.** As migrations criavam objetos como papel `postgres` e dependiam de um
`ALTER DEFAULT PRIVILEGES` implícito do Supabase que existe **só para
`supabase_admin`** — não há entrada para `postgres`. O privilégio nunca foi
concedido; foi presumido. Não era problema das chaves novas: a chave legacy
`service_role` falhava exatamente igual à `sb_secret_...`.

**Correção:** `supabase/migrations/20260804220000_grant_api_roles.sql`, com
`GRANT` para os objetos existentes **e** `ALTER DEFAULT PRIVILEGES FOR ROLE
postgres` para que a próxima migration não reintroduza o bug em silêncio.

| Depois                                      | Antes |
| ------------------------------------------- | ----- |
| 47/47 tabelas acessíveis por `service_role` | 0/47  |
| integração **66/66**                        | 31/60 |
| E2E **12/12** em reset frio                 | 10/12 |

> **RLS continua protegendo.** `GRANT` dá acesso à tabela; as políticas de RLS
> filtram linha por linha. Sem o `GRANT`, o RLS nem chegava a ser avaliado.

~~🔒 Falta uma verificação sua, em produção, logo depois do primeiro `db push`~~ — não
se aplica mais: sem `service_role`/`authenticated` nem RLS, não há "grant" separado
para conferir. `packages/db/schema.sql` + as migrations rodam com o único papel que o
Neon usa (o dono da `DATABASE_URL`), que já nasce com privilégio sobre tudo que criou.

### Efeito colateral do diagnóstico: o kong local (histórico, não existe mais)

Vale saber, porque custou meia hora quando pegou de surpresa: `supabase db reset`
reiniciava os containers, e o **kong guardava o IP antigo do serviço de auth**. Todo
`/auth/v1/*` passava a devolver **502** — inclusive `createUser` nos testes — sem
nenhuma pista do motivo. A saída era reiniciar o kong:

```bash
docker restart supabase_kong_seu.bolso.feliz
```

Não existe mais Supabase local, kong, nem esse modo de falha. Hoje um teste de
integração fala com um Postgres real (Neon ou um container `postgres:17-alpine`
descartável — receita em `__tests__/helpers/db.ts`), sem proxy de API na frente.

## 2. Saúde do código — medido a frio, não afirmado

Tudo abaixo foi reexecutado depois de `supabase db reset` + reinício do kong,
que é o cenário mais próximo do que o CI vai encontrar. Os números são da medição
de 2026-08-05 e continuam valendo: nenhum arquivo de código mudou desde então —
só este documento.

| Sinal                 | Resultado                        | Comando                         |
| --------------------- | -------------------------------- | ------------------------------- |
| TypeScript            | ✅ **zero erros**                | `bun run typecheck`             |
| Lint                  | ✅ limpo                         | `bun run lint`                  |
| Formatação            | ✅ limpo                         | `bun run format:check`          |
| Testes unitários      | ✅ **499/499** (30 arquivos)     | `bun run test:unit`             |
| Testes de integração  | ✅ **66/66** (7 arquivos)        | `bun run test:integration`      |
| Testes E2E            | ✅ **12/12** (2 arquivos)        | `bunx vitest run --project e2e` |
| Build web             | ✅ compila, rotas novas inclusas | `bun run build`                 |
| Worker, execução real | ✅ 1 documento → 2 drafts        | `bun run cli --local --process` |

**Total: 577 testes verdes** — eram 505 no início da sessão.

## 3. Tudo que já foi feito (histórico consolidado)

### 3.1 Correções estruturais que fecharam o ciclo do documento

| O quê                                                                       | Commit    |
| --------------------------------------------------------------------------- | --------- |
| `@sbf/contracts` — contrato de draft unificado gerador ↔ materializador     | `fb7c698` |
| Chave de identidade financeira unificada                                    | `0e32a66` |
| Materialização atômica e idempotente via RPC `fn_materialize_draft_record`  | `9e3bd42` |
| Aprovação na UI passa a lançar de verdade — ciclo fechado                   | `340e735` |
| `financial_obligations` conectada ao pipeline (antes: zero código a tocava) | `2861dc2` |
| Criptografia real de segredos (`private.crypto_keys`)                       | `2497684` |
| CLI único em processo, substituindo o orquestrador de `spawnSync`           | `13e2b7c` |
| Fixtures reais + E2E que fecha o ciclo do documento                         | `3d2549c` |
| Migração para GitHub Actions, ambiente único de produção                    | `290427f` |
| 5 erros de typecheck corrigidos (2 eram bugs reais de coluna inexistente)   | `ab07bf4` |
| Adoção das chaves de API publishable/secret                                 | `12f20ce` |

> **Por que o typecheck importava:** três dos cinco erros eram consultas a colunas que
> não existem (`transactions.category`, `ingestion_jobs.step`). Como `next build` roda
> type check, **o deploy no Vercel teria falhado** — e as tools de IA afetadas estavam
> quebradas em runtime também.

### 3.1.1 Nesta sessão (2026-08-05, ainda não commitado)

| O quê                                                           | Onde                                               |
| --------------------------------------------------------------- | -------------------------------------------------- |
| `GRANT` para os papéis da API — o bloqueador de tudo            | `20260804220000_grant_api_roles.sql`               |
| `pdf-parse@1.1.1` → `pdfjs-dist`, com reconstrução de linhas    | `workers/ingestion/src/parsers/pdf-text.ts`        |
| Parsers de CSV, OFX e XLSX                                      | `workers/ingestion/src/parsers/tabular.ts`         |
| OCR de imagem via tesseract, atrás de flag                      | `workers/ingestion/src/parsers/text-extractor.ts`  |
| Registro único de formatos + teste de equivalência              | `packages/contracts/src/formats.ts`                |
| MIME types do bucket alinhados ao registro                      | `20260804230000_bucket_mime_types_do_registro.sql` |
| Provedor de LLM por env (OpenAI ou OpenRouter)                  | `lib/ai/provider.ts` · `parsers/llm-endpoint.ts`   |
| Ciclo financeiro e dedup ADR-001 nos relatórios                 | `reports/report-period.ts` · `reports/page.tsx`    |
| CRUD e telas de cartão                                          | `actions/cards.ts` · `dashboard/cards/`            |
| Tela de senhas de documentos                                    | `dashboard/settings/passwords/`                    |
| Limpeza de staging (`.env.staging`, `.env.local.bak-*`, 6 vars) | raiz                                               |

### 3.2 Migração das chaves de API do Supabase — código 100% pronto (histórico, superado)

> ⚠️ Todo este trabalho foi **superado** pela saída completa do Supabase (ADR-009,
> 2026-08-30): não sobrou Edge Function, `verify_jwt`, nem chave `sb_secret_...`/
> `sb_publishable_...` para migrar — o projeto não fala mais com Supabase em lugar
> nenhum. Mantido como registro do que foi feito nesta fase intermediária.

| Feito (nesta fase intermediária, hoje superado)                | Onde (na época)                      |
| -------------------------------------------------------------- | ------------------------------------ |
| Edge Functions leem `SUPABASE_SECRET_KEYS` / `..._KEYS` (JSON) | `supabase/functions/_shared/keys.ts` |
| `verify_jwt = false` declarado por função                      | `supabase/config.toml`               |
| `--no-verify-jwt` global removido do deploy                    | `.github/workflows/deploy.yml`       |
| `.env.local` e fixtures de teste em `sb_secret_...`            | `.env.local`, `__tests__/`           |
| `NEXT_PUBLIC_SUPABASE_ANON_KEY` órfã removida do CI            | `.github/workflows/ci.yml`           |
| `.env.example` / `.env.production.example` documentados        | raiz                                 |

**Verificado na época:** a chave nova `sb_secret_...` autenticava corretamente contra o
stack local. O que falhava era privilégio (§1), não formato de chave. Hoje nenhuma
dessas linhas tem chamador: `.env.example` já é só Neon/Vercel Blob/auth própria.

### 3.3 CI/CD escrito

> ⚠️ A coluna "Conteúdo" abaixo descrevia os workflows **na época** (Supabase local no
> CI, `db push` + edge functions no deploy). Reescritos desde então para Neon/Vercel —
> conteúdo atual: `ci.yml` sobe `postgres:17-alpine` como serviço, aplica
> `packages/db/schema.sql` + migrations do zero e roda lint/format/typecheck/
> commitlint/testes/build, sem Supabase em lugar nenhum; `deploy.yml` faz
> `pg_dump` → `scripts/aplicar-migrations.sh` → `vercel pull/build/deploy --prod`, com
> portão humano (`environment: production`) e sem passo de Edge Functions.

| Arquivo                        | Conteúdo (na época)                                                                         | Já rodou?    |
| ------------------------------ | ------------------------------------------------------------------------------------------- | ------------ |
| `.github/workflows/ci.yml`     | lint · format · typecheck · commitlint · unit + integração + E2E com Supabase local · build | ⬜ **nunca** |
| `.github/workflows/deploy.yml` | backup → `db push` → edge functions → `vercel pull/build/deploy`, com portão humano         | ⬜ **nunca** |

O deploy web é **real** agora (`bunx vercel deploy --prebuilt --prod`), não o
`echo "configurar provedor"` que existia no GitLab CI.

### 3.4 Capacidades entregues

| Capacidade                                                   | Estado | Nota                                                                    |
| ------------------------------------------------------------ | ------ | ----------------------------------------------------------------------- |
| Pipeline de ingestão (13 status)                             | ✅     | provado por execução hoje                                               |
| Parsers determinísticos (boleto, CEMIG, templates, consenso) | ✅     | `boleto-parser` ainda básico                                            |
| Enriquecimento IA lite/full                                  | ✅     | OpenAI **ou OpenRouter**, por variável de ambiente                      |
| Convergência para obrigação canônica                         | ✅     | dedup cross-canal provada em E2E                                        |
| Materialização atômica + idempotência                        | ✅     | 13 testes de integração, incluindo `Promise.all` concorrente            |
| UI de ingestão e revisão                                     | ✅     | aprovar/lançar/rejeitar; falta split-view                               |
| Chat IA + sugestões inline                                   | ✅     | `approve_draft` e `post_draft` separadas — IA não cria dinheiro sozinha |
| Criptografia de segredos                                     | ✅     | 8 testes; senha nunca em log                                            |
| MCP server                                                   | ✅     | 10 tools                                                                |
| Scanner Gmail e pasta local                                  | ✅     | Gmail precisa de refresh token                                          |
| Telas CRUD (transações, fornecedores, produtos, …)           | ✅     | funcionam                                                               |
| **Cartões** — lista, criação e edição                        | ✅     | `/dashboard/cards` (#4, #5) — antes, só por SQL                         |
| **Senhas de documentos protegidos**                          | ✅     | `/dashboard/settings/passwords` — a action existia sem tela             |
| **Formatos: PDF, CSV, XLSX, OFX, XML**                       | ✅     | registro único; imagens com OCR opcional                                |
| **Relatórios sem contagem em dobro**                         | ✅     | `v_expenses_deduplicated` + ciclo do `@sbf/domain` (#17, #18)           |

---

## 4. A fila ⚙️ — executada

Os treze itens da lista anterior, mais dois que apareceram durante a execução.

| #    | Tarefa                                                                                             | Estado | Evidência                                                                                      |
| ---- | -------------------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------- |
| ⚙️1  | Migration de `GRANT` para os papéis da API (histórico — Supabase; arquivo não existe mais, ver §1) | ✅     | era `supabase/migrations/20260804220000_grant_api_roles.sql` · 47/47 tabelas, integração 66/66 |
| ⚙️2  | `db reset` + suíte inteira provando a migration                                                    | ✅     | três resets frios seguidos, tudo verde                                                         |
| ⚙️3  | Apagar `.env.staging` e `.env.local.bak-*`                                                         | ✅     | removidos do disco                                                                             |
| ⚙️4  | Remover as 6 variáveis `STAGING_*` do `.env`                                                       | ✅     | bloco substituído por uma nota do porquê                                                       |
| ⚙️5  | Atualizar `docs/specs/00-estado-real.md`                                                           | ✅     | seções de infra e formatos reescritas, números remedidos, gaps reordenados                     |
| ⚙️6  | `inbox/` no `.gitignore`                                                                           | ✅     | são seus documentos, não artefato de código                                                    |
| ⚙️7  | **OpenRouter** — provedor por env, web e workers                                                   | ✅     | `lib/ai/provider.ts` · `parsers/llm-endpoint.ts` · 7 testes                                    |
| ⚙️8  | `supported-formats` como fonte única (#14)                                                         | ✅     | `@sbf/contracts/formats` · 19 testes, incluindo equivalência contra os arquivos                |
| ⚙️9  | Ciclo financeiro e dedup ADR-001 nos relatórios (#17, #18)                                         | ✅     | `report-period.ts` (7 testes) · `reports.test.ts` (6 testes de integração)                     |
| ⚙️10 | `actions/cards.ts` + `/dashboard/cards` (#4, #5)                                                   | ✅     | CRUD, lista, formulário, edição — e link na sidebar                                            |
| ⚙️11 | Tela de senhas de documentos protegidos                                                            | ✅     | `/dashboard/settings/passwords`                                                                |
| ⚙️12 | Parsers de XLSX / OFX / CSV (#13)                                                                  | ✅     | `parsers/tabular.ts` · 24 testes                                                               |
| ⚙️13 | OCR de imagem (#15)                                                                                | ✅     | tesseract, atrás de `INGESTION_ENABLE_IMAGE_OCR`                                               |
| ➕   | Substituir `pdf-parse@1.1.1` por `pdfjs-dist`                                                      | ✅     | `parsers/pdf-text.ts` · 8 testes — ver §0; era defeito real, não teste frágil                  |
| ➕   | Delimitador de CSV que reconhece "não é tabela"                                                    | ✅     | apareceu ao integrar o ⚙️12; ver abaixo                                                        |

### O que cada um mudou, em detalhe

**⚙️7 — OpenRouter.** Detalhado no §0.

**⚙️8 — registro único de formatos.** Havia **cinco** listas independentes:
três `accept` na interface, a allowlist do scanner local, a do scanner de Gmail
— mais os `allowed_mime_types` do bucket. Divergiam entre si e do código: a
interface anunciava `.doc`, `.docx` e `.qif`, que nenhum scanner aceitava e
nenhum parser lia; os scanners aceitavam `.xls`, que a interface não oferecia e
o pipeline não abria.

O efeito era o pior possível: o arquivo era aceito no upload, subia, virava
documento e parava em revisão sem campo nenhum — sem erro, sem aviso.

Agora há um registro só (`packages/contracts/src/formats.ts`) com a regra **um
formato só entra quando existe código que o lê**. Saíram `.xls` (o `exceljs` lê
XLSX e não lê XLS), `.doc`, `.docx` e `.qif`. O teste de equivalência lê os
arquivos reais — os três componentes de upload, os dois scanners e a migration
do bucket — e falha se alguém reintroduzir uma lista literal ao lado do import.

**⚙️9 — relatórios.** Duas correções independentes:

- o cálculo do ciclo era reimplementado inline e **errava em fevereiro**: fazia
  `new Date(ano, mes, 31)` e transbordava para março, devolvendo um período que
  começava depois de terminar — um relatório vazio que pareceria "não tenho
  gastos neste ciclo". Agora chama `getCurrentPeriod` do `@sbf/domain`;
- as despesas somavam `transactions` cru. Numa vida financeira normal quase tudo
  passa pelo cartão, e cada compra aparecia **duas** vezes — como item da fatura
  e como transação — enquanto o pagamento da fatura entrava como uma terceira.
  O relatório inflava na proporção do que fosse pago no cartão. A view
  `v_expenses_deduplicated` já existia e implementava a regra certa; ninguém a
  chamava. O último teste de `reports.test.ts` mede a divergência: **270 no cru
  contra 350 na view**, no mesmo conjunto de dados.

**⚙️10 — cartões.** `cards` era a única entidade central sem porta de entrada:
sem action, sem tela, só `INSERT` manual no SQL. Como `statement_cycles.card_id`
é `NOT NULL`, sem cartão nenhuma fatura podia existir — o Marco 2 parava na
primeira porta. Agora há `/dashboard/cards` com limite, fechamento e vencimento,
formulário de criação e edição, e link na sidebar. `deleteCard` recusa excluir
cartão com fatura e sugere desativar, para não apagar o histórico.

> **Efeito colateral que vale registrar:** a tela escreve em `cards.credit_limit`,
> e `financial_products.credit_limit` também existe, sem regra de precedência
> entre os dois. O problema é anterior a esta sessão (issue #6), mas antes ele
> estava escondido — não havia interface que escrevesse em nenhum dos dois.
> Agora há, então a ambiguidade passa a produzir dado divergente de verdade.
> Resolver é o #6: uma view `v_card_limits` com a regra explícita.

**⚙️11 — senhas.** A action `setSecret` existia e ninguém a chamava, porque não
havia tela. Quem recebe fatura protegida — a maioria dos bancos — via o
documento parar com "PDF protegido" e não tinha onde informar a senha. A tela
está em `/dashboard/settings/passwords`, com escopo global ou por fornecedor. O
valor em claro só trafega no envio; a listagem devolve rótulo, escopo e uso.

**⚙️12 — formatos tabulares.** XLSX caía num `Buffer.toString()` — e XLSX é um
ZIP, então o "texto extraído" era lixo binário que atravessava o pipeline sem
erro e virava um `extraction_result` vazio. CSV era lido como texto puro, sem
delimitador nem codificação, o que quebrava qualquer extrato brasileiro (`;` como
separador, latin-1 nos acentos). OFX não tinha parser nenhum.

Os três agora produzem **linhas `Campo: valor`**, o mesmo formato que sai de um
PDF — assim um extrato atravessa o pipeline pelo caminho já existente, sem
código novo a jusante.

**➕ delimitador de CSV.** Ao integrar o ⚙️12, um teste de ingestão que passava
começou a falhar: `expected 245 to be 245.5`. A causa é instrutiva — uma conta de
luz exportada em `.csv` é **texto corrido**, não tabela. A detecção elegia a
vírgula como delimitador e quebrava `Valor a Pagar R$ 245,50` em dois campos: o
valor virava R$ 245,00. Cinquenta centavos que ninguém percebe olhando o
relatório. `detectarDelimitador` agora devolve `null` quando nenhum candidato
produz o mesmo número de campos (≥2) em todas as linhas, e o extrator devolve o
texto cru nesse caso.

**⚙️13 — OCR de imagem.** Antes, toda imagem devolvia texto **vazio** e sem
erro: o documento atravessava o pipeline, gerava um `extraction_result` sem
campo algum e parava em revisão sem explicação. Fotografar um boleto com o
celular é o caminho mais natural para quem usa o sistema pelo telefone, e era
exatamente o que não funcionava. Agora há OCR via tesseract, desligado por
padrão como o de PDF — depende de binário externo:

```bash
sudo apt install tesseract-ocr tesseract-ocr-por
# e no .env:
INGESTION_ENABLE_IMAGE_OCR=true
```

### Os testes que entraram

| Arquivo                                    | Testes | O que tranca                                      |
| ------------------------------------------ | -----: | ------------------------------------------------- |
| `packages/contracts/src/formats.test.ts`   |     19 | equivalência do registro contra os arquivos reais |
| `__tests__/domain/tabular-parsers.test.ts` |     24 | CSV, OFX e XLSX, incluindo latin-1 e BOM          |
| `__tests__/domain/pdf-text.test.ts`        |      8 | determinismo do PDF e reconstrução de linhas      |
| `__tests__/domain/llm-endpoint.test.ts`    |      7 | precedência e cabeçalhos do provedor de IA        |
| `__tests__/domain/report-period.test.ts`   |      7 | ciclo financeiro, com o caso de fevereiro         |
| `__tests__/integration/reports.test.ts`    |      6 | dedup ADR-001 com fatura, itens e pagamento       |
| `__tests__/domain/parsers.test.ts`         |     +1 | novo contrato do CSV (converte, ou devolve o cru) |

71 testes novos, mais um saldo de +1 em `parsers.test.ts` — o teste antigo de CSV
virou dois, porque o contrato passou a ter dois caminhos: converter a tabela ou
devolver o texto cru. **+72 no total**, de 505 para 577.

---

## 5. O que falta — 🔒 **só você pode fazer**

### 5.1 ⚠️ Verificar: o projeto Supabase antigo ainda existe?

As chaves de produção vazaram no commit `38b8126`, em **repositório público**. Isto
era 🚨 quando a aplicação ainda dependia delas — hoje ela não fala mais com o Supabase
em lugar nenhum (verificado: zero credencial Supabase em código vivo), mas isso **não
desliga** o projeto Supabase em si. Enquanto ele existir com aquelas chaves ativas, o
passivo de segurança continua real, só que fora do raio do código deste repositório.

- [ ] **⚠️ Verificar** se o projeto Supabase de produção e o de staging
      (`seu-bolso-feliz-staging`) ainda existem
- [ ] Se existirem: apagar o projeto, ou girar/desativar as chaves lá (Settings → API
      Keys) e decidir se o repositório continua **público**

Não há mais ação de código associada a este item — é inteiramente do lado do painel do
Supabase (ou da decisão de apagar o projeto de vez).

### 5.2 🔒 Neon de produção

- [ ] Anotar as duas connection strings do projeto/branch de produção: **pooled**
      (`DATABASE_URL`) e **direta** (`DATABASE_URL_UNPOOLED`)
- [ ] Aplicar schema + migrations (substitui `supabase db push`):

  ```bash
  psql "$DATABASE_URL_UNPOOLED" -f packages/db/schema.sql   # só se o banco for novo
  bash scripts/aplicar-migrations.sh
  ```

- [ ] **Google Cloud Console**: credencial OAuth 2.0, com
      `https://seudominio.com.br/auth/callback` na lista de URIs de redirecionamento
      autorizadas. Não existe mais "Auth → URL Configuration" do Supabase — a
      autenticação é própria (`apps/web/src/lib/auth/`). Sem essa URI cadastrada, o
      login falha no callback (o equivalente de hoje ao antigo "redireciona para
      localhost")
- [ ] **Backup da chave de criptografia** — irreversível se perder. Isto **continua
      valendo** depois da saída do Supabase: a mesma tabela `private.crypto_keys` foi
      portada para o Neon em `packages/db/migrations/0002_crypto_key_e_usuarios.sql`:

  ```sql
  select version, key from private.crypto_keys;
  ```

  A chave nasce dentro do banco e **nunca sai dele**. Um dump que exclua o schema
  `private` torna **todas as senhas de PDF indecifráveis para sempre**. Guarde num
  gerenciador de senhas e confirme que o backup do deploy inclui o schema — o workflow
  de deploy já tira um dump via `pg_dump` antes de qualquer migration:

  ```bash
  grep -c "crypto_keys" backup-*.sql
  ```

- [ ] **Não é preciso criar usuário.** O dono (`fran.fig@gmail.com`) e o admin de
      emergência (`admin@seubolsofeliz.com.br`) já nascem semeados pela migration
      acima. `LOCAL_USER_ID` só importa para os workers (opcional — `resolveOwnerId()`
      acha o dono sozinho) e para o MCP server (obrigatório) — ver
      [cfg §1.4](cfg.fornecedores.md#14-identidade-local--obrigatória-só-para-o-mcp-server)

### 5.3 ~~🚨 Logo depois do primeiro `db push` — conferir os grants~~ (não se aplica mais)

Esta verificação era específica do Supabase (`service_role`/`authenticated`, RLS) —
ver o histórico no §1. O Neon não tem esse par de papéis nem RLS (`row_security = off`
no dump, `packages/db/schema.sql`); o isolamento é `WHERE user_id = ownerId` explícito
em cada query, e o único papel de banco é o dono da `DATABASE_URL`, que já nasce com
privilégio sobre tudo que criou. Não há "grant" separado para conferir depois do
deploy.

### 5.4 🔒 Vercel

- [ ] **Add New → Project** → importar `Seu-Bolso-Feliz`

  | Configuração   | Valor      |
  | -------------- | ---------- |
  | Framework      | Next.js    |
  | Root Directory | `apps/web` |
  | Build Command  | padrão     |

- [ ] Anotar `VERCEL_PROJECT_ID` e `VERCEL_ORG_ID` (Settings → General)
- [ ] **Settings → Environment Variables**, ambiente **Production** — não existe mais
      `NEXT_PUBLIC_SUPABASE_*`, o banco não expõe nada ao cliente:

  | Variável                           | Valor                                            |
  | ---------------------------------- | ------------------------------------------------ |
  | `DATABASE_URL`                     | connection string Neon, **pooled**               |
  | `AUTH_SECRET`                      | `openssl rand -base64 48`                        |
  | `AUTH_ALLOWED_EMAILS`              | e-mails autorizados a entrar pelo Google         |
  | `GOOGLE_OAUTH_CLIENT_ID`/`_SECRET` | credenciais OAuth do Google Cloud Console (§5.2) |
  | `ADMIN_INITIAL_PASSWORD`           | senha inicial do `admin@` (≥12 caracteres)       |
  | `BLOB_READ_WRITE_TOKEN`            | token do Vercel Blob                             |
  | `NEXT_PUBLIC_APP_URL`              | `https://seudominio.com.br`                      |
  | `OPENAI_API_KEY`                   | só se quiser chat e sugestões na web             |
  | `OPENROUTER_API_KEY`               | alternativa à de cima — tem precedência          |
  | `AI_CHAT_MODEL`                    | opcional; default `gpt-4o`                       |
  | `AI_SUGGEST_MODEL`                 | opcional; default `gpt-4o-mini`                  |

  > Só uma das duas chaves de IA é necessária. No OpenRouter os modelos levam prefixo
  > (`openai/gpt-4o`), então definir a chave sem ajustar os modelos daria erro de modelo
  > inexistente na primeira mensagem do chat.

  > ⚠️ **`DATABASE_URL_UNPOOLED` não vai no Vercel.** A web (serverless) usa só a
  > pooled; a unpooled é para os workers (`LISTEN`/`NOTIFY`) e migrations, que rodam
  > fora da Vercel.

- [ ] **Settings → Domains** — o domínio precisa bater com a URI de redirecionamento
      do Google OAuth (§5.2)

### 5.5 🔒 GitHub

- [ ] **Settings → Secrets and variables → Actions** — os que `deploy.yml` de fato lê
      hoje, bem menos que antes (não há mais projeto Supabase para autenticar):

  | Secret                  | Onde obter                      |
  | ----------------------- | ------------------------------- |
  | `DATABASE_URL_UNPOOLED` | Neon — connection string direta |
  | `VERCEL_TOKEN`          | Vercel → Account → Tokens       |
  | `VERCEL_ORG_ID`         | §5.4                            |
  | `VERCEL_PROJECT_ID`     | §5.4                            |

- [ ] **Settings → Environments → New environment** → `production` → marcar **Required
      reviewers** e se adicionar. Sem staging, esse clique é a última barreira entre uma
      migration ruim e o banco de verdade
- [ ] `git push` da sua branch e acompanhar em **Actions**. O `ci.yml` de hoje sobe um
      Postgres real como serviço (`postgres:17-alpine`), aplica `packages/db/schema.sql` + migrations do zero e roda a suíte inteira — não depende de nenhum stack
      Supabase, nem de uma migration de `GRANT` (histórico, §1)
- [ ] **Actions → Deploy → Run workflow**, e aprovar quando pausar

---

## 6. Subir o ambiente local — reescrito para Neon (⚠️ não reverificado ponta a ponta)

> ⚠️ Esta seção descrevia o stack local do Supabase (`supabase start`, kong, `seed.sql`).
> Isso não existe mais — não há "ambiente local do Supabase" nem `seed.sql` em
> `packages/db/` (confirmado: só há `schema.sql` + 3 migrations, sem arquivo de seed).
> O procedimento abaixo é a reconstrução fiel a partir do código de hoje, mas **não foi
> reexecutado ponta a ponta** para gerar números novos — trate os passos como corretos
> e os números de teste como a pedir reconfirmação.

```bash
export PATH="$HOME/.bun/bin:$PATH"

# Aponte DATABASE_URL/DATABASE_URL_UNPOOLED (.env) para o seu Neon de desenvolvimento,
# ou suba um Postgres descartável (ex.: para rodar a suíte de integração sem tocar o
# banco de trabalho — receita completa em __tests__/helpers/db.ts):
docker run -d --name sbf-integration-test \
  -e POSTGRES_PASSWORD=sbf -e POSTGRES_USER=sbf -e POSTGRES_DB=sbf \
  -p 55480:5432 postgres:17-alpine

psql "postgresql://sbf:sbf@127.0.0.1:55480/sbf" -f packages/db/schema.sql
for m in packages/db/migrations/*.sql; do
  psql "postgresql://sbf:sbf@127.0.0.1:55480/sbf" -f "$m"
done

bun run test:unit                # não toca banco
INTEGRATION_TEST_DATABASE_URL=postgresql://sbf:sbf@127.0.0.1:55480/sbf bun run test:integration
INTEGRATION_TEST_DATABASE_URL=postgresql://sbf:sbf@127.0.0.1:55480/sbf bunx vitest run --project e2e

bun run dev                 # web + worker de ingestão juntos, contra o DATABASE_URL do .env
```

Não existe mais RLS/`GRANT` para conferir (§1, §5.3) — o Postgres do `docker run` acima
usa um único papel (`sbf`), dono de tudo que criou.

**Não há mais `seed.sql`.** O único dado semeado é o dono e o admin de emergência, pela
migration `0002_crypto_key_e_usuarios.sql` — nenhum produto financeiro. Isso significa
que **o ambiente local esbarra no mesmo bloqueio do §7.2** que a produção: sem cadastrar
ao menos um produto financeiro pela UI (ou por INSERT direto), nenhum documento é
lançável, nem localmente.

O `.env` local aponta para o seu Neon (não há mais `127.0.0.1:54321` nem chave fixa de
CLI) — confira com `psql "$DATABASE_URL" -c '\dt'` ou olhando o painel do Neon.

Para exercitar o pipeline com um documento:

```bash
mkdir -p inbox && cp <seu-arquivo>.pdf inbox/
bun run cli --local --dir ./inbox --process
```

> `inbox/` está no `.gitignore`: são seus documentos financeiros, não artefato de
> código.

### Variáveis novas, todas opcionais

Sem nenhuma delas nada muda — os defaults preservam o comportamento anterior.

| Variável                     | Para quê                                            | Default       |
| ---------------------------- | --------------------------------------------------- | ------------- |
| `OPENROUTER_API_KEY`         | usar OpenRouter no lugar da OpenAI; tem precedência | —             |
| `OPENAI_BASE_URL`            | apontar para Ollama, LiteLLM ou endpoint próprio    | —             |
| `AI_CHAT_MODEL`              | modelo do chat da web                               | `gpt-4o`      |
| `AI_SUGGEST_MODEL`           | modelo das sugestões inline                         | `gpt-4o-mini` |
| `INGESTION_ENABLE_IMAGE_OCR` | OCR de foto de boleto (requer `tesseract-ocr-por`)  | `false`       |
| `TESSERACT_BIN`              | caminho do binário do tesseract                     | `tesseract`   |
| `INGESTION_OCR_LANG`         | idiomas do OCR                                      | `por+eng`     |

Todas estão documentadas no `.env.example`.

---

## 7. Depois de subir a produção — configuração de dados

| #   | O quê                                                                                                                                                       | Referência                                                      |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| 7.1 | Ciclo financeiro (dia do seu mês) + gerar períodos via MCP                                                                                                  | [cfg §3](cfg.fornecedores.md#3-ciclo-financeiro)                |
| 7.2 | 🚨 **Ao menos uma instituição e um produto financeiro**                                                                                                     | [cfg §4](cfg.fornecedores.md#4-instituições-produtos-e-cartões) |
| 7.3 | Categorias e tags                                                                                                                                           | [cfg §5](cfg.fornecedores.md#5-categorias-e-tags)               |
| 7.4 | Fornecedores e aliases                                                                                                                                      | [cfg §6](cfg.fornecedores.md#6-fornecedores)                    |
| 7.5 | Cartões — **agora pela interface**                                                                                                                          | `/dashboard/cards` (era só por SQL)                             |
| 7.6 | Senhas de PDF, se tiver fatura protegida                                                                                                                    | `/dashboard/settings/passwords` (era só por SQL)                |
| 7.7 | Gmail: OAuth com redirect de produção                                                                                                                       | [cfg §8](cfg.fornecedores.md#8-gmail)                           |
| 7.8 | Regenerar tipos: `bun run generate-types` + conferir o diff — ⚠️ verificar: o script ainda invoca `supabase gen types` internamente, provavelmente quebrado | —                                                               |

> **7.2 trava tudo.** `transactions.financial_product_id` é `NOT NULL` e o pipeline não
> tem como adivinhar a conta. Sem um produto cadastrado, **nenhum documento pode ser
> lançado**.

### Confirmar que funcionou

```bash
curl -sI https://seudominio.com.br | head -1      # aplicação responde
psql "$DATABASE_URL" -c "\dt" | head               # tabelas presentes (sem "supabase migration list")
# a chave de criptografia existe (deve retornar 1):
psql "$DATABASE_URL" -tAc "select count(*) from private.crypto_keys;"
```

Na aplicação: login → cadastrar instituição e produto → subir documento em
`/dashboard/ingestion` → aprovar e lançar em `/dashboard/ingestion/review` → confirmar a
transação em `/dashboard/transactions`.

---

## 8. Backlog no GitHub — 18 issues, 7 prontas para fechar

Nenhuma foi fechada: os commits não foram enviados, e fechar issue é ação sua.
"Resolvida por código" abaixo significa que existe implementação testada — não
que a issue esteja fechada no GitHub.

| #   | Issue                                            | Estado                                    |
| --- | ------------------------------------------------ | ----------------------------------------- |
| #2  | E6 — jornada da fatura protegida de cartão       | aberta (épico; filhas em aberto)          |
| #3  | F6.1 — Telas de cartão                           | aberta (falta #6)                         |
| #4  | I6.1.1 — `actions/cards.ts` com CRUD e validação | ✅ resolvida por código                   |
| #5  | I6.1.2 — `/dashboard/cards`                      | ✅ resolvida por código                   |
| #6  | I6.1.4 — divergência de `credit_limit`           | aberta — **e mais visível agora**, ver §9 |
| #7  | F6.3 — Motores de domínio da fatura              | aberta (épico de #8, #9, #10)             |
| #8  | I6.3.1 — derivar ciclo a partir do cartão        | aberta                                    |
| #9  | I6.3.2 — detectar parcelas                       | aberta                                    |
| #10 | I6.3.3 — limite comprometido                     | aberta                                    |
| #11 | F6.5 — Goldens e E2E da jornada                  | aberta                                    |
| #12 | I6.5.4 — E2E provando o Marco 2                  | aberta                                    |
| #13 | E10 — formatos anunciados sem parser             | ✅ **épico inteiro** — #14 e #15 feitas   |
| #14 | I10.1 — `supported-formats` como fonte única     | ✅ resolvida por código                   |
| #15 | I10.5 — OCR de imagem                            | ✅ resolvida por código (atrás de flag)   |
| #16 | E11 — domínio financeiro sem consumidor          | aberta (falta #19)                        |
| #17 | I11.1 — `financial-cycle` nos relatórios         | ✅ resolvida por código                   |
| #18 | I11.2 — deduplicação ADR-001 nos relatórios      | ✅ resolvida por código                   |
| #19 | I11.4 — teste de arquitetura contra código morto | aberta                                    |

Depois do push, estas sete fecham juntas:

```bash
for n in 4 5 13 14 15 17 18; do
  gh issue close $n --comment "Resolvida em feat/api-key — ver docs/_atual/_checklist.md §4"
done
```

**Por que só um épico entra na lista.** O #13 fecha porque as duas filhas dele
(#14 e #15) estão prontas. O #16 fica aberto por causa do #19, e o #3 por causa
do #6 — fechar épico com filha em aberto é a maneira mais fácil de perder
trabalho de vista.

O que sobra do E6 é a **inteligência** do cartão, não o cadastro: derivação de
ciclo de fatura (#8), detecção de parcelas (#9), limite comprometido (#10),
goldens e E2E da jornada (#11, #12).

---

## 9. O que **não** vai funcionar mesmo com tudo acima

Não são bloqueadores de subida, mas evitam surpresa.

| Limitação                          | Detalhe                                                                                                                                                                                                                                                                     |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Sem derivação de ciclo de fatura   | O cartão tem fechamento e vencimento cadastrados; nada deriva em qual fatura uma compra cai                                                                                                                                                                                 |
| Sem detecção de parcelas           | `statement_items.installment_number` existe e nada o preenche                                                                                                                                                                                                               |
| Sem limite comprometido            | `cards.credit_limit` é exibido, mas nada calcula quanto dele já está comprometido                                                                                                                                                                                           |
| **Dois `credit_limit` sem regra**  | `cards.credit_limit` e `financial_products.credit_limit` coexistem; a tela nova escreve no primeiro e nada define qual vence — issue [#6](https://github.com/ChicoFigueiredo/Seu-Bolso-Feliz/issues/6)                                                                      |
| Sem agenda consolidada             | A home mostra três listas separadas                                                                                                                                                                                                                                         |
| OCR desligado por padrão           | PDF e imagem: requer `ocrmypdf`/`tesseract-ocr-por` instalados e a flag ligada                                                                                                                                                                                              |
| XLS, DOC, DOCX e QIF não são lidos | **Saíram da interface** de propósito — a promessa foi removida junto com a lacuna                                                                                                                                                                                           |
| Upload acima de ~4,5 MB falha      | Teto de Server Action do Vercel; corrigido em F6.4                                                                                                                                                                                                                          |
| MCP `recompute_financial_periods`  | O defeito antigo (`auth.uid()`/`service_role`) era do Supabase e não existe mais — a tool foi reimplementada em TypeScript (`apps/mcp-server/src/tools/recompute-financial-periods.ts`) e tem chamador real, mas sem teste dedicado ainda; trate como 🟡, não como quebrada |
| ~~Upload-pelo-chat~~               | Resolvido: não existe mais Edge Function `trigger-ingestion`; o chat chama a mesma `uploadDocument()` do upload manual                                                                                                                                                      |
| Lógica de reconciliação duplicada  | `reconciliation.ts` e `api/reconciliation/[draftId]/route.ts` são cópias que podem divergir                                                                                                                                                                                 |
| Nenhum documento seu foi testado   | Todos os fixtures são sintéticos                                                                                                                                                                                                                                            |

---

## 10. Resumo — o que sobrou, na ordem

Tudo que era ⚙️ foi feito. A fila restante é sua.

1. ⚠️ **Verificar se o projeto Supabase antigo ainda existe** (§5.1) e, se sim,
   desativá-lo ou girar as chaves vazadas lá — a aplicação já não depende delas
2. 🔒 **Backup da chave de criptografia** (§5.2) — perdê-la é irreversível, e o
   sintoma só aparece no dia em que uma senha de PDF precisar ser lida
3. 🔒 Projeto Neon, credencial OAuth do Google, variáveis de ambiente e domínio no
   Vercel (§5.2, §5.4)
4. 🔒 Quatro secrets + environment `production` com aprovação no GitHub (§5.5)
5. 🔒 `git push` dos commits → primeira execução do CI → **Run workflow** do Deploy
6. ~~🚨 Conferir os grants em produção~~ — não se aplica mais (§5.3); Neon não tem
   RLS nem papel `service_role` para verificar
7. 🔒 Instituição + produto financeiro (§7.2), senão nada é lançável — vale local
   também, já que não há mais `seed.sql` semeando produtos de teste (§6)
8. 🔒 Mandar algumas faturas suas, anonimizadas: é o único gap de qualidade que
   nenhum teste sintético cobre
9. ⚙️ Depois disso, o que resta do backlog: derivação de ciclo (#8), parcelas
   (#9) e limite comprometido (#10) — é o que falta para a jornada de cartão
   deixar de ser cadastro e virar inteligência
