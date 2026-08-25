# Integração Pluggy (Open Finance)

> Verificado contra código em 2026-08-24 (branch `feat/pluggy-integration-plan`,
> HEAD `da82084`). Segue a convenção de `docs/specs/`: comportamento e contrato,
> não ata de reunião — se este doc divergir do código, o doc está errado.
>
> Plano completo (fases, gates, decisões de arquitetura):
> [`docs/planejamento/2026-08-24-plano-integracao-pluggy.md`](../planejamento/2026-08-24-plano-integracao-pluggy.md).
> Critérios de aceite originais: [`docs/prompts/2026-08-24-prompt-integracao-pluggy.md`](../prompts/2026-08-24-prompt-integracao-pluggy.md) §37.

## O que é

Pluggy é o provedor de Open Finance: dado o consentimento do usuário, expõe contas
bancárias e transações via API. A integração busca essas transações e as injeta no
pipeline de ingestão **existente** (`draft_records` → reconciliação → revisão humana),
sem motor paralelo — a mesma superfície que já processa boletos e emails.

## Arquitetura (contrato verificado)

```
PluggyClient (REST)  →  PluggyProvider (implementa FinancialDataProvider)
                              │
                              ▼
                    NormalizedTransaction
                              │
              ┌───────────────┼────────────────┐
              ▼                                 ▼
   resolveSupplierForTransaction      pluggyTransactionToDraft
   (@sbf/domain — alias exato)         (NormalizedTransaction → TransactionDraftV1)
              │                                 │
              └───────────────┬─────────────────┘
                               ▼
                    buildDraftRecordInsert
                               │
                               ▼
                  INSERT draft_records (idempotente via external_ref)
                               │
                               ▼
                  findReconciliationCandidates (motor existente, não Pluggy-específico)
```

| Camada                                            | Arquivo                                                                                                                 |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Cliente REST + cache de API key                   | `packages/financial-connectors/src/pluggy/pluggy-client.ts`                                                             |
| Contrato genérico de provedor                     | `packages/financial-connectors/src/financial-data-provider.ts`                                                          |
| Implementação Pluggy → NormalizedTransaction      | `packages/financial-connectors/src/pluggy/pluggy-provider.ts`                                                           |
| NormalizedTransaction → TransactionDraftV1        | `packages/financial-connectors/src/pluggy/map-to-draft.ts`                                                              |
| Resolução de fornecedor (alias exato)             | `packages/domain/src/supplier-resolution/index.ts`                                                                      |
| Orquestração do sync (por conexão)                | `workers/pluggy-sync/src/sync-runner.ts`                                                                                |
| Checkpoint de backfill (retomável)                | `workers/pluggy-sync/src/checkpoint.ts`                                                                                 |
| Draft → linha de `draft_records`                  | `workers/pluggy-sync/src/build-draft.ts`                                                                                |
| Motor de reconciliação (não é Pluggy-específico)  | `workers/ingestion/src/reconciliation/reconciliation.ts`                                                                |
| Conexão (widget Connect)                          | `apps/web/src/components/pluggy-connect-button.tsx`                                                                     |
| Lista de conexões / mapeamento de conta → produto | `apps/web/src/app/dashboard/connections/page.tsx`, `.../connections/[id]/page.tsx`                                      |
| Server actions de conexão/mapeamento              | `apps/web/src/app/actions/pluggy-connections.ts`                                                                        |
| Schema: conexões, mapeamento de conta, checkpoint | `supabase/migrations/20260824010000_create_provider_connections.sql`, `20260824030000_ingestion_checkpoints_pluggy.sql` |

## Resolução de fornecedor (Fase 4)

`resolveSupplierForTransaction(rawName, candidates)` é pura — recebe o nome bruto da
transação e os `supplier_aliases` ativos já buscados (por `user_id`, uma vez por sync,
não por transação) e devolve:

```ts
{ status: "matched"; supplierId: string }
  | { status: "needs_review"; reason: "no_exact_alias_match" }
```

Escopo deliberadamente mínimo: **igualdade exata** após `normalizeName` (lowercase, sem
acento, pontuação → `_`). Sem motor de scoring (CPF/CNPJ/NSU) — decisão registrada, não
esquecimento: `supplier_aliases` não tem coluna normalizada nem índice sobre
`normalizeName`, só um trigger de unicidade em `LOWER(alias_name)` (ver
`docs/_arquivo/adrs/ADR-003-governanca-aliases-fornecedor.md` §2.7, que já escopa exato
como auto-aceite e fuzzy como sempre-confirmação-humana). Sem match, `supplierId` fica
`null` e o draft nasce honesto mesmo assim — mesmo princípio de `financial_product_id`
null quando a conta ainda não foi mapeada: humano resolve na revisão, nunca autopost
sobre incerteza.

