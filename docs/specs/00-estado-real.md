# 00 — Estado Real do Projeto (mapa único e verificado)

> **Esta é a única fonte de verdade sobre o que está pronto.** Verificada contra o
> código e contra execução em **2026-07-27** (auditoria do plano mestre + execução de P0).
> Sempre que reabrir, **reconfirme rodando** `bun run typecheck`, `bun run lint` e
> `bun run test` antes de confiar em qualquer linha.
>
> Legenda: ✅ Pronto e testado · 🟡 Parcial (existe, falta X) · ⬜ Faltando · 🔒 Bloqueado por você (CEO)

## A regra que faltava

> **Um item só é ✅ com DUAS evidências: (a) um teste que passa E (b) um chamador em
> produção. "Testado mas nunca chamado" é 🟡 por definição.**

Esta regra é o principal aprendizado da auditoria de 2026-07-27. A versão anterior deste
documento marcava o domínio financeiro como ✅ porque os módulos existiam e tinham testes —
sem registrar que **quatro dos cinco não eram importados por nenhuma tela**. O mesmo vale
para a camada de materialização, que tinha 533 linhas, testes de tipo nenhum e **zero
chamadores**: nenhuma UI, action, worker, tool de IA ou MCP a invocava.

## Saúde do código (medida em 2026-08-05 a partir de `db reset` limpo, não afirmada)

| Sinal                | Resultado                    | Como reproduzir                 |
| -------------------- | ---------------------------- | ------------------------------- |
| TypeScript           | ✅ **zero erros**            | `bun run typecheck`             |
| Lint                 | ✅ limpo                     | `bun run lint`                  |
| Formatação           | ✅ limpo                     | `bun run format:check`          |
| Testes unitários     | ✅ **499/499** (30 arquivos) | `bun run test:unit`             |
| Testes de integração | ✅ **66/66** (7 arquivos)    | `bun run test:integration`      |
| Testes E2E           | ✅ **12/12** (2 arquivos)    | `bunx vitest run --project e2e` |
| Build web            | ✅ passa                     | `bun run build`                 |

**Como estes números foram obtidos:** `supabase db reset` limpo, reinício do kong
(ver §11) e as três suítes reexecutadas. Rodar sobre um banco que já acumulou dados
de execuções anteriores esconde exatamente a classe de defeito que o CI encontra.

**Histórico:** os 5 erros de typecheck — dois deles consultas a colunas inexistentes
(`transactions.category`, `ingestion_jobs.step`) — foram corrigidos em `ab07bf4`. Os
unitários eram 319 em 20 arquivos; hoje são 499 em 30, e os projetos `integration` e
`e2e` são contados separadamente.

---

## Mapa por capacidade

### 1. Ingestão (pipeline de documentos) — ✅ com o ciclo agora fechado

Spec detalhado: [`01-ingestao.md`](01-ingestao.md)

| Item                                                             | Estado                       | Evidência                                                             |
| ---------------------------------------------------------------- | ---------------------------- | --------------------------------------------------------------------- |
| Máquina de estados (13 status)                                   | ✅                           | `workers/ingestion/src/state-machine.ts` + testes                     |
| Processor (download→hash→parse→enrich→obrigação→draft)           | ✅                           | `workers/ingestion/src/processor.ts`                                  |
| Extração de texto PDF nativo                                     | ✅                           | `parsers/pdf-text.ts` (pdfjs-dist) + **E2E com PDF real**             |
| Parsers determinísticos (boleto, CEMIG, boleto-utils, templates) | ✅ (boleto-parser 🟡 básico) | `parsers/*.ts`                                                        |
| Consenso de campos multi-fonte                                   | ✅                           | `parsers/field-consensus.ts`                                          |
| Enriquecimento IA lite/full                                      | ✅                           | `parsers/ai-lite-enricher.ts`, `ai-full-enricher.ts`                  |
| **Contrato de draft unificado e versionado**                     | ✅                           | `packages/contracts/` (57 testes) — ver §9 abaixo                     |
| **Convergência para obrigação canônica**                         | ✅                           | `workers/ingestion/src/obligations/` + `obligations.test.ts` (10)     |
| Scanner Gmail com `--query` e período                            | ✅                           | `query-builder.ts` + `gmail-client.ts` — corrigido em P0-9            |
| Scanner pasta local (scan-once + watch)                          | ✅                           | `workers/local-scanner/`                                              |
| **CLI único de orquestração**                                    | ✅                           | `workers/cli/` (26 testes) — substitui `financial-evidence-worker`    |
| OCR de PDF escaneado e de imagem                                 | 🟡                           | existe; `INGESTION_ENABLE_OCRMYPDF` e `..._IMAGE_OCR` default `false` |
| Checkpoint de backfill retomável                                 | 🟡                           | tabela `ingestion_checkpoints` criada; escrita pelo scanner pendente  |

