import { describe, it, expect } from "vitest";
import {
  getCurrentPeriod,
  generatePeriods,
  findPeriodForDate,
  daysRemainingInPeriod,
  deduplicateExpenses,
  sumDeduplicatedExpenses,
  isExpenseType,
  isStatementPayment,
  isTransferType,
  prioritizeItems,
  deriveEffectivePriority,
  filterByPriority,
  groupByPriority,
  generateSchedule,
  simulateEarlyPayoff,
  totalInterestPaid,
  getOutstandingBalanceAfter,
} from "@sbf/domain";
import type {
  TransactionInput,
  StatementItemInput,
  PrioritizableItem,
  AmortizationConfig,
  FinancialCycleConfig,
} from "@sbf/domain";

// ════════════════════════════════════════════════════════════════
// Testes de integração — fluxos que combinam múltiplos módulos
// do domínio financeiro para validar coerência ponta a ponta.
// ════════════════════════════════════════════════════════════════

describe("Integração: Ciclo financeiro + Priorização", () => {
  const config: FinancialCycleConfig = { startDay: 20 };

  it("prioriza itens dentro do período financeiro atual", () => {
    const period = getCurrentPeriod(config, new Date("2026-04-01"));
    // Período 20/03 → 19/04

    const items: PrioritizableItem[] = [
      {
        id: "1",
        amount: 500,
        priority: "essential",
        dueDate: "2026-04-05",
        description: "Aluguel",
      },
      { id: "2", amount: 100, priority: "low", dueDate: "2026-04-10", description: "Streaming" },
      { id: "3", amount: 300, priority: "high", dueDate: "2026-04-02", description: "Energia" },
      {
        id: "4",
        amount: 200,
        priority: "medium",
        dueDate: "2026-04-25",
        description: "Fora do período",
      },
    ];

    // Item 4 vence fora do período (após 19/04)
    expect(new Date(items[3]!.dueDate!).getTime()).toBeGreaterThan(
      new Date(period.endDate).getTime(),
    );

    const prioritized = prioritizeItems(items, new Date("2026-04-01"));
    // Verificar que todos foram priorizados
    expect(prioritized).toHaveLength(4);
    // Sorting é ascendente pelo sortScore (menor score = mais prioritário)
    const topItem = prioritized[0]!;
    const lastItem = prioritized.at(-1)!;
    expect(["essential", "high"]).toContain(topItem.effectivePriority);
    // Score ascendente: primeiro item tem score menor que o último
    expect(topItem.sortScore).toBeLessThan(lastItem.sortScore);
  });

  it("dias restantes no período coerentes com datas de vencimento", () => {
    const ref = new Date("2026-04-10");
    const remaining = daysRemainingInPeriod(config, ref);
    // Período atual: 20/03 → 19/04, dia 10/04 → ~9 dias restantes
    expect(remaining).toBeGreaterThan(0);
    expect(remaining).toBeLessThanOrEqual(30);

    // Itens essenciais com vencimento dentro dos dias restantes devem ser priorizados
    const items: PrioritizableItem[] = [
      { id: "1", amount: 500, priority: "essential", dueDate: "2026-04-15" },
      { id: "2", amount: 500, priority: "optional", dueDate: "2026-04-12" },
    ];
    const sorted = prioritizeItems(items, ref);
    expect(sorted[0]!.id).toBe("1"); // essential vence dentro do intervalo
  });

  it("gera períodos consecutivos sem sobreposição ou lacuna", () => {
    const periods = generatePeriods(config, 6);
    expect(periods).toHaveLength(6);

    for (let i = 1; i < periods.length; i++) {
      const prevEnd = new Date(periods[i - 1]!.endDate);
      const currStart = new Date(periods[i]!.startDate);
      // O próximo período começa exatamente 1 dia depois do anterior
      const diff = (currStart.getTime() - prevEnd.getTime()) / (1000 * 60 * 60 * 24);
      expect(diff).toBe(1);
    }
  });
});