## Bug corrigido durante a Fase 4: motor de reconciliação estava morto para match exato

`reconciliation.ts` nunca tinha sido exercitado contra o schema real antes desta fase —
consultava colunas inexistentes em `transactions` (`transaction_date`/`supplier_name`/
`category`, corretos: `event_date`/sem coluna equivalente/`category_id`) e em
`recurring_instances` (`due_date`/`template_id`, corretos: `expected_date`/
`recurring_template_id`), escondido atrás de casts `as never`/`as unknown` que
bypassavam o typecheck. O erro do Supabase nunca era checado — a Regra 2 (match
exato/aproximado, a regra principal) sempre retornava zero candidatos, silenciosamente,
em produção, não só em teste. Afeta o pipeline de documento inteiro, não só Pluggy.
Corrigido em `4d20000` — ver commit para detalhes e o teste de reprodução
(`__tests__/integration/reconciliation.test.ts`).

## Gate da Fase 4 — critérios de aceite (§37)

> "casos de teste do prompt §37 (100 transações sintéticas, 40 e-mails, 20 PDFs, 15
> recorrências, 10 parcelamentos, 10 conflitos, 10 ambíguos) rodando como suíte, sem
> payload real de usuário no repo." — plano, Fase 4.

Satisfeito. Dataset 100% sintético e determinístico (sem `Date.now()`/`Math.random()`,
sem dado real de usuário) em `__tests__/fixtures/pluggy-reconciliation/`
(`transactions.ts`, `emails.ts`, `documents.ts`), exercitado por 52 testes em
`__tests__/integration/pluggy-reconciliation-fixtures.test.ts` contra
`findReconciliationCandidates` de verdade (Supabase local, sem mock):

| Critério do §37                          | Como é demonstrado                                                                                                                                                                       |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Correspondência exata                    | 10 emails com fornecedor/valor/data certos → `match_exact`, score 0.95                                                                                                                   |
| Correspondência aproximada               | 5 emails com valor certo, data fora da janela de 7 dias → `match_fuzzy`                                                                                                                  |
| Não-correspondência (score insuficiente) | 10 fornecedor desconhecido + 15 sem `due_date` → `no_match`, duas causas distintas                                                                                                       |
| Detecção de ambiguidade                  | 5 grupos com 2 transações do mesmo fornecedor/valor/data próximos → `candidates.length > 1`, scores a até 0.25 de distância                                                              |
| Prevenção de falso positivo              | 5 pares de conflito (fornecedor diferente, mesmo valor/data) nunca viram candidato; parcelas da mesma compra nunca no mesmo nível de confiança (só 1 `match_exact`, resto `match_fuzzy`) |
| Preservação de evidência                 | par de documento com `content_hash` idêntico de verdade (via `computeContentHash`) → `match_duplicate`; as duas linhas em `source_documents` continuam existindo depois                  |

"Ambiguidade" não é um status nativo do motor — estender `reconciliation.ts` para isso
foi avaliado e descartado nesta fase (raio de impacto maior que o gate da Fase 4 pede,
já que o motor atende o pipeline de documento inteiro). Detectada no nível do teste:
mais de um candidato plausível com score próximo, decisão explícita de deixar a escolha
final para o humano em vez de o motor arbitrar.

## Rodando os testes localmente

```bash
supabase start
supabase status -o env | grep SECRET_KEY   # SUPABASE_SECRET_KEY, cole no .env local

bun run test:unit                            # inclui packages/domain/src/supplier-resolution
bun run test:integration                     # inclui reconciliation.test.ts e pluggy-reconciliation-fixtures.test.ts
```

## Fora de escopo desta fase (registrado, não esquecido)

- Motor de scoring completo (CPF/CNPJ/NSU) no resolver de fornecedor — só igualdade
  exata de alias por enquanto.
- Status "ambíguo" nativo no motor de reconciliação.
- Deploy do worker na VPS e produção Vercel+Neon — Fases 5-7 do plano, bloqueadas por
  acesso que a sessão de desenvolvimento não tem (🤚).
