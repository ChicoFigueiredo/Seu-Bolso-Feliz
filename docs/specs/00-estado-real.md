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

## Saúde do código (medida em 2026-07-27, não afirmada)

| Sinal                | Resultado                    | Como reproduzir                 |
| -------------------- | ---------------------------- | ------------------------------- |
| TypeScript           | ✅ **zero erros**            | `bun run typecheck`             |
| Lint                 | ✅ limpo                     | `bun run lint`                  |
| Formatação           | ✅ limpo                     | `bun run format:check`          |
| Testes unitários     | ✅ **433/433** (25 arquivos) | `bun run test:unit`             |
| Testes de integração | ✅ **60/60** (6 arquivos)    | `bun run test:integration`      |
| Testes E2E           | ✅ **12/12** (2 arquivos)    | `bunx vitest run --project e2e` |
| Build web            | ✅ passa                     | `bun run build`                 |

**Correções em relação à versão anterior deste documento:**

- Dizia "TypeScript ✅ passa limpo". **Não passava**: havia 7 erros, medidos contra o
  baseline com as dependências antigas. Restam 5, em 3 arquivos (`document-metadata.ts`,
  `api/reconciliation/[draftId]/route.ts`, `lib/ai/tools.ts`). Dois deles são bugs reais
  que a tipagem mais estrita expôs: consultas às colunas `transactions.category` e
  `ingestion_logs.step`, que **não existem**.
- Dizia "319/319 (20 arquivos)" para testes unitários. O número correto hoje é 433/433 em
  25 arquivos, e os projetos `integration` e `e2e` são contados separadamente.
- `lint` e `format:check` também estavam vermelhos (1 erro de lint, 38 arquivos fora de
  formato) e não eram mencionados.

---

## Mapa por capacidade

### 1. Ingestão (pipeline de documentos) — ✅ com o ciclo agora fechado

Spec detalhado: [`01-ingestao.md`](01-ingestao.md)

| Item                                                             | Estado                       | Evidência                                                            |
| ---------------------------------------------------------------- | ---------------------------- | -------------------------------------------------------------------- |
| Máquina de estados (13 status)                                   | ✅                           | `workers/ingestion/src/state-machine.ts` + testes                    |
| Processor (download→hash→parse→enrich→obrigação→draft)           | ✅                           | `workers/ingestion/src/processor.ts`                                 |
| Extração de texto PDF nativo                                     | ✅                           | `parsers/text-extractor.ts` + **E2E com PDF real**                   |
| Parsers determinísticos (boleto, CEMIG, boleto-utils, templates) | ✅ (boleto-parser 🟡 básico) | `parsers/*.ts`                                                       |
| Consenso de campos multi-fonte                                   | ✅                           | `parsers/field-consensus.ts`                                         |
| Enriquecimento IA lite/full                                      | ✅                           | `parsers/ai-lite-enricher.ts`, `ai-full-enricher.ts`                 |
| **Contrato de draft unificado e versionado**                     | ✅                           | `packages/contracts/` (57 testes) — ver §9 abaixo                    |
| **Convergência para obrigação canônica**                         | ✅                           | `workers/ingestion/src/obligations/` + `obligations.test.ts` (10)    |
| Scanner Gmail com `--query` e período                            | ✅                           | `query-builder.ts` + `gmail-client.ts` — corrigido em P0-9           |
| Scanner pasta local (scan-once + watch)                          | ✅                           | `workers/local-scanner/`                                             |
| **CLI único de orquestração**                                    | ✅                           | `workers/cli/` (26 testes) — substitui `financial-evidence-worker`   |
| OCR de PDF escaneado (`ocrmypdf`)                                | 🟡                           | existe, mas `INGESTION_ENABLE_OCRMYPDF` é **desligado por padrão**   |
| Checkpoint de backfill retomável                                 | 🟡                           | tabela `ingestion_checkpoints` criada; escrita pelo scanner pendente |

### 2. Formatos suportados — 🟡 a UI anuncia mais do que o código faz

| Formato    | Estado | Realidade                                                                     |
| ---------- | ------ | ----------------------------------------------------------------------------- |
| PDF        | ✅     | `pdf-parse`, com retry de senha. Coberto por E2E com PDF real.                |
| CSV        | 🟡     | lido como texto puro; **não há parser de CSV** (sem delimitador, sem latin-1) |
| Imagens    | ⬜     | `extractionMethod: "image_placeholder"`, texto vazio. **Sem OCR.**            |
| XLSX / XLS | ⬜     | cai em `Buffer.toString()`. XLSX é ZIP → lixo binário.                        |
| DOC / DOCX | ⬜     | idem                                                                          |
| OFX        | ⬜     | sem parser. É SGML, "lê" como texto, mas nada é extraído.                     |
| QIF        | ⬜     | **nem sequer está nas extensões aceitas pelos scanners**                      |

