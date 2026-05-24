import { describe, it, expect } from "vitest";
import {
  createTransactionSchema,
  createSupplierSchema,
  createSupplierAliasSchema,
  createSupplierContractSchema,
  createRecurringTemplateSchema,
  createLiabilitySchema,
  createStatementCycleSchema,
  createStatementItemSchema,
  createFinancialPeriodSchema,
  updateFinancialPreferencesSchema,
  createInstitutionSchema,
  createFinancialProductSchema,
  createCategorySchema,
  createTagSchema,
  createTransferSchema,
} from "@sbf/validation";
import {
  getCurrentPeriod,
  findPeriodForDate,
  generateSchedule,
  simulateEarlyPayoff,
  deduplicateExpenses,
  sumDeduplicatedExpenses,
  isExpenseType,
  isStatementPayment,
  isTransferType,
  prioritizeItems,
  deriveEffectivePriority,
  groupByPriority,
} from "@sbf/domain";
import type {
  TransactionInput,
  StatementItemInput,
  PrioritizableItem,
  FinancialCycleConfig,
} from "@sbf/domain";

// ════════════════════════════════════════════════════════════════
// E2E — Jornadas completas do usuário, simulando o fluxo desde
// a validação de input até o output final do domínio.
// ════════════════════════════════════════════════════════════════

const UUID = "00000000-0000-0000-0000-000000000001";
const UUID2 = "00000000-0000-0000-0000-000000000002";
const UUID3 = "00000000-0000-0000-0000-000000000003";

describe("E2E: Jornada — Cadastro inicial do usuário", () => {
  it("configura instituição → produto financeiro → preferências → ciclo financeiro", () => {
    // 1. Cadastra instituição
    const inst = createInstitutionSchema.parse({
      name: "Nubank",
      type: "fintech",
    });
    expect(inst.name).toBe("Nubank");
    expect(inst.type).toBe("fintech");

    // 2. Cadastra produto financeiro (cartão de crédito)
    const product = createFinancialProductSchema.parse({
      institution_id: UUID,
      name: "Cartão Nubank",
      type: "credit_card",
    });
    expect(product.type).toBe("credit_card");

    // 3. Configura preferências financeiras
    const prefs = updateFinancialPreferencesSchema.parse({
      financial_cycle_start_day: 20,
      default_currency: "BRL",
    });
    expect(prefs.financial_cycle_start_day).toBe(20);

    // 4. Verifica ciclo financeiro gerado
    const config: FinancialCycleConfig = {
      startDay: prefs.financial_cycle_start_day!,
    };
    const period = getCurrentPeriod(config, new Date("2026-04-01"));
    expect(period.startDate.getDate()).toBe(20);
    expect(period.isCurrent).toBe(true);
  });
});

describe("E2E: Jornada — Lançamento de transação com categorização", () => {
  it("cria categoria → tag → transação → classifica → prioriza", () => {
    // 1. Cria categoria
    const cat = createCategorySchema.parse({
      name: "Moradia",
      color: "#4F46E5",
      icon: "home",
    });
    expect(cat.name).toBe("Moradia");

    // 2. Cria tags
    const tag1 = createTagSchema.parse({ name: "essencial" });
    const tag2 = createTagSchema.parse({ name: "moradia" });
    expect(tag1.name).toBe("essencial");
    expect(tag2.name).toBe("moradia");

    // 3. Cria transação
    const tx = createTransactionSchema.parse({
      financial_product_id: UUID,
      type: "expense",
      amount: 1500,
      event_date: "2026-04-05",
      description: "Aluguel abril",
      category_id: UUID2,
      supplier_id: UUID3,
      priority: "essential",
    });
    expect(tx.type).toBe("expense");
    expect(tx.amount).toBe(1500);
    expect(tx.supplier_id).toBe(UUID3);
    expect(tx.priority).toBe("essential");

    // 4. Classifica e prioriza
    expect(isExpenseType(tx.type)).toBe(true);
    expect(isTransferType(tx.type)).toBe(false);
    expect(isStatementPayment(tx.type)).toBe(false);

    const item: PrioritizableItem = {
      id: "tx-1",
      amount: tx.amount,
      priority: tx.priority as "essential",
      dueDate: tx.event_date,
      description: tx.description,
      tags: [
        { name: "essencial", influencesPriority: true, suggestedPriority: "essential" },
        { name: "moradia", influencesPriority: true, suggestedPriority: "high" },
      ],
    };
    const priority = deriveEffectivePriority(item);
    expect(priority).toBe("essential"); // manual priority takes precedence
  });
});