describe("Integração: Deduplicação + Classificação de tipo", () => {
  it("pagamento de fatura NÃO gera nova despesa (regra crítica #1)", () => {
    const transactions: TransactionInput[] = [
      { id: "t1", type: "expense", amount: 150, event_date: "2026-03-01", description: "Compra A" },
      { id: "t2", type: "expense", amount: 80, event_date: "2026-03-05", description: "Compra B" },
      {
        id: "t3",
        type: "statement_payment",
        amount: 230,
        event_date: "2026-03-15",
        description: "Pag fatura Nubank",
      },
    ];

    // statement_payment não é despesa
    expect(isExpenseType("statement_payment")).toBe(false);
    expect(isStatementPayment("statement_payment")).toBe(true);

    // Apenas despesas reais entram na deduplicação
    const expenseTransactions = transactions.filter((t) => isExpenseType(t.type));
    expect(expenseTransactions).toHaveLength(2);
    expect(expenseTransactions.map((t) => t.id)).toEqual(["t1", "t2"]);
  });

  it("transferência entre contas próprias NÃO é contabilizada como gasto (regra #2)", () => {
    expect(isTransferType("transfer")).toBe(true);
    expect(isExpenseType("transfer")).toBe(false);

    const transactions: TransactionInput[] = [
      { id: "t1", type: "expense", amount: 100, event_date: "2026-03-01" },
      { id: "t2", type: "transfer", amount: 500, event_date: "2026-03-01" },
    ];

    const total = sumDeduplicatedExpenses(
      transactions.filter((t) => isExpenseType(t.type)),
      [],
    );
    expect(total).toBe(100); // Transferência não deve ser contada
  });

  it("deduplicação entre transação e item de fatura evita contagem dupla", () => {
    const transactions: TransactionInput[] = [
      { id: "t1", type: "expense", amount: 150, event_date: "2026-03-01", description: "Netflix" },
      { id: "t2", type: "expense", amount: 80, event_date: "2026-03-05", description: "Spotify" },
    ];

    const statementItems: StatementItemInput[] = [
      { id: "si1", transaction_id: "t1", amount: 150, description: "Netflix" },
      {
        id: "si2",
        transaction_id: null,
        amount: 40,
        description: "Uber",
        transaction_date: "2026-03-08",
      },
    ];

    const deduped = deduplicateExpenses(transactions, statementItems);
    // t1 linkada a si1 → aparece uma vez.
    // t2 sem link → aparece como transação.
    // si2 sem link → aparece como item de fatura.
    expect(deduped.length).toBe(3);

    const totalDeduped = sumDeduplicatedExpenses(transactions, statementItems);
    // 150 (Netflix deduplicated) + 80 (Spotify) + 40 (Uber) = 270
    expect(totalDeduped).toBe(270);
  });

  it("estornos e ajustes não distorcem receita ou despesa (regra #8)", () => {
    expect(isExpenseType("refund")).toBe(false);
    expect(isExpenseType("adjustment")).toBe(false);
    expect(isExpenseType("expense")).toBe(true);
    expect(isExpenseType("fee")).toBe(true);
    expect(isExpenseType("interest_charge")).toBe(true);
  });
});

describe("Integração: Amortização + Priorização de dívidas", () => {
  const loanConfig: AmortizationConfig = {
    system: "price",
    principalAmount: 10000,
    monthlyRate: 0.015,
    totalInstallments: 12,
    monthlyInsurance: 30,
    monthlyFee: 5,
  };

  it("parcelas de dívida geram itens priorizáveis com composição separada", () => {
    const schedule = generateSchedule(loanConfig);
    expect(schedule).toHaveLength(12);

    // Cada parcela tem amortização + juros + seguro + taxa separados
    for (const inst of schedule) {
      expect(inst.principalAmount).toBeGreaterThan(0);
      expect(inst.interestAmount).toBeGreaterThanOrEqual(0);
      expect(inst.insuranceAmount).toBe(30);
      expect(inst.feeAmount).toBe(5);
      expect(inst.totalAmount).toBeCloseTo(
        inst.principalAmount + inst.interestAmount + inst.insuranceAmount + inst.feeAmount,
        2,
      );
    }

    // Converter parcelas em itens priorizáveis
    const items: PrioritizableItem[] = schedule.map((inst, i) => ({
      id: `loan-${inst.installmentNumber}`,
      amount: inst.totalAmount,
      priority: "high" as const,
      dueDate: new Date(2026, 2 + i, 15).toISOString().split("T")[0],
      description: `Parcela ${inst.installmentNumber}/12 — R$${inst.totalAmount.toFixed(2)}`,
    }));

    const prioritized = prioritizeItems(items, new Date("2026-03-01"));
    expect(prioritized).toHaveLength(12);
    // Todos são high priority
    expect(prioritized.every((p) => p.effectivePriority === "high")).toBe(true);
  });

  it("quitação antecipada recalcula corretamente saldo e economia (regra #11)", () => {
    const totalWithoutPayoff = totalInterestPaid(loanConfig);
    const payoff = simulateEarlyPayoff(loanConfig, 6);

    // Saldo devedor após 6 parcelas
    const balance = getOutstandingBalanceAfter(loanConfig, 6);
    expect(payoff.outstandingBalance).toBeCloseTo(balance, 2);

    // Juros economizados devem ser positivos
    expect(payoff.savedInterest).toBeGreaterThan(0);
    expect(payoff.totalInterest).toBeLessThan(totalWithoutPayoff);
  });

  it("SAC vs Price produzem cronogramas distintos com saldo final próximo de zero", () => {
    const sacConfig: AmortizationConfig = { ...loanConfig, system: "sac" };
    const priceConfig: AmortizationConfig = { ...loanConfig, system: "price" };

    const sacSchedule = generateSchedule(sacConfig);
    const priceSchedule = generateSchedule(priceConfig);

    // Saldo final próximo de zero (tolerância de arredondamento: < R$1)
    expect(sacSchedule.at(-1)!.outstandingBalance).toBeLessThan(1);
    expect(priceSchedule.at(-1)!.outstandingBalance).toBeLessThan(1);

    // SAC: amortização constante, parcelas decrescentes
    const sacAmt = sacSchedule.map((i) => i.principalAmount);
    expect(new Set(sacAmt.map((a) => Math.round(a * 100))).size).toBe(1); // todas iguais

    // Price: parcela total (sem seguro/taxa) constante
    const pricePayments = priceSchedule.map((i) => i.principalAmount + i.interestAmount);
    const firstPayment = pricePayments[0]!;
    for (const p of pricePayments) {
      expect(p).toBeCloseTo(firstPayment, 0);
    }
  });
});

