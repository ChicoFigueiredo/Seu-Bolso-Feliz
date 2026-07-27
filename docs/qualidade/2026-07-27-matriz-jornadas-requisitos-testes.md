# Matriz de rastreabilidade — jornadas × requisitos × testes

> Entregável §16.5 do plano mestre. Data: 2026-07-27.
>
> **Regra:** uma linha só recebe status ✅ com **duas** evidências — um teste que passa
> **e** um chamador em produção. Linha sem teste citado não pode ser marcada como pronta.

## Legenda

✅ pronto e provado · 🟡 parcial · ⬜ não construído · 🔒 depende do CEO

## Marco 1 — Um documento fecha o ciclo (§18)

| #    | Requisito                                 | Componente                                 | Teste                                          | Evidência                                  | Ambiente       | Status |
| ---- | ----------------------------------------- | ------------------------------------------ | ---------------------------------------------- | ------------------------------------------ | -------------- | ------ |
| 1.1  | Ingere documento real                     | `local-scanner/scanner.ts`                 | `e2e/document-to-transaction.test.ts`          | PDF real descoberto e enfileirado          | Supabase local | ✅     |
| 1.2  | Extrai texto de PDF                       | `parsers/text-extractor.ts`                | idem                                           | `pdf_native`, 1 página, texto íntegro      | local          | ✅     |
| 1.3  | Identifica fornecedor                     | `parsers/*`, `field-consensus`             | `__tests__/domain/parsers.test.ts`             | `extraction_results` populado              | local          | 🟡     |
| 1.4  | Cria obrigação canônica                   | `obligations/obligation-writer.ts`         | `integration/obligations.test.ts` (10)         | 1 obrigação, N evidências                  | local          | ✅     |
| 1.5  | Permite correção                          | `draft-review-form.tsx`, `updateDraftData` | e2e (passo 5)                                  | draft → `corrected`                        | local          | ✅     |
| 1.6  | Aprova **sem** lançar                     | `actions/ingestion.ts`                     | e2e (passo 6)                                  | `approved` com `posted_record_id` NULL     | local          | ✅     |
| 1.7  | Materializa **uma única vez**             | `fn_materialize_draft_record`              | `integration/materialization-rpc.test.ts` (13) | `Promise.all` → 1 posted, 2 already_posted | local          | ✅     |
| 1.8  | `pending_review` não vira dinheiro        | mesma RPC                                  | e2e (passo 4) + integração                     | `check_violation`, 0 transações            | local          | ✅     |
| 1.9  | Duplicata cross-canal não duplica despesa | identity keys + `content_hash`             | e2e (teste 2)                                  | mesma obrigação, 1 transação               | local          | ✅     |
| 1.10 | Auditoria da materialização               | RPC, mesma transação                       | `materialization-rpc.test.ts`                  | linha `draft_materialized`                 | local          | ✅     |
| 1.11 | Atualiza agenda e relatório               | —                                          | —                                              | —                                          | —              | ⬜     |

> **1.11 é o elo que falta para o Marco 1 estar 100%.** Não existe agenda consolidada:
> a home mostra três listas separadas e `liability_installments` não aparece em lugar
> nenhum. Rastreado como E11 no backlog.

## Marco 2 — Fatura de cartão utilizável

| #    | Requisito                     | Componente                           | Teste                             | Status                      |
| ---- | ----------------------------- | ------------------------------------ | --------------------------------- | --------------------------- |
| 2.1  | Resolve a senha com segurança | `fn_get_secrets`, `secret-lookup.ts` | `integration/secrets.test.ts` (8) | ✅                          |
| 2.2  | Senha nunca em log            | idem                                 | teste com sentinela               | ✅                          |
| 2.3  | Perfis de senha por escopo    | —                                    | —                                 | ⬜ (F6.2)                   |
| 2.4  | Identifica o cartão           | —                                    | —                                 | ⬜ (F6.1 — rota não existe) |
| 2.5  | Extrai ciclo e vencimento     | —                                    | —                                 | ⬜ (I6.3.1)                 |
| 2.6  | Extrai itens                  | —                                    | —                                 | ⬜                          |
| 2.7  | Identifica parcelas           | —                                    | —                                 | ⬜ (I6.3.2)                 |
| 2.8  | Projeta parcelas futuras      | —                                    | —                                 | ⬜ (I6.3.3)                 |
| 2.9  | Calcula limite comprometido   | —                                    | —                                 | ⬜ (I6.3.3)                 |
| 2.10 | Reconcilia o pagamento        | —                                    | —                                 | ⬜                          |

## Marco 3 — Backfill supervisionado

