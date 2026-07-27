# 02 — Spec: Conciliação (documento × registros financeiros)

> **Estado: ✅ motor determinístico pronto · 🟡 UX de revisão.**
> Verificado contra código em 2026-06-20. Ver [`00-estado-real.md`](00-estado-real.md).

## Objetivo (comportamento desejado)

Antes de virar registro definitivo, todo draft precisa responder: **isto é lançamento
novo, ou já existe?** O sistema sugere candidatos de match (transação/fatura/recorrência
já existentes), classifica o risco de duplicidade e deixa a decisão final para humano —
nunca faz autopost sobre ambiguidade.

## Motor de conciliação (contrato)

Implementação: `workers/ingestion/src/reconciliation/reconciliation.ts`.
Chamado em `processor.ts` no `stepDraft()`, antes de gerar o draft. Resultado gravado nas
colunas `reconciliation_status` / `reconciled_transaction_id` / `reconciliation_candidates`
de `draft_records` (migração `20260403110000`).

`findReconciliationCandidates()` aplica 4 regras, em ordem de força:

| Regra | Sinal | Status | Score |
| --- | --- | --- | --- |
| Duplicata | mesmo `content_hash` | `match_duplicate` | 1.0 |
| Exato | `supplier_id` + valor ±5% + vencimento ±7 dias | `match_exact` | 0.95 |
| Fuzzy | `supplier_id` + valor ±5%, data diverge | `match_fuzzy` | 0.7 |
| Recorrência | `supplier_id` + padrão mensal esperado | `match_recurring` | — |

`isDuplicateRisk()` → `true` quando `match_duplicate`. **Política: duplicata bloqueia autopost.**

## Camada de API (web)

- `GET /api/reconciliation/[draftId]` — roda o motor, retorna candidatos rankeados + cache. ✅
- `PATCH /api/reconciliation/[draftId]` — confirma decisão humana (novo / duplicado). ✅
- `GET /api/reconciliation/[draftId]/progress` — RPC `fn_reconciliation_progress`. ✅
- Sugestão inline via IA: tool `suggest_reconciliation` em `/api/ai-suggest`. ✅

## Matriz de decisão documental (a formalizar como contrato testado)

A lógica de "que tipo de documento gera que tipo de registro" hoje vive em `draft-generation`
(`classifyDraftTypes`, `buildTransactionDraft`, etc., cobertos por `draft-generation.test.ts`).
Precisa ser **extraída para uma tabela explícita** neste spec, ex.:

| Documento | Vira | Quando |
| --- | --- | --- |
| Boleto de fornecedor | transação (despesa) | pagamento avulso |
| Conta de consumo (CEMIG) | transação + métrica de consumo | recorrente por fornecedor |
| Fatura de cartão | statement_cycle + itens | ciclo de cartão |
| Comprovante de pagamento de fatura | pagamento de fatura (não nova despesa) | concilia com statement |
| Extrato | múltiplas transações | importação |
| Doc sem valor lançável | documento de apoio | anexo |

> ⚠️ Invariante de domínio (ver [`05-dominio-financeiro.md`](05-dominio-financeiro.md)):
> pagamento de fatura **não** é nova despesa; transferência interna **não** é gasto.

## Gaps e critérios de aceite

| Gap | Critério de aceite | Prioridade |
| --- | --- | --- |
| 🟡 Tela dedicada de revisão de conciliação | UI mostra candidatos com score/sinal e permite "é novo" / "é duplicata de X" / "anexar a registro existente" | Média |
| ⬜ Matriz documental como contrato testado | Tabela acima vira spec + teste que falha se a classificação divergir | Alta |
| ⬜ Tool MCP `suggest_reconciliation` | Expor conciliação via MCP para uso por agente externo | Baixa |

## Referências de código

- `workers/ingestion/src/reconciliation/reconciliation.ts`
- `apps/web/src/app/api/reconciliation/[draftId]/route.ts` (+ `/progress`)
- `apps/web/src/app/api/ai-suggest/route.ts`
- Testes: `__tests__/integration/domain-flows.test.ts`, `__tests__/domain/draft-generation.test.ts`
- Decisão original: `docs/_arquivo/adrs/ADR-001-deduplicacao-transacao-item-fatura.md`