### 2. Formatos suportados — ✅ o que a UI anuncia é o que o código lê

Fonte única: `packages/contracts/src/formats.ts`. A regra do registro é **um
formato só entra quando existe código que o lê**; não há categoria "planejado".

| Formato    | Estado | Realidade                                                                 |
| ---------- | ------ | ------------------------------------------------------------------------- |
| PDF        | ✅     | `pdfjs-dist`, com retry de senha. Coberto por E2E com PDF real.           |
| CSV        | ✅     | delimitador detectado por consistência; latin-1 e BOM tratados            |
| XLSX       | ✅     | `exceljs`; datas convertidas para o padrão brasileiro                     |
| OFX        | ✅     | SGML (1.x) e XML (2.x) com o mesmo código; aceita vírgula decimal         |
| XML        | ✅     | lido como texto — o que se extrai depende do documento                    |
| Imagens    | 🟡     | OCR via tesseract atrás de `INGESTION_ENABLE_IMAGE_OCR` (default `false`) |
| XLS        | ⬜     | **removido da UI**: o `exceljs` lê XLSX e não lê XLS                      |
| DOC / DOCX | ⬜     | **removidos da UI**: sem parser, e nenhum scanner os aceitava             |
| QIF        | ⬜     | **removido da UI**: nem estava na lista de nenhum scanner                 |

> O teste de equivalência (`formats.test.ts`) lê os arquivos reais — os três
> componentes de upload, os dois scanners e a migration do bucket — e falha se
> alguém reintroduzir uma lista literal ao lado do import. Eram cinco listas
> divergentes antes disso.

### 3. UI de ingestão e revisão — ✅ o ciclo fecha; 🟡 falta split-view

| Item                                                     | Estado | Evidência                                              |
| -------------------------------------------------------- | ------ | ------------------------------------------------------ |
| `/dashboard/ingestion` (visão geral, stats, upload)      | ✅     | `dashboard/ingestion/page.tsx`                         |
| **Revisão com aprovação → lançamento real**              | ✅     | `draft-review-form.tsx` + `actions/materialization.ts` |
| **Seletor de conta no lote e por rascunho**              | ✅     | `draft-review-form.tsx` — sem ele nada é lançável      |
| **Estado intermediário "aprovado, não lançado" visível** | ✅     | `draft-review-form.tsx`                                |
| **Falha parcial com os campos exatos que faltam**        | ✅     | render de `validationErrors`                           |
| `/ingestion/documents` + `[id]`                          | ✅     | `ingestion/documents/`                                 |
| `/ingestion/patterns` + `[id]`                           | 🟡     | UI existe; ciclo de feedback não fecha                 |
| `/ingestion/logs`                                        | ✅     | `ingestion/logs/page.tsx`                              |
| Split-view documento × draft com edição inline           | 🟡     | aprovar/lançar/rejeitar OK; sem painel lado-a-lado     |
| Gestão de senhas de documentos protegidos                | ✅     | `/dashboard/settings/passwords`                        |

### 4. IA e Chat — ✅ integrado

