// @sbf/domain — Deduplication Logic (ADR-001)
//
// Regras de deduplicação de despesas para relatórios:
// 1. Soma APENAS a fonte primária de cada evento
// 2. statement_payment NUNCA entra na soma de despesas
// 3. transfers NUNCA entram na soma de despesas
// 4. recurring_instances com status ≠ 'paid' NÃO entram na soma de gastos realizados
// 5. Se statement_item tem transaction_id preenchido → conta APENAS a transaction
// 6. Se statement_item NÃO tem transaction_id → conta o statement_item
// 7. Exceção: relatório "Composição da Fatura" conta todos os statement_items

export interface ExpenseItem {
  id: string;
  sourceType: "transaction" | "statement_item";
  amount: number;
  description?: string | null;
  eventDate: string;
  supplierId?: string | null;
  categoryId?: string | null;
  priority?: string | null;
}

export interface TransactionInput {
  id: string;
  type: string;
  amount: number;
  description?: string | null;
  event_date: string;
  supplier_id?: string | null;
  category_id?: string | null;
  priority?: string | null;
}

export interface StatementItemInput {
  id: string;
  transaction_id?: string | null;
  amount: number;
  description?: string | null;
  transaction_date?: string | null;
  supplier_id?: string | null;
}

// Tipos de transação que contam como despesa
const EXPENSE_TYPES = new Set(["expense", "fee", "interest_charge"]);

// Tipos que NUNCA contam como despesa
const EXCLUDED_TYPES = new Set(["statement_payment", "refund"]);

/**
 * Deduplica despesas: dado um conjunto de transações e itens de fatura,
 * retorna a lista canônica de despesas sem duplicação.
 *
 * Implementa as regras do ADR-001:
 * - transaction_id preenchido no statement_item → conta APENAS transaction
 * - transaction_id NULL → conta o statement_item
 * - statement_payment e refund são excluídos
 */
export function deduplicateExpenses(
  transactions: TransactionInput[],
  statementItems: StatementItemInput[],
): ExpenseItem[] {
  const result: ExpenseItem[] = [];

  // 1. Adicionar transações de despesa (incluindo as vinculadas a statement_items)
  for (const t of transactions) {
    if (EXCLUDED_TYPES.has(t.type)) continue;
    if (!EXPENSE_TYPES.has(t.type)) continue;

    result.push({
      id: t.id,
      sourceType: "transaction",
      amount: t.amount,
      description: t.description,
      eventDate: t.event_date,
      supplierId: t.supplier_id,
      categoryId: t.category_id,
      priority: t.priority,
    });
  }

  // 2. Adicionar statement_items SEM transação vinculada
  for (const si of statementItems) {
    if (si.transaction_id != null) continue; // já contado via transaction

    result.push({
      id: si.id,
      sourceType: "statement_item",
      amount: si.amount,
      description: si.description,
      eventDate: si.transaction_date ?? "",
      supplierId: si.supplier_id,
    });
  }

  return result;
}

/**
 * Calcula o total de despesas deduplicadas.
 */
export function sumDeduplicatedExpenses(
  transactions: TransactionInput[],
  statementItems: StatementItemInput[],
): number {
  const items = deduplicateExpenses(transactions, statementItems);
  return items.reduce((sum, item) => sum + item.amount, 0);
}

/**
 * Verifica se uma transação é do tipo que conta como despesa.
 */
export function isExpenseType(type: string): boolean {
  return EXPENSE_TYPES.has(type);
}

/**
 * Verifica se uma transação é pagamento de fatura (nunca conta como despesa).
 */
export function isStatementPayment(type: string): boolean {
  return type === "statement_payment";
}

/**
 * Verifica se uma transação é transferência interna (nunca conta como despesa).
 * Transferências são entidades separadas, mas essa função pode ser usada
 * para excluir tipos ambíguos.
 */
export function isTransferType(type: string): boolean {
  return type === "transfer";
}

/**
 * Para o relatório de "Composição da Fatura", retorna TODOS os statement_items
 * do ciclo, sem deduplicação. Este é o caso de exceção da regra 7.
 */
export function getStatementComposition(
  statementItems: StatementItemInput[],
): StatementItemInput[] {
  return [...statementItems];
}