> A interface de upload anuncia todos esses formatos
> (`document-upload-dnd.tsx`, `upload-documents.tsx`, `ai-chat-drawer.tsx`).
> Isso é uma promessa não cumprida e deve ser corrigido junto com P1.

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
| Gestão de senhas de documentos protegidos                | 🟡     | `actions/secrets.ts` existe; **falta a tela**          |

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
| Ciclo financeiro personalizado         | 🟡     | **Nenhum.** `reports/page.tsx:36-46` reimplementa a matemática inline                                             |
| Amortização SAC/Price/Misto + quitação | 🟡     | **Nenhum.** `liabilities/[id]` só renderiza parcelas armazenadas                                                  |
| Prioridade de pagamento (5 níveis)     | ✅     | `dashboard/page.tsx:119` usa `prioritizeItems` — único consumidor                                                 |
| Deduplicação ADR-001                   | 🟡     | **Nenhum.** Relatórios somam `transactions` cru e ignoram a view `v_expenses_deduplicated`                        |
| Identity key financeira                | ✅     | `obligations/obligation-writer.ts` — ganhou consumidor em P0-7                                                    |
| Telas CRUD (transactions…settings)     | ✅     | existem e funcionam como CRUD                                                                                     |
| Detecção de recorrência por histórico  | ⬜     | a regra antiga (`hasSupplier && hasCompetence`) foi **removida** em P0-1 por criar recorrência de um documento só |
| Derivação de ciclo de fatura           | ⬜     | não existe                                                                                                        |
| Detecção de parcelas                   | ⬜     | não existe                                                                                                        |
| Limite comprometido do cartão          | ⬜     | `cards.credit_limit` não é lido por nenhum código                                                                 |
| Agenda consolidada de pagamentos       | ⬜     | home mostra 3 listas separadas; `liability_installments` não aparece                                              |
| Projeção de caixa                      | ⬜     | não existe                                                                                                        |
| Avalanche / bola de neve               | ⬜     | não existe                                                                                                        |
| Rota `/dashboard/cards`                | ⬜     | **não é possível cadastrar um cartão pela interface**                                                             |

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

### 10. Infra, deploy e ambientes — 🟡 o maior buraco restante

| Item                                    | Estado | Evidência                                                                                   |
| --------------------------------------- | ------ | ------------------------------------------------------------------------------------------- |
| Deploy de migrations e Edge Functions   | ✅     | `.gitlab-ci.yml` — real e funcional                                                         |
| **Deploy web (Vercel)**                 | ⬜     | ainda é `echo "configurar provedor"` (`.gitlab-ci.yml:188,231`)                             |
| Migração para GitHub Actions            | ⬜     | decidida, não executada                                                                     |
| `.vercel/project.json` commitado        | ⬜     | artefato local que expõe org/project id em repo público                                     |
| Filtro `changes:` de `test-integration` | ⬜     | não inclui `workers/**` — **nunca disparou** em mudança de ingestão                         |
| Cobertura de teste                      | 🟡     | inclui só `packages/{domain,validation,operations,contracts}`; `apps/web` e `workers/` fora |
| Promoção entre ambientes                | ⬜     | não existe                                                                                  |

---

## Gaps reais, em ordem de importância

1. **Formatos anunciados sem parser** — a UI aceita XLSX/OFX/DOCX/QIF e o código não os lê. Ou o parser entra, ou a promessa sai.
2. **Domínio financeiro sem consumidor** — amortização, ciclo e deduplicação são bibliotecas testadas que nenhuma tela chama. É o maior desperdício de código pronto do repositório.
3. **Nenhuma inteligência de cartão** — sem `/dashboard/cards`, sem derivação de ciclo, sem parcelas, sem limite comprometido. É o caminho crítico da jornada P1 escolhida.
4. **Deploy web real** — placeholder no CI. (🔒 credenciais Vercel)
5. **Validação com documentos reais seus** — o pipeline agora tem um E2E com PDF sintético, mas nunca viu suas faturas. (🔒 depende de você fornecer amostras anonimizadas)
6. **OCR desligado por padrão** — `INGESTION_ENABLE_OCRMYPDF` default `false`, e imagens não têm OCR nenhum.
7. **5 erros de typecheck**, dois deles bugs reais de coluna inexistente.
8. **Rotação das chaves Supabase expostas** — pendência de segurança aberta. (🔒 você)

## Como manter este mapa honesto

- Toda mudança de estado aqui precisa de **evidência**: caminho de arquivo + teste + chamador.
- Nada vira ✅ sem as **duas** evidências da regra no topo.
- Cada item fechado atualiza esta tabela **no mesmo commit** que o fecha.
- Números de teste são **medidos**, nunca copiados de outra seção: `test:unit`,
  `test:integration` e `e2e` são projetos distintos e têm contagens distintas.
