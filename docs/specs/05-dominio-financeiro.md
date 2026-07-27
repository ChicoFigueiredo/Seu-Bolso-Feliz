# 05 — Spec: Domínio Financeiro

> **Estado: ✅ núcleo determinístico pronto e testado.**
> Verificado contra código em 2026-06-20. Ver [`00-estado-real.md`](00-estado-real.md).

## Objetivo (comportamento desejado)

Substituir planilhas por um modelo de domínio correto: distinguir despesa de transferência
interna, parcela de dívida de gasto comum, previsto de realizado — e operar em **ciclos
financeiros personalizados** (não o mês civil). Núcleo determinístico, isolado da camada de IA.

## Invariantes inegociáveis (contrato de domínio)

1. **Pagar fatura ≠ nova despesa.** É quitação de obrigação + transferência entre contas.
2. **Parcela de dívida ≠ gasto comum.** Separar amortização, juros, seguros, encargos.
3. **Transferência interna ≠ gasto.** Não contabilizar como despesa.
4. **Previsto ≠ realizado.** Recorrência gera expectativa, não marca pagamento automático.
5. **Período financeiro do usuário ≠ mês civil.** Ex.: ciclo 20/03–19/04.

> Estas invariantes têm **testes obrigatórios** (`deduplication.test.ts`, `financial-cycle.test.ts`,
> `amortization.test.ts`, `priority.test.ts`). Testes são contrato — não alterar por conveniência.

## Módulos de domínio (verificado) — `packages/domain/src/`

| Módulo             | Função                                                                                                         | Estado |
| ------------------ | -------------------------------------------------------------------------------------------------------------- | ------ |
| `financial-cycle`  | `getCurrentPeriod`, `generatePeriods`, `findPeriodForDate`, `daysRemainingInPeriod` (startDay 1–31)            | ✅     |
| `amortization`     | `generateSchedule` (SAC/Price/Misto), `getOutstandingBalanceAfter`, `totalInterestPaid`, `simulateEarlyPayoff` | ✅     |
| `deduplication`    | `deduplicateExpenses`, `sumDeduplicatedExpenses`, `getStatementComposition` (ADR-001)                          | ✅     |
| `priority`         | prioridade efetiva de 5 níveis (manual + tags + tipo), score de urgência                                       | ✅     |
| `financial-intent` | classifica texto como transação/recorrência/métrica/passivo                                                    | ✅     |

## Três janelas temporais (devem conviver sem ambiguidade)

- **Mês civil** (01/03–31/03) — filtros tradicionais.
- **Período financeiro do usuário** (20/03–19/04) — `financial-cycle` + tabela `financial_periods`.
- **Ciclo da fatura** (fecha dia 15, vence dia 23) — `statement_cycles`.

Cada transação mapeia: data do evento, competência, período financeiro, ciclo de fatura (quando aplicável).

## Hierarquia de dados (verificado no schema)

`institutions` → `financial_products` → `cards`/contas → eventos.
Tabelas: `transactions`, `statement_cycles`, `liabilities` (+ `liability_installments`),
`recurring_*`, `suppliers` (+ `supplier_aliases`, `supplier_associations`, `mv_supplier_spending`),
`categories`, `tags`, `financial_periods`, `financial_obligations` (+ evidences).

## Telas (verificado) — `apps/web/src/app/dashboard/`

`transactions`, `statements`, `liabilities`, `recurring`, `products`, `institutions`,
`suppliers`, `reports`, `import` (CSV), `settings` (ciclo financeiro, categorias, tags) — todas ✅.

## Gaps e critérios de aceite

| Gap                                         | Critério de aceite                                                                                                      | Prioridade |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | ---------- |
| 🟡 Matriz documento→registro como contrato  | Ver [`02-conciliacao.md`](02-conciliacao.md): tabela explícita + teste                                                  | Alta       |
| ⬜ Primeira tela orientada a decisão        | Tela inicial responde: o que vence primeiro, o que é essencial, quanto o dinheiro precisa durar (não dashboard passivo) | Média      |
| ⬜ Auditoria histórica por fornecedor na UI | Ver gasto/histórico consolidado por fornecedor                                                                          | Baixa      |

## Referências de código

- `packages/domain/src/{financial-cycle,amortization,deduplication,priority,financial-intent}/`
- Testes: `__tests__/domain/{financial-cycle,amortization,deduplication,priority}.test.ts`
- Decisões: `docs/_arquivo/adrs/ADR-001` (dedup), `ADR-002` (métricas de consumo), `ADR-003` (aliases de fornecedor)
