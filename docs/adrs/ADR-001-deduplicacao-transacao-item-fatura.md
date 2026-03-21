# ADR-001: Estratégia de Deduplicação — Transação vs Item de Fatura

**Status:** Aprovado
**Data:** 2026-03-21
**Origem:** Parecer Verônica — Ajuste Obrigatório 4.1
**Ata de referência:** `docs/refinos/2026-03/2026-03-21-13-30-checkpoint-pre-implementacao-ajustes-obrigatorios.md` (Seção 2)

---

## 1. Contexto

Uma compra no cartão de crédito pode existir em múltiplas formas no sistema:

- Como `statement_item` na fatura do cartão
- Como `transaction` manual registrada pelo usuário
- Como registro importado de extrato CSV

Sem regras claras, relatórios podem contar a mesma despesa 2-3 vezes, destruindo a confiabilidade do sistema.

A regra anterior (R27) usava heurística: `mesmo supplier_id + valor + data ± 3 dias + competência = duplicata`. Isso é frágil, sujeito a falsos positivos/negativos, e inadequado para o núcleo determinístico.

## 2. Decisão

### 2.1. Vínculo Explícito via FK

O campo `statement_items.transaction_id` é o **único mecanismo de conciliação** entre item de fatura e transação. É um vínculo 1:1 (cada item pode ter no máximo uma transação, e cada transação estar vinculada a no máximo um item).

```sql
CREATE UNIQUE INDEX idx_statement_items_transaction_id
  ON statement_items(transaction_id) WHERE transaction_id IS NOT NULL;
```

### 2.2. Campo origin_type em transactions

```sql
ALTER TABLE transactions ADD COLUMN origin_type text DEFAULT 'manual'
  CHECK (origin_type IN ('manual', 'import', 'recurring', 'statement_link'));
```

| Valor            | Significado                                  |
| ---------------- | -------------------------------------------- |
| `manual`         | Usuário digitou manualmente                  |
| `import`         | Importação de CSV/XLSX                       |
| `recurring`      | Gerado por recorrência (recurring_instances) |
| `statement_link` | Gerado a partir de statement_item            |

### 2.3. Regra de Precedência (Fonte Primária)

| Cenário                                     | Fonte primária para relatórios | Motivo                                              |
| ------------------------------------------- | ------------------------------ | --------------------------------------------------- |
| Só statement_item (sem transação vinculada) | statement_item                 | Única observação                                    |
| statement_item + transação vinculada        | transação                      | Mais metadados (categoria, tags, fornecedor, notas) |
| Só transação manual (sem item de fatura)    | transação                      | Única observação                                    |
| Importação detecta match com statement_item | statement_item existente       | Evita duplicação                                    |
| recurring_instance confirmada → transação   | transação gerada               | Instância é expectativa, transação é fato           |

### 2.4. Regra de Soma em Relatórios

```
1. Para cada evento real, soma-se APENAS a fonte primária
2. statement_payment NUNCA entra na soma de despesas (é quitação)
3. transfers NUNCA entram na soma de despesas (são movimentações internas)
4. recurring_instances com status ≠ 'paid' NÃO entram na soma de gastos realizados
5. Se statement_item tem transaction_id preenchido → conta APENAS a transaction
6. Se statement_item NÃO tem transaction_id → conta o statement_item
7. Exceção: relatório "Composição da Fatura" — conta todos os statement_items (é visão da fatura)
```

### 2.5. Regra de Exibição

| Tela                     | O que mostrar                                                   |
| ------------------------ | --------------------------------------------------------------- |
| Relatório de despesas    | Fonte primária de cada evento (sem duplicatas)                  |
| Fatura do cartão         | Todos os statement_items (é a visão da fatura)                  |
| Extrato da conta         | Todas as transactions da conta (incluindo statement_payment)    |
| Relatório por fornecedor | Fonte primária de cada evento, agrupada por supplier_id         |
| Detalhes do lançamento   | Fonte primária + indicador visual se tem observações vinculadas |

### 2.6. View de Deduplicação