describe("E2E: Jornada — Fornecedor completo com aliases e contratos", () => {
  it("cria fornecedor → adiciona alias → adiciona contrato → valida tudo", () => {
    // 1. Cria fornecedor
    const supplier = createSupplierSchema.parse({
      name: "Neoenergia Coelba",
      type: "utility",
      document_number: "15.139.629/0001-94",
      trade_name: "Coelba",
    });
    expect(supplier.name).toBe("Neoenergia Coelba");
    expect(supplier.type).toBe("utility");

    // 2. Adiciona alias (nome antigo)
    const alias = createSupplierAliasSchema.parse({
      supplier_id: UUID,
      alias_name: "COELBA - Companhia de Eletricidade do Estado da Bahia",
      alias_type: "former_name",
      valid_from: "2020-01-01",
      valid_until: "2025-12-31",
    });
    expect(alias.alias_type).toBe("former_name");
    expect(alias.alias_name).toContain("COELBA");

    // 3. Adiciona contrato
    const contract = createSupplierContractSchema.parse({
      supplier_id: UUID,
      contract_type: "subscription",
      description: "Residência — Unidade consumidora principal",
      start_date: "2023-06-01",
    });
    expect(contract.description).toContain("Residência");
    expect(contract.contract_type).toBe("subscription");
  });
});

describe("E2E: Jornada — Fatura de cartão sem duplicação de despesa", () => {
  it("compras no cartão → fatura → pagamento → nenhuma despesa duplicada", () => {
    // 1. Valida compras (transações de despesa)
    createTransactionSchema.parse({
      financial_product_id: UUID,
      type: "expense",
      amount: 150,
      event_date: "2026-03-05",
      description: "Supermercado",
    });
    createTransactionSchema.parse({
      financial_product_id: UUID,
      type: "expense",
      amount: 80,
      event_date: "2026-03-10",
      description: "Farmácia",
    });

    // 2. Cria ciclo de fatura
    const statement = createStatementCycleSchema.parse({
      card_id: UUID,
      reference_month: "2026-03-01",
      cycle_start_date: "2026-03-01",
      cycle_end_date: "2026-03-15",
      due_date: "2026-03-23",
      status: "closed",
      total_amount: 230,
    });
    expect(statement.total_amount).toBe(230);

    // 3. Itens da fatura (linkados às transações)
    createStatementItemSchema.parse({
      statement_cycle_id: UUID,
      amount: 150,
      description: "Supermercado",
      transaction_date: "2026-03-05",
    });
    createStatementItemSchema.parse({
      statement_cycle_id: UUID,
      amount: 80,
      description: "Farmácia",
      transaction_date: "2026-03-10",
    });

    // 4. Pagamento da fatura (NÃO é despesa nova — regra #1)
    const pagamento = createTransactionSchema.parse({
      financial_product_id: UUID2,
      type: "statement_payment",
      amount: 230,
      event_date: "2026-03-23",
      description: "Pag. fatura Nubank Mar/26",
    });
    expect(isStatementPayment(pagamento.type)).toBe(true);
    expect(isExpenseType(pagamento.type)).toBe(false);

    // 5. Deduplicação — transações linkadas a itens de fatura não duplicam
    const transactions: TransactionInput[] = [
      {
        id: "t1",
        type: "expense",
        amount: 150,
        event_date: "2026-03-05",
        description: "Supermercado",
      },
      { id: "t2", type: "expense", amount: 80, event_date: "2026-03-10", description: "Farmácia" },
    ];
    const statementItems: StatementItemInput[] = [
      { id: "si1", transaction_id: "t1", amount: 150, description: "Supermercado" },
      { id: "si2", transaction_id: "t2", amount: 80, description: "Farmácia" },
    ];

    const deduped = deduplicateExpenses(transactions, statementItems);
    // Cada compra aparece exatamente uma vez
    expect(deduped).toHaveLength(2);
    const total = sumDeduplicatedExpenses(transactions, statementItems);
    expect(total).toBe(230); // 150 + 80, sem duplicação
  });
});