| Item                                         | Estado | Evidência                                                                      |
| -------------------------------------------- | ------ | ------------------------------------------------------------------------------ |
| Chat drawer com histórico e streaming        | ✅     | `ai-chat-drawer.tsx`, `api/chat/route.ts`                                      |
| Sugestões inline                             | ✅     | `api/ai-suggest/route.ts`                                                      |
| Rate limiting                                | ✅     | `api/chat/route.ts`                                                            |
| **`approve_draft` e `post_draft` separadas** | ✅     | `lib/ai/tools.ts` — a IA não cria dinheiro numa chamada só                     |
| Explicabilidade                              | 🟡     | tool existe; calibração pendente                                               |
| Upload-pelo-chat                             | 🟡     | chama `trigger-ingestion`, uma Edge Function **quebrada** contra o schema real |

### 5. Materialização e integridade — ✅ (seção nova)

Esta seção não existia. A omissão importava: a camada estava quebrada em três pontos
independentes e o documento não registrava sequer que ela existia.

| Item                                            | Estado | Evidência                                                                          |
| ----------------------------------------------- | ------ | ---------------------------------------------------------------------------------- |
| Contrato compartilhado gerador ↔ materializador | ✅     | `packages/contracts/` — antes eram schemas divergentes, 100% dos drafts reprovavam |
| Materialização em transação única               | ✅     | `fn_materialize_draft_record` + 13 testes de integração                            |
| Idempotência sob concorrência                   | ✅     | teste de `Promise.all`: 1 posted, N already_posted                                 |
| `pending_review` não pode ser materializado     | ✅     | guarda na RPC + teste                                                              |
| Índice único contra duplo lançamento            | ✅     | `uq_draft_records_posted_record`                                                   |
| Auditoria de materialização                     | ✅     | dentro da mesma transação — antes gravava em colunas inexistentes e engolia o erro |
| Aprovação e lançamento distintos                | ✅     | 5 pontos de entrada, todos separados                                               |
| Compensação/retry visível na UI                 | ✅     | `materialization_error` + botão "tentar novamente"                                 |

### 6. Conciliação e obrigações — ✅ motor; 🟡 UX

| Item                                              | Estado | Evidência                                                                |
| ------------------------------------------------- | ------ | ------------------------------------------------------------------------ |
| Motor determinístico de reconciliação             | ✅     | `reconciliation/reconciliation.ts`                                       |
| **`financial_obligations` conectada ao pipeline** | ✅     | `obligations/obligation-writer.ts` — antes: zero código a tocava         |
| **Identity key que realmente agrupa**             | ✅     | `identity-key.ts` (20 testes) — antes, fatura e lembrete jamais colidiam |
| Supressão de draft duplicado por obrigação        | ✅     | `draft-generator.ts` + E2E cross-canal                                   |
| UI dedicada de conciliação                        | 🟡     | sugestão inline existe; tela própria a definir                           |
| Lógica de reconciliação duplicada em 2 lugares    | ⬜     | `reconciliation.ts` e `api/reconciliation/[draftId]/route.ts` são cópias |

### 7. Domínio financeiro — 🟡 biblioteca testada, quase toda sem consumidor

Esta é a correção mais importante do documento. A versão anterior marcava tudo ✅.