describe("Integração: Priorização com tags derivadas", () => {
  it("tags com influencesPriority derivam prioridade correta", () => {
    // Tags são objetos com metadados; deriveEffectivePriority usa influencesPriority + suggestedPriority
    const items: PrioritizableItem[] = [
      {
        id: "1",
        amount: 200,
        description: "Aluguel",
        tags: [
          { name: "essencial", influencesPriority: true, suggestedPriority: "essential" },
          { name: "moradia", influencesPriority: false },
        ],
      },
      {
        id: "2",
        amount: 50,
        tags: [{ name: "diversao", influencesPriority: false }],
        description: "Cinema",
      },
      {
        id: "3",
        amount: 150,
        dueDate: "2026-03-02",
        description: "Energia",
        tags: [{ name: "essencial", influencesPriority: true, suggestedPriority: "high" }],
      },
    ];

    const p1 = deriveEffectivePriority(items[0]!);
    const p2 = deriveEffectivePriority(items[1]!);
    const p3 = deriveEffectivePriority(items[2]!);

    expect(p1).toBe("essential"); // tag com suggestedPriority essential
    expect(p2).toBe("medium"); // sem influência → default medium
    expect(p3).toBe("high"); // tag com suggestedPriority high
  });

  it("filtro por prioridade mínima separa essenciais dos opcionais", () => {
    const items: PrioritizableItem[] = [
      { id: "1", amount: 500, priority: "essential" },
      { id: "2", amount: 200, priority: "high" },
      { id: "3", amount: 100, priority: "low" },
      { id: "4", amount: 50, priority: "optional" },
    ];

    const essentials = filterByPriority(items, "high");
    expect(essentials.length).toBeGreaterThanOrEqual(2);
    expect(essentials.map((i) => i.id)).toContain("1");
    expect(essentials.map((i) => i.id)).toContain("2");
  });

  it("agrupamento por prioridade cobre todas as categorias", () => {
    const items: PrioritizableItem[] = [
      { id: "1", amount: 500, priority: "essential" },
      { id: "2", amount: 200, priority: "high" },
      { id: "3", amount: 100, priority: "medium" },
      { id: "4", amount: 50, priority: "low" },
      { id: "5", amount: 25, priority: "optional" },
    ];

    const grouped = groupByPriority(items);
    expect(grouped.essential).toHaveLength(1);
    expect(grouped.high).toHaveLength(1);
    expect(grouped.medium).toHaveLength(1);
    expect(grouped.low).toHaveLength(1);
    expect(grouped.optional).toHaveLength(1);
  });
});