describe("E2E: Jornada — Transferência interna entre contas", () => {
  it("transferência entre contas próprias não conta como despesa (regra #2)", () => {
    // 1. Valida transferência
    const transfer = createTransferSchema.parse({
      source_product_id: UUID,
      target_product_id: UUID2,
      amount: 1000,
      event_date: "2026-04-01",
      description: "Caixa → Nubank para pagar fatura",
    });
    expect(transfer.amount).toBe(1000);
    expect(transfer.source_product_id).not.toBe(transfer.target_product_id);

    // 2. Classificação correta
    expect(isTransferType("transfer")).toBe(true);
    expect(isExpenseType("transfer")).toBe(false);

    // 3. No relatório, transferências não entram nos totais de despesa
    const transactions: TransactionInput[] = [
      { id: "t1", type: "expense", amount: 200, event_date: "2026-04-01" },
      { id: "t2", type: "transfer", amount: 1000, event_date: "2026-04-01" },
      { id: "t3", type: "income", amount: 3500, event_date: "2026-04-01" },
    ];
    const expenses = transactions.filter((t) => isExpenseType(t.type));
    expect(expenses).toHaveLength(1);
    expect(sumDeduplicatedExpenses(expenses, [])).toBe(200);
  });
});

describe("E2E: Jornada — Empréstimo com amortização e priorização", () => {
  it("cadastra dívida → gera cronograma → prioriza parcelas → simula quitação", () => {
    // 1. Valida cadastro da dívida
    const liability = createLiabilitySchema.parse({
      financial_product_id: UUID,
      name: "Empréstimo pessoal Caixa",
      type: "personal_loan",
      original_amount: 20000,
      outstanding_balance: 18500,
      interest_rate: 1.8,
      rate_type: "monthly",
      amortization_system: "price",
      total_installments: 24,
      paid_installments: 3,
      status: "active",
      start_date: "2026-01-15",
    });
    expect(liability.type).toBe("personal_loan");
    expect(liability.amortization_system).toBe("price");

    // 2. Gera cronograma (apenas para principal restante)
    const schedule = generateSchedule({
      system: "price",
      principalAmount: liability.outstanding_balance!,
      monthlyRate: liability.interest_rate! / 100,
      totalInstallments: liability.total_installments! - liability.paid_installments!,
    });
    expect(schedule).toHaveLength(21); // 24 - 3

    // Cada parcela tem composição separada (regra #6 do domínio)
    const firstInst = schedule[0]!;
    expect(firstInst.principalAmount).toBeGreaterThan(0);
    expect(firstInst.interestAmount).toBeGreaterThan(0);
    expect(firstInst.installmentNumber).toBe(1);
    expect(firstInst.totalAmount).toBeCloseTo(
      firstInst.principalAmount +
        firstInst.interestAmount +
        firstInst.insuranceAmount +
        firstInst.feeAmount,
      2,
    );

    // 3. Prioriza parcelas como itens de pagamento
    const items: PrioritizableItem[] = schedule.slice(0, 3).map((inst, i) => ({
      id: `loan-${inst.installmentNumber}`,
      amount: inst.totalAmount,
      priority: "high" as const,
      dueDate: new Date(2026, 3 + i, 15).toISOString().split("T")[0],
    }));

    const prioritized = prioritizeItems(items, new Date("2026-04-01"));
    expect(prioritized).toHaveLength(3);
    expect(prioritized[0]!.effectivePriority).toBe("high");

    // 4. Simula quitação antecipada (regra #11)
    const payoff = simulateEarlyPayoff(
      {
        system: "price",
        principalAmount: liability.outstanding_balance!,
        monthlyRate: liability.interest_rate! / 100,
        totalInstallments: 21,
      },
      10,
    );
    expect(payoff.savedInterest).toBeGreaterThan(0);
    expect(payoff.outstandingBalance).toBeGreaterThan(0);
    expect(payoff.outstandingBalance).toBeLessThan(liability.outstanding_balance!);
  });
});

describe("E2E: Jornada — Recorrência e período financeiro", () => {
  it("cria template recorrente → atribui ao período financeiro correto", () => {
    // 1. Cria template recorrente
    const recurring = createRecurringTemplateSchema.parse({
      financial_product_id: UUID,
      name: "Internet Vivo",
      type: "expense",
      amount: 149.9,
      frequency: "monthly",
      day_of_month: 10,
      supplier_id: UUID2,
      category_id: UUID3,
      priority: "essential",
    });
    expect(recurring.frequency).toBe("monthly");
    expect(recurring.day_of_month).toBe(10);

    // 2. Período financeiro para vencimento dia 10/04
    const config: FinancialCycleConfig = { startDay: 20 };
    const period = findPeriodForDate(config, new Date("2026-04-10"));
    expect(period).not.toBeNull();
    // Dia 10/04 cai no período 20/03 → 19/04
    expect(period!.startDate.getMonth()).toBe(2); // março
    expect(period!.startDate.getDate()).toBe(20);

    // 3. Validar instância de recorrência
    const tx = createTransactionSchema.parse({
      financial_product_id: recurring.financial_product_id,
      type: recurring.type,
      amount: recurring.amount,
      event_date: "2026-04-10",
      description: recurring.name,
      supplier_id: recurring.supplier_id,
      category_id: recurring.category_id,
      priority: recurring.priority,
    });
    expect(tx.amount).toBe(149.9);
    expect(tx.supplier_id).toBe(UUID2);
  });
});