| #   | Requisito                                | Componente                            | Teste                                           | Status                                           |
| --- | ---------------------------------------- | ------------------------------------- | ----------------------------------------------- | ------------------------------------------------ |
| 3.1 | Gmail é paginado                         | `gmail-scanner/index.ts`              | —                                               | ✅ (já existia)                                  |
| 3.2 | Query e período funcionam                | `query-builder.ts`, `gmail-client.ts` | `query-builder.test.ts`, `gmail-client.test.ts` | ✅                                               |
| 3.3 | Flags chegam ao destino                  | `workers/cli/args.ts`                 | `args.test.ts` — toda flag da ajuda é aceita    | ✅                                               |
| 3.4 | Há checkpoint                            | tabela `ingestion_checkpoints`        | —                                               | 🟡 tabela criada, escrita pendente (I4.1)        |
| 3.5 | Pausa e retoma                           | —                                     | —                                               | ⬜ (I4.2)                                        |
| 3.6 | Duplicatas não viram despesas duplicadas | identity keys                         | `obligations.test.ts` + e2e                     | ✅                                               |
| 3.7 | Erros isolados                           | `runPipeline`                         | —                                               | 🟡 fonte que falha não impede a outra; sem teste |
| 3.8 | Progresso aparece na web                 | —                                     | —                                               | ⬜ (I4.3)                                        |
| 3.9 | Nenhuma materialização sem revisão       | RPC                                   | `materialization-rpc.test.ts`                   | ✅                                               |

## Marco 4 — Planejamento financeiro

Nenhum requisito construído. Os motores de amortização existem e são testados, mas
**nenhuma tela os chama** — ver E11.

| #   | Requisito                     | Status                          |
| --- | ----------------------------- | ------------------------------- |
| 4.1 | Próximos pagamentos           | ⬜                              |
| 4.2 | Saldo projetado               | ⬜                              |
| 4.3 | Cartões e limites futuros     | ⬜                              |
| 4.4 | Dívidas com estratégia        | 🟡 motor pronto, sem consumidor |
| 4.5 | Cenário de receita adicional  | ⬜                              |
| 4.6 | Data projetada de dívida zero | ⬜                              |

## Requisitos transversais

| Requisito                                             | Componente                             | Teste                                      | Status |
| ----------------------------------------------------- | -------------------------------------- | ------------------------------------------ | ------ |
| Schemas compartilhados entre gerador e materializador | `@sbf/contracts`                       | 57 testes                                  | ✅     |
| Conversão explícita entre DTOs                        | `TO_INSERT`                            | `transaction.test.ts`                      | ✅     |
| Transação de banco                                    | RPC                                    | integração                                 | ✅     |
| Chave de materialização                               | `uq_draft_records_materialization_key` | integração                                 | ✅     |
| Controle de concorrência                              | `FOR UPDATE`                           | teste de `Promise.all`                     | ✅     |
| Compensação em caso de falha                          | rollback + `materialization_error`     | integração                                 | ✅     |
| Recorrência nunca de um documento só                  | `classifyDraftTypes`                   | `extraction-to-draft.test.ts`              | ✅     |
| Recorrência por histórico                             | —                                      | —                                          | ⬜     |
| RLS e segregação por usuário                          | policies                               | `materialization-rpc.test.ts` (cross-user) | 🟡     |
| Logs sem dados sensíveis                              | `fn_get_secrets`                       | sentinela                                  | ✅     |
| Retenção e exclusão de originais                      | —                                      | —                                          | ⬜     |
| Identidade de dispositivo                             | —                                      | —                                          | ⬜     |
| Observabilidade                                       | —                                      | —                                          | ⬜     |

## Cobertura de testes por camada (§15)

| Camada        | Estado | Contagem medida                                                            |
| ------------- | ------ | -------------------------------------------------------------------------- |
| Unitários     | ✅     | 433 em 25 arquivos                                                         |
| Contrato      | 🟡     | dentro dos unitários; sem projeto vitest próprio                           |
| Integração    | ✅     | 60 em 6 arquivos                                                           |
| Ponta a ponta | 🟡     | 12 em 2 arquivos — falta fatura protegida, OFX, XLSX, extrato              |
| Goldens       | ⬜     | semente existe em `fixtures/documents/*.expected`; projeto vitest pendente |

## Como manter esta matriz

- Toda issue fechada atualiza a linha correspondente **no mesmo commit**.
- Uma linha sem teste citado **não pode** ser marcada ✅, nem que o código exista.
- Uma linha com teste mas sem chamador em produção é 🟡 — foi exatamente esse o erro que
  fez o domínio financeiro parecer pronto durante meses.
