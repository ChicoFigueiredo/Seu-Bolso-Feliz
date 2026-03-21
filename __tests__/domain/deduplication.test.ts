import { describe, it, expect } from "vitest";
import {
  deduplicateExpenses,
  sumDeduplicatedExpenses,
  isExpenseType,
  isStatementPayment,
  isTransferType,
  getStatementComposition,
} from "@sbf/domain";
import type { TransactionInput, StatementItemInput } from "@sbf/domain";

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

function makeTx(overrides: Partial<TransactionInput> & { id: string }): TransactionInput {
  return {
    type: "expense",
    amount: 100,
    description: "Test",
    event_date: "2025-03-20",
    ...overrides,
  };
}

function makeSI(overrides: Partial<StatementItemInput> & { id: string }): StatementItemInput {
  return {
    amount: 100,
    description: "Test",
    transaction_date: "2025-03-20",
    ...overrides,
  };
}

describe("Deduplication — ADR-001", () => {
  // ── Regra 1: Pagamento de fatura NÃO gera nova despesa ──────────

  describe("Regra crítica 1: statement_payment nunca conta como despesa", () => {
    it("T-DEDUP-03: statement_payment NUNCA aparece na soma de despesas", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "statement_payment", amount: 500 }),
        makeTx({ id: "tx-2", type: "expense", amount: 100 }),
      ];

      const result = deduplicateExpenses(transactions, []);

      expect(result).toHaveLength(1);
      expect(result[0]!.id).toBe("tx-2");
      expect(result[0]!.amount).toBe(100);
    });

    it("pagamento de fatura excluído mesmo com valor alto", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "statement_payment", amount: 99999 }),
      ];

      const total = sumDeduplicatedExpenses(transactions, []);
      expect(total).toBe(0);
    });
  });

  // ── Regra 2: Transferência NÃO é despesa ────────────────────────

  describe("Regra crítica 2: transferências não contam como gasto", () => {
    it("tipo 'transfer' não é classificado como despesa", () => {
      expect(isTransferType("transfer")).toBe(true);
      expect(isExpenseType("transfer")).toBe(false);
    });
  });

  // ── Regra 5: statement_item com transaction_id → conta APENAS tx ─

  describe("T-DEDUP-01: statement_item vinculado → conta APENAS transaction", () => {
    it("item de fatura com transaction_id não duplica a despesa", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "expense", amount: 150 }),
      ];
      const statementItems: StatementItemInput[] = [
        makeSI({ id: "si-1", transaction_id: "tx-1", amount: 150 }),
      ];

      const result = deduplicateExpenses(transactions, statementItems);

      expect(result).toHaveLength(1);
      expect(result[0]!.sourceType).toBe("transaction");
      expect(result[0]!.id).toBe("tx-1");
    });

    it("T27: mesmo valor não é somado duas vezes", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "expense", amount: 200 }),
      ];
      const statementItems: StatementItemInput[] = [
        makeSI({ id: "si-1", transaction_id: "tx-1", amount: 200 }),
      ];

      const total = sumDeduplicatedExpenses(transactions, statementItems);
      expect(total).toBe(200); // Não 400!
    });
  });

  // ── Regra 6: statement_item sem transaction_id → conta o SI ──────

  describe("T-DEDUP-02: statement_item sem vinculo → conta o statement_item", () => {
    it("item de fatura sem transação vinculada entra na soma", () => {
      const statementItems: StatementItemInput[] = [
        makeSI({ id: "si-1", transaction_id: null, amount: 75 }),
      ];

      const result = deduplicateExpenses([], statementItems);

      expect(result).toHaveLength(1);
      expect(result[0]!.sourceType).toBe("statement_item");
      expect(result[0]!.amount).toBe(75);
    });
  });

  // ── T-DEDUP-04: cenário misto ────────────────────────────────────

  describe("T-DEDUP-04: cenário misto vinculados + não vinculados", () => {
    it("soma correta com mix de itens vinculados e não vinculados", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "expense", amount: 100 }),
        makeTx({ id: "tx-2", type: "expense", amount: 200 }),
        makeTx({ id: "tx-3", type: "statement_payment", amount: 500 }),
      ];
      const statementItems: StatementItemInput[] = [
        makeSI({ id: "si-1", transaction_id: "tx-1", amount: 100 }), // vinculado → tx-1
        makeSI({ id: "si-2", transaction_id: null, amount: 50 }), // não vinculado → conta
        makeSI({ id: "si-3", transaction_id: "tx-2", amount: 200 }), // vinculado → tx-2
        makeSI({ id: "si-4", transaction_id: null, amount: 30 }), // não vinculado → conta
      ];

      const total = sumDeduplicatedExpenses(transactions, statementItems);

      // tx-1:100 + tx-2:200 + si-2:50 + si-4:30 = 380
      // tx-3 (statement_payment) excluído
      // si-1 e si-3 excluídos (vinculados)
      expect(total).toBe(380);
    });

    it("resultado contém items corretos separados por sourceType", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "expense", amount: 100 }),
        makeTx({ id: "tx-2", type: "fee", amount: 10 }),
      ];
      const statementItems: StatementItemInput[] = [
        makeSI({ id: "si-1", transaction_id: "tx-1", amount: 100 }),
        makeSI({ id: "si-2", transaction_id: null, amount: 50 }),
      ];

      const result = deduplicateExpenses(transactions, statementItems);

      const txItems = result.filter((r) => r.sourceType === "transaction");
      const siItems = result.filter((r) => r.sourceType === "statement_item");

      expect(txItems).toHaveLength(2); // tx-1 (expense) + tx-2 (fee)
      expect(siItems).toHaveLength(1); // si-2 (não vinculado)
    });
  });

  // ── Tipos de despesa ─────────────────────────────────────────────

  describe("classificação de tipos", () => {
    it("expense, fee e interest_charge são tipos de despesa", () => {
      expect(isExpenseType("expense")).toBe(true);
      expect(isExpenseType("fee")).toBe(true);
      expect(isExpenseType("interest_charge")).toBe(true);
    });

    it("statement_payment e refund NÃO são tipos de despesa", () => {
      expect(isExpenseType("statement_payment")).toBe(false);
      expect(isExpenseType("refund")).toBe(false);
    });

    it("receita, transfer e outros não são despesa", () => {
      expect(isExpenseType("income")).toBe(false);
      expect(isExpenseType("transfer")).toBe(false);
      expect(isExpenseType("adjustment")).toBe(false);
    });

    it("isStatementPayment identifica corretamente", () => {
      expect(isStatementPayment("statement_payment")).toBe(true);
      expect(isStatementPayment("expense")).toBe(false);
    });
  });

  // ── Regra 7: Composição da fatura (exceção) ──────────────────────

  describe("Regra 7: relatório de composição da fatura mostra tudo", () => {
    it("getStatementComposition retorna todos os items sem deduplicação", () => {
      const statementItems: StatementItemInput[] = [
        makeSI({ id: "si-1", transaction_id: "tx-1", amount: 100 }),
        makeSI({ id: "si-2", transaction_id: null, amount: 50 }),
        makeSI({ id: "si-3", transaction_id: "tx-2", amount: 200 }),
      ];

      const composition = getStatementComposition(statementItems);

      expect(composition).toHaveLength(3);
    });
  });

  // ── Regra 8: Estornos não distorcem despesa ──────────────────────

  describe("Regra crítica 8: estornos e ajustes não distorcem receita/despesa", () => {
    it("refund é excluído da soma de despesas", () => {
      const transactions: TransactionInput[] = [
        makeTx({ id: "tx-1", type: "expense", amount: 100 }),
        makeTx({ id: "tx-2", type: "refund", amount: 100 }),
      ];

      const total = sumDeduplicatedExpenses(transactions, []);
      expect(total).toBe(100); // Refund não entra
    });
  });
});