describe("E2E: Jornada — Primeira tela (decisão e ação)", () => {
  it("priorização gera fila de pagamento correta para decisão operacional", () => {
    // Simula dados que alimentariam a primeira tela
    const today = new Date("2026-04-05");
    const config: FinancialCycleConfig = { startDay: 20 };
    const period = getCurrentPeriod(config, today);
    expect(period.isCurrent).toBe(true);

    const items: PrioritizableItem[] = [
      // Essencial, vence amanhã
      {
        id: "1",
        amount: 1500,
        priority: "essential",
        dueDate: "2026-04-06",
        description: "Aluguel",
        isOverdue: false,
      },
      // Alta prioridade, atrasado
      {
        id: "2",
        amount: 300,
        priority: "high",
        dueDate: "2026-04-01",
        description: "Energia",
        isOverdue: true,
      },
      // Média, vence em 10 dias
      {
        id: "3",
        amount: 149.9,
        priority: "medium",
        dueDate: "2026-04-15",
        description: "Internet",
      },
      // Baixa, vence no fim do período
      { id: "4", amount: 50, priority: "low", dueDate: "2026-04-18", description: "Streaming" },
      // Opcional, sem urgência
      {
        id: "5",
        amount: 30,
        priority: "optional",
        dueDate: "2026-04-19",
        description: "App extra",
      },
    ];

    // 1. Priorização retorna lista ordenada
    const prioritized = prioritizeItems(items, today);
    expect(prioritized).toHaveLength(5);

    // 2. Itens atrasados ou essenciais devem estar no topo
    const top3Ids = prioritized.slice(0, 3).map((p) => p.id);
    expect(top3Ids).toContain("1"); // essential
    expect(top3Ids).toContain("2"); // high + overdue

    // 3. Agrupamento por prioridade para painel
    const grouped = groupByPriority(items);
    expect(grouped.essential).toHaveLength(1);
    expect(grouped.high).toHaveLength(1);
    expect(grouped.optional).toHaveLength(1);

    // 4. Total de essenciais + alta prioridade (o que precisa ser pago)
    const mustPay = [...grouped.essential, ...grouped.high];
    const mustPayTotal = mustPay.reduce((s, i) => s + i.amount, 0);
    expect(mustPayTotal).toBe(1800); // 1500 + 300

    // 5. Total geral do período
    const totalPeriod = items.reduce((s, i) => s + i.amount, 0);
    expect(totalPeriod).toBeCloseTo(2029.9, 1);
  });
});

describe("E2E: Jornada — Período financeiro personalizado (regra #16-18)", () => {
  it("ciclo 20/03-19/04: transação dia 25/03 está no período, dia 20/04 não", () => {
    const config: FinancialCycleConfig = { startDay: 20 };

    // Dentro do período 20/03 → 19/04
    const inPeriod = findPeriodForDate(config, new Date("2026-03-25"));
    expect(inPeriod).not.toBeNull();
    expect(inPeriod!.startDate.getMonth()).toBe(2); // março
    expect(inPeriod!.endDate.getMonth()).toBe(3); // abril
    expect(inPeriod!.endDate.getDate()).toBe(19);

    // Fora do período (20/04 → começa novo período 20/04 a 19/05)
    // Usa construtor local para evitar ambiguidade UTC na data-limite
    const nextPeriod = findPeriodForDate(config, new Date(2026, 3, 20));
    expect(nextPeriod).not.toBeNull();
    expect(nextPeriod!.startDate.getDate()).toBe(20);
    expect(nextPeriod!.startDate.getMonth()).toBe(3); // abril
    expect(nextPeriod!.endDate.getMonth()).toBe(4); // maio
  });

  it("cria e valida período financeiro explícito", () => {
    const period = createFinancialPeriodSchema.parse({
      start_date: "2026-03-20",
      end_date: "2026-04-19",
      label: "Mar/Abr 2026",
    });
    expect(period.start_date).toBe("2026-03-20");
    expect(period.end_date).toBe("2026-04-19");

    // Rejeita período inválido (fim antes do início)
    const invalid = createFinancialPeriodSchema.safeParse({
      start_date: "2026-04-19",
      end_date: "2026-03-20",
      label: "Inválido",
    });
    expect(invalid.success).toBe(false);
  });
});