describe("Integração: Ciclo financeiro + Deduplicação temporal", () => {
  it("lançamentos atribuídos ao período financeiro correto (regra #3)", () => {
    const config: FinancialCycleConfig = { startDay: 20 };

    // Transação dia 25/03 → período 20/03 a 19/04
    const period = findPeriodForDate(config, new Date("2026-03-25"));
    expect(period).not.toBeNull();
    expect(period!.startDate.getDate()).toBe(20);
    expect(period!.startDate.getMonth()).toBe(2); // março = 2
    expect(period!.endDate.getDate()).toBe(19);
    expect(period!.endDate.getMonth()).toBe(3); // abril = 3

    // Transação dia 15/03 → período 20/02 a 19/03
    const period2 = findPeriodForDate(config, new Date("2026-03-15"));
    expect(period2).not.toBeNull();
    expect(period2!.startDate.getDate()).toBe(20);
    expect(period2!.startDate.getMonth()).toBe(1); // fevereiro = 1
    expect(period2!.endDate.getDate()).toBe(19);
    expect(period2!.endDate.getMonth()).toBe(2); // março = 2
  });

  it("relatório por mês civil e por período personalizado podem divergir (regra #12)", () => {
    const config: FinancialCycleConfig = { startDay: 20 };

    // Março civil: 01/03 a 31/03
    const civilStart = new Date("2026-03-01");
    const civilEnd = new Date("2026-03-31");

    // Período financeiro contendo 01/04: 20/03 a 19/04
    const financialPeriod = findPeriodForDate(config, new Date("2026-04-01"));
    expect(financialPeriod).not.toBeNull();

    // Os intervalos divergem — período financeiro não coincide com mês civil
    expect(financialPeriod!.startDate.getDate()).toBe(20);
    expect(financialPeriod!.endDate.getDate()).toBe(19);

    // Uma transação em 25/03 está em AMBOS os intervalos
    const txDate = new Date("2026-03-25");
    const inCivil = txDate >= civilStart && txDate <= civilEnd;
    const periodForTx = findPeriodForDate(config, txDate);
    expect(inCivil).toBe(true);
    expect(periodForTx!.startDate.getDate()).toBe(20);
    expect(periodForTx!.startDate.getMonth()).toBe(2); // março

    // Uma transação em 05/04 está apenas no período financeiro, não no civil
    const txDate2 = new Date("2026-04-05");
    const inCivil2 = txDate2 >= civilStart && txDate2 <= civilEnd;
    const periodForTx2 = findPeriodForDate(config, txDate2);
    expect(inCivil2).toBe(false);
    expect(periodForTx2!.startDate.getDate()).toBe(20);
    expect(periodForTx2!.startDate.getMonth()).toBe(2); // março
  });
});

describe("Integração: Validação + Domínio combinados", () => {
  it("fluxo completo: validar transação → classificar → deduplicar → priorizar", () => {
    // Simula o fluxo desde input bruto até priorização

    // 1. Transações validadas (como se viessem do schema)
    const transactions: TransactionInput[] = [
      {
        id: "t1",
        type: "expense",
        amount: 500,
        event_date: "2026-04-02",
        supplier_id: "s1",
        priority: "essential",
      },
      {
        id: "t2",
        type: "expense",
        amount: 100,
        event_date: "2026-04-05",
        supplier_id: "s2",
        priority: "low",
      },
      { id: "t3", type: "income", amount: 3000, event_date: "2026-04-01" },
      { id: "t4", type: "statement_payment", amount: 600, event_date: "2026-04-10" },
      { id: "t5", type: "transfer", amount: 1000, event_date: "2026-04-03" },
    ];

    // 2. Classificar — separar tipos
    const expenses = transactions.filter((t) => isExpenseType(t.type));
    const payments = transactions.filter((t) => isStatementPayment(t.type));
    const transfers = transactions.filter((t) => isTransferType(t.type));
    const income = transactions.filter((t) => t.type === "income");

    expect(expenses).toHaveLength(2);
    expect(payments).toHaveLength(1);
    expect(transfers).toHaveLength(1);
    expect(income).toHaveLength(1);

    // 3. Deduplicar despesas
    const deduped = deduplicateExpenses(expenses, []);
    expect(deduped).toHaveLength(2);
    const totalExpense = sumDeduplicatedExpenses(expenses, []);
    expect(totalExpense).toBe(600);

    // 4. Priorizar
    const prioritizable: PrioritizableItem[] = deduped.map((e) => ({
      id: e.id,
      amount: e.amount,
      priority: e.priority as "essential" | "high" | "medium" | "low" | "optional" | undefined,
      dueDate: e.eventDate,
      description: e.description,
    }));

    const prioritized = prioritizeItems(prioritizable, new Date("2026-04-01"));
    expect(prioritized[0]!.effectivePriority).toBe("essential");
    expect(prioritized[0]!.amount).toBe(500);
  });
});