| Item                                   | Estado | **Consumidor em produção?**                                                                                       |
| -------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------- |
| Ciclo financeiro personalizado         | ✅     | `reports/report-period.ts` chama `getCurrentPeriod` (#17)                                                         |
| Amortização SAC/Price/Misto + quitação | 🟡     | **Nenhum.** `liabilities/[id]` só renderiza parcelas armazenadas                                                  |
| Prioridade de pagamento (5 níveis)     | ✅     | `dashboard/page.tsx:119` usa `prioritizeItems` — único consumidor                                                 |
| Deduplicação ADR-001                   | ✅     | `reports/page.tsx` consome `v_expenses_deduplicated` (#18) — 6 testes de integração                               |
| Identity key financeira                | ✅     | `obligations/obligation-writer.ts` — ganhou consumidor em P0-7                                                    |
| Telas CRUD (transactions…settings)     | ✅     | existem e funcionam como CRUD                                                                                     |
| Detecção de recorrência por histórico  | ⬜     | a regra antiga (`hasSupplier && hasCompetence`) foi **removida** em P0-1 por criar recorrência de um documento só |
| Derivação de ciclo de fatura           | ⬜     | não existe                                                                                                        |
| Detecção de parcelas                   | ⬜     | não existe                                                                                                        |
| Limite comprometido do cartão          | ⬜     | `cards.credit_limit` não é lido por nenhum código                                                                 |
| Agenda consolidada de pagamentos       | ⬜     | home mostra 3 listas separadas; `liability_installments` não aparece                                              |
| Projeção de caixa                      | ⬜     | não existe                                                                                                        |
| Avalanche / bola de neve               | ⬜     | não existe                                                                                                        |
| Rota `/dashboard/cards`                | ✅     | lista, criação e edição, com limite, fechamento e vencimento (#4, #5)                                             |

### 8. Segurança e segredos — ✅ corrigido em P0-8

| Item                                 | Estado | Evidência                                                                                             |
| ------------------------------------ | ------ | ----------------------------------------------------------------------------------------------------- |
| Criptografia real de segredos        | ✅     | `private.crypto_keys` + `encrypt/decrypt_secret` — antes a chave vinha de uma GUC que ninguém definia |
| Leitura decriptada, só service_role  | ✅     | `fn_get_secrets` + 8 testes                                                                           |
| Escrita sem superfície de forja      | ✅     | `fn_set_secret` usa `auth.uid()`, não aceita `p_user_id`                                              |
| Senha nunca em log                   | ✅     | teste com sentinela                                                                                   |
| `audit_logs` imutável                | ✅     | DELETE recusado por política                                                                          |
| Rotação de chave                     | 🟡     | estrutura pronta (`encryption_version`); procedimento a documentar                                    |
| Rotação das chaves Supabase expostas | 🔒     | pendência aberta (commit `38b8126`)                                                                   |
| Repositório público                  | 🔒     | app financeiro pessoal, público, com histórico de vazamento                                           |

### 9. MCP server — ✅ 10 tools

| Item                                                      | Estado | Evidência                                                                                      |
| --------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------- |
| 9 tools originais                                         | ✅     | `apps/mcp-server/src/index.ts`                                                                 |
| **`post_draft_batch`** (lançamento separado da aprovação) | ✅     | `tools/post-draft-batch.ts`                                                                    |
| `recompute_financial_periods`                             | ⬜     | **nunca funcionou**: nomes de parâmetro divergem da RPC e `auth.uid()` é NULL sob service_role |
| Tools de Gmail / conciliação / decisão financeira via MCP | ⬜     | —                                                                                              |
| Identidade de dispositivo e auditoria por chamador        | ⬜     | usa `SUPABASE_SECRET_KEY` + `LOCAL_USER_ID`                                                    |

### 10. Infra, deploy e ambientes — 🟡 escrito e verde localmente; nunca executado em nuvem

| Item                                   | Estado | Evidência                                                                                   |
| -------------------------------------- | ------ | ------------------------------------------------------------------------------------------- |
| **Privilégios das tabelas para a API** | ✅     | `20260804220000_grant_api_roles.sql` — ver §11, era o bloqueador de tudo                    |
| Migração para GitHub Actions           | ✅     | `.github/workflows/{ci,deploy}.yml` — o `.gitlab-ci.yml` não existe mais                    |
| **Deploy web (Vercel)**                | ✅     | `deploy.yml` roda `vercel pull/build/deploy --prod` de verdade                              |
| Primeira execução do CI                | ⬜     | **nunca rodou**: 18 commits ainda não enviados                                              |
| `.vercel/project.json` commitado       | ✅     | não está versionado — `git ls-files .vercel/` vazio, e `.gitignore` cobre                   |
| Cobertura de teste                     | 🟡     | inclui só `packages/{domain,validation,operations,contracts}`; `apps/web` e `workers/` fora |
| Promoção entre ambientes               | ⬜     | não existe, por decisão: ambiente único                                                     |
| Segredos do GitHub e do Vercel         | 🔒     | seis secrets e as variáveis do Vercel dependem de você                                      |

### 11. O bloqueador que estava invisível — privilégios de tabela

Descoberto por execução em 2026-08-04, não por leitura. Depois de um
`supabase db reset` limpo, **nenhuma** das 47 tabelas e views de `public` era
acessível por `service_role` nem por `authenticated`: o worker recebia
`permission denied for table ingestion_runs`, a web não lia nada, e 29 dos 60
testes de integração falhavam.

As migrations criavam objetos como papel `postgres` e dependiam de um
`ALTER DEFAULT PRIVILEGES` implícito do Supabase que existe **só para
`supabase_admin`**. O privilégio nunca foi concedido — apenas presumido. A
chave legacy `service_role` falhava exatamente igual à `sb_secret_...`, o que
descarta problema de formato de chave.

| Depois da migration         | Antes |
| --------------------------- | ----- |
| 47/47 tabelas acessíveis    | 0/47  |
| integração **66/66**        | 31/60 |
| E2E **12/12** em reset frio | 10/12 |

## Gaps reais, em ordem de importância

Atualizado em 2026-08-05. Os fechados ficam na lista, riscados, porque saber o
que já foi resolvido evita reabrir a discussão.

1. **Rotação das chaves Supabase expostas** — pendência de segurança aberta desde o commit `38b8126`, num repositório público. (🔒 você)
2. **Primeira execução do CI** — os workflows existem e nunca rodaram: 18 commits ainda não enviados. A paridade só se confirma na primeira execução.
3. **Validação com documentos reais seus** — o pipeline tem E2E com PDF sintético e nunca viu suas faturas. (🔒 depende de você fornecer amostras anonimizadas)
4. **Inteligência de cartão** — `/dashboard/cards` agora existe (#4, #5), mas derivação de ciclo (#8), parcelas (#9) e limite comprometido (#10) continuam abertos.
5. **OCR desligado por padrão** — `INGESTION_ENABLE_OCRMYPDF` e `INGESTION_ENABLE_IMAGE_OCR` default `false`. O código existe; ligar depende de instalar os binários.
6. **Lógica de reconciliação duplicada** — `reconciliation.ts` e `api/reconciliation/[draftId]/route.ts` são cópias que podem divergir.
7. **Upload-pelo-chat** — chama `trigger-ingestion`, uma Edge Function quebrada contra o schema real.

Fechados em 2026-08-05:

- ~~**Privilégios de tabela**~~ — descoberto e corrigido. Era o bloqueador de tudo; ver §11.
- ~~**Formatos anunciados sem parser**~~ — XLSX, OFX e CSV têm parser; `.doc`, `.docx`, `.qif` e `.xls` saíram da UI. Registro único em `@sbf/contracts/formats`.
- ~~**Domínio financeiro sem consumidor**~~ — relatórios chamam `financial-cycle` (#17) e a view `v_expenses_deduplicated` (#18).
- ~~**Deploy web real**~~ — `deploy.yml` publica no Vercel de verdade.
- ~~**5 erros de typecheck**~~ — corrigidos; dois eram bugs de coluna inexistente.
- ~~**`pdf-parse@1.1.1`**~~ — substituído por `pdfjs-dist`. A build de 2018 devolvia erros DIFERENTES para os mesmos bytes no mesmo processo, e reprovava o E2E em toda primeira execução após reset.

## Como manter este mapa honesto

- Toda mudança de estado aqui precisa de **evidência**: caminho de arquivo + teste + chamador.
- Nada vira ✅ sem as **duas** evidências da regra no topo.
- Cada item fechado atualiza esta tabela **no mesmo commit** que o fecha.
- Números de teste são **medidos**, nunca copiados de outra seção: `test:unit`,
  `test:integration` e `e2e` são projetos distintos e têm contagens distintas.