```sql
CREATE VIEW v_expenses_deduplicated AS
-- Caso 1: statement_items SEM transação vinculada (a despesa é o item)
SELECT
  si.id AS canonical_id,
  'statement_item' AS source_type,
  si.user_id,
  si.amount,
  si.description,
  si.supplier_id,
  NULL::uuid AS category_id,
  NULL::text AS priority,
  si.transaction_date AS event_date,
  NULL::date AS competence_date,
  NULL::uuid AS financial_period_id,
  si.statement_cycle_id
FROM statement_items si
WHERE si.transaction_id IS NULL

UNION ALL

-- Caso 2: transactions que são despesas (inclui as vinculadas a statement_items)
SELECT
  t.id AS canonical_id,
  'transaction' AS source_type,
  t.user_id,
  t.amount,
  t.description,
  t.supplier_id,
  t.category_id,
  t.priority,
  t.event_date,
  t.competence_date,
  t.financial_period_id,
  t.statement_cycle_id
FROM transactions t
WHERE t.type IN ('expense', 'fee', 'interest_charge')
  -- Excluir transações que NÃO são despesas reais
  AND t.type NOT IN ('statement_payment', 'refund');
```

**Nota:** A view usa UNION ALL de dois conjuntos mutuamente exclusivos:

- statement_items sem transação (onde `transaction_id IS NULL`)
- transactions de despesa (que inclui as vinculadas via `statement_items.transaction_id`)

Não há duplicação porque quando um statement_item tem transaction vinculada, só a segunda parte do UNION emite o registro.

## 3. Exemplos

### 3.1. Compra no cartão sem lançamento manual

```
Ação: Usuário compra no Supermercado X com Nubank
Resultado: statement_item criado → transaction_id = NULL
Relatório: conta R$ 350 via statement_item
Pagamento fatura: transaction type=statement_payment → NÃO soma como despesa
```

### 3.2. Lançamento manual vinculado à fatura

```
Ação: Usuário lança "Supermercado X — R$ 350" antes da fatura fechar
Resultado: transaction criada → transaction_id preenchido
Fatura chega: statement_item.transaction_id = id da transação
Relatório: conta R$ 350 via transaction (tem categoria, tags, notas)
Fatura: mostra statement_item (é visão da fatura)
```

### 3.3. Parcelas: visão do total vs item mensal

```
Ação: TV R$ 3.000 em 10x no Nubank
Cada fatura: statement_item "LOJA TV R$ 300 (3/10)"
Transação: PODE existir transação-mãe de R$ 3.000 com parcelas filhas
Vínculo: statement_item de cada parcela vincula à instância da parcela
Relatório mensal: R$ 300 (parcela do mês)
Relatório total da dívida: R$ 3.000 (transação-mãe)
```

### 3.4. Estorno

```
statement_item: "LOJA X R$ 200"
statement_item: "ESTORNO LOJA X -R$ 200"
Ambos na fatura, sem vínculo entre si (são itens distintos)
Relatório: soma líquida R$ 0
```

### 3.5. Débito automático (conta de luz)

```
Ação: Conta de energia debitada automaticamente
Resultado: transaction type=expense R$ 280 (sem statement_item)
Relatório: conta R$ 280 via transaction
```

## 4. Consequências

### Positivas

- Zero duplicação por design (vínculo explícito, não heurística)
- Regra determinística e auditável
- View SQL materializa a regra — qualquer relatório pode reusar
- Payment de fatura claramente separado de despesa

### Negativas

- O vínculo `statement_item.transaction_id` depende de ação do usuário ou sugestão inteligente (futuro)
- Items de fatura sem transação vinculada terão menos metadados (sem categoria, sem tags)
- A view precisa ser mantida se o schema evoluir

## 5. R27 Revisada

> Relatórios por fornecedor (e por qualquer dimensão) utilizam a fonte primária de cada evento financeiro. Quando um `statement_item` possui `transaction_id` vinculado, a `transaction` é a fonte primária. Quando não possui, o `statement_item` é a fonte primária. Pagamentos de fatura (`statement_payment`) NUNCA são contabilizados como despesa. A view `v_expenses_deduplicated` materializa essa regra.

## 6. Testes Associados

- **T22 (revisado):** report por fornecedor usa `v_expenses_deduplicated`
- **T27 (revisado):** deduplicação usa vínculo explícito, não heurística
- **Novo: T-DEDUP-01:** statement_item com transaction_id → relatório conta APENAS transaction
- **Novo: T-DEDUP-02:** statement_item sem transaction_id → relatório conta statement_item
- **Novo: T-DEDUP-03:** statement_payment NUNCA aparece em soma de despesas
- **Novo: T-DEDUP-04:** soma da view == soma esperada para cenário com mix de vinculados e não vinculados

## 7. Etapa de Implementação

**Etapa 1** (Base Estrutural): UNIQUE INDEX, campo origin_type
**Etapa 4** (Relatórios e Filtros): View `v_expenses_deduplicated`, integração em relatórios
