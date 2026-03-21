import { describe, it, expect } from "vitest";
import {
  createInstitutionSchema,
  updateInstitutionSchema,
  createFinancialProductSchema,
  createTransactionSchema,
  createTransferSchema,
  createRecurringTemplateSchema,
  createStatementCycleSchema,
  createStatementItemSchema,
  createLiabilitySchema,
  createLiabilityInstallmentSchema,
  createDocumentSchema,
  createFinancialPeriodSchema,
  updateFinancialPreferencesSchema,
  createCategorySchema,
  createTagSchema,
  createCardSchema,
} from "@sbf/validation";

const UUID = "00000000-0000-0000-0000-000000000001";
const UUID2 = "00000000-0000-0000-0000-000000000002";

describe("Validation Schemas — Schemas Zod de CRUD", () => {
  // ── Instituições ───────────────────────────────────────────────

  describe("Instituições", () => {
    it("cria instituição válida", () => {
      const result = createInstitutionSchema.safeParse({
        name: "Nubank",
        type: "fintech",
      });
      expect(result.success).toBe(true);
    });

    it("rejeita nome vazio", () => {
      const result = createInstitutionSchema.safeParse({ name: "" });
      expect(result.success).toBe(false);
    });

    it("tipo padrão é bank", () => {
      const data = createInstitutionSchema.parse({ name: "Caixa" });
      expect(data.type).toBe("bank");
    });

    it("update parcial aceita campos isolados", () => {
      const result = updateInstitutionSchema.safeParse({ name: "Novo Nome" });
      expect(result.success).toBe(true);
    });
  });

  // ── Produtos Financeiros ───────────────────────────────────────

  describe("Produtos Financeiros", () => {
    it("cria produto válido", () => {
      const result = createFinancialProductSchema.safeParse({
        institution_id: UUID,
        name: "Conta Corrente",
        type: "checking_account",
      });
      expect(result.success).toBe(true);
    });

    it("rejeita sem institution_id", () => {
      const result = createFinancialProductSchema.safeParse({
        name: "CC",
        type: "checking_account",
      });
      expect(result.success).toBe(false);
    });
  });

  // ── Cartões ────────────────────────────────────────────────────

  describe("Cartões", () => {
    it("cria cartão válido", () => {
      const result = createCardSchema.safeParse({
        financial_product_id: UUID,
        last_four_digits: "1234",
        closing_day: 15,
        due_day: 23,
      });
      expect(result.success).toBe(true);
    });

    it("rejeita dígitos com letras", () => {
      const result = createCardSchema.safeParse({
        financial_product_id: UUID,
        last_four_digits: "12AB",
      });
      expect(result.success).toBe(false);
    });

    it("rejeita dia de fechamento > 31", () => {
      const result = createCardSchema.safeParse({
        financial_product_id: UUID,
        closing_day: 32,
      });
      expect(result.success).toBe(false);
    });
  });

  // ── Categorias e Tags ──────────────────────────────────────────

  describe("Categorias e Tags", () => {
    it("cria categoria válida", () => {
      const result = createCategorySchema.safeParse({ name: "Alimentação" });
      expect(result.success).toBe(true);
    });

    it("cria tag com influência de prioridade", () => {
      const result = createTagSchema.safeParse({
        name: "essencial",
        influences_priority: true,
        suggested_priority: "essential",
      });
      expect(result.success).toBe(true);
    });

    it("rejeita prioridade inválida em tag", () => {
      const result = createTagSchema.safeParse({
        name: "tag",
        suggested_priority: "super_urgente",
      });
      expect(result.success).toBe(false);
    });
  });

  // ── Transações ─────────────────────────────────────────────────

  describe("Transações", () => {
    it("cria transação válida com tags", () => {
      const result = createTransactionSchema.safeParse({
        financial_product_id: UUID,
        type: "expense",
        amount: 150.5,
        event_date: "2025-03-20",
        tag_ids: [UUID, UUID2],
      });
      expect(result.success).toBe(true);
    });

    it("rejeita tipo de transação inválido", () => {
      const result = createTransactionSchema.safeParse({
        financial_product_id: UUID,
        type: "compra",
        amount: 100,
        event_date: "2025-03-20",
      });
      expect(result.success).toBe(false);
    });

    it("rejeita data em formato errado", () => {
      const result = createTransactionSchema.safeParse({
        financial_product_id: UUID,
        type: "expense",
        amount: 100,
        event_date: "20/03/2025",
      });
      expect(result.success).toBe(false);
    });

    it("aceita prioridade em transação", () => {
      const result = createTransactionSchema.safeParse({
        financial_product_id: UUID,
        type: "expense",
        amount: 100,
        event_date: "2025-03-20",
        priority: "essential",
      });
      expect(result.success).toBe(true);
    });
  });

  // ── Transferências ─────────────────────────────────────────────

  describe("Transferências (regra 1: não é nova despesa)", () => {
    it("cria transferência válida", () => {
      const result = createTransferSchema.safeParse({
        source_product_id: UUID,
        target_product_id: UUID2,
        amount: 500,
        event_date: "2025-03-20",
      });
      expect(result.success).toBe(true);
    });

    it("rejeita transferência com origem = destino", () => {
      const result = createTransferSchema.safeParse({
        source_product_id: UUID,
        target_product_id: UUID,
        amount: 500,
        event_date: "2025-03-20",
      });
      expect(result.success).toBe(false);
    });

    it("rejeita valor não positivo", () => {
      const result = createTransferSchema.safeParse({
        source_product_id: UUID,
        target_product_id: UUID2,
        amount: 0,
        event_date: "2025-03-20",
      });
      expect(result.success).toBe(false);
    });
  });

  // ── Recorrências ───────────────────────────────────────────────

  describe("Recorrências (regra 7: geram expectativa, não pagamento)", () => {
    it("cria template recorrente válido", () => {
      const result = createRecurringTemplateSchema.safeParse({
        name: "Internet Vivo",
        type: "expense",
        amount: 149.9,
        frequency: "monthly",
        day_of_month: 15,
      });
      expect(result.success).toBe(true);
    });

    it("aceita supplier_id no template", () => {
      const result = createRecurringTemplateSchema.safeParse({
        name: "Luz CELPE",
        type: "expense",
        frequency: "monthly",
        supplier_id: UUID,
      });
      expect(result.success).toBe(true);
    });

    it("aceita tag_ids em template", () => {
      const result = createRecurringTemplateSchema.safeParse({
        name: "Aluguel",
        type: "expense",
        amount: 2500,
        frequency: "monthly",
        tag_ids: [UUID],
      });
      expect(result.success).toBe(true);
    });
  });

  // ── Ciclos de Fatura ───────────────────────────────────────────

  describe("Ciclos de Fatura", () => {
    it("cria ciclo de fatura válido", () => {
      const result = createStatementCycleSchema.safeParse({
        card_id: UUID,
        reference_month: "2025-03-01",
        cycle_start_date: "2025-02-16",
        cycle_end_date: "2025-03-15",
        due_date: "2025-03-23",
      });
      expect(result.success).toBe(true);
    });
  });

  // ── Itens de Fatura ────────────────────────────────────────────

  describe("Itens de Fatura", () => {
    it("cria item de fatura válido", () => {
      const result = createStatementItemSchema.safeParse({
        statement_cycle_id: UUID,
        description: "Compra iFood",
        amount: 45.9,
        transaction_date: "2025-03-10",
        supplier_id: UUID2,
      });
      expect(result.success).toBe(true);
    });

    it("aceita item com parcelamento", () => {
      const result = createStatementItemSchema.safeParse({
        statement_cycle_id: UUID,
        description: "TV Samsung 3/12",
        amount: 250,
        installment_number: 3,
        total_installments: 12,
      });
      expect(result.success).toBe(true);
    });
  });

  // ── Dívidas ────────────────────────────────────────────────────

  describe("Dívidas / Passivos", () => {
    it("cria dívida válida", () => {
      const result = createLiabilitySchema.safeParse({
        financial_product_id: UUID,
        name: "Empréstimo Pessoal",
        type: "personal_loan",
        original_amount: 50000,
        outstanding_balance: 45000,
        interest_rate: 0.018,
        rate_type: "monthly",
        amortization_system: "price",
        total_installments: 48,
      });
      expect(result.success).toBe(true);
    });

    it("rejeita valor original negativo", () => {
      const result = createLiabilitySchema.safeParse({
        financial_product_id: UUID,
        name: "Dívida",
        type: "personal_loan",
        original_amount: -1000,
        outstanding_balance: 0,
      });
      expect(result.success).toBe(false);
    });
  });

  // ── Parcelas de Dívida ─────────────────────────────────────────

  describe("Parcelas de Dívida (regra 6: separar composição)", () => {
    it("cria parcela com composição detalhada", () => {
      const result = createLiabilityInstallmentSchema.safeParse({
        liability_id: UUID,
        installment_number: 1,
        due_date: "2025-04-01",
        total_amount: 1500,
        principal_amount: 1041.67,
        interest_amount: 450,
        insurance_amount: 5.33,
        fee_amount: 3,
      });
      expect(result.success).toBe(true);
    });
  });

  // ── Documentos ─────────────────────────────────────────────────

  describe("Documentos", () => {
    it("cria documento válido com supplier", () => {
      const result = createDocumentSchema.safeParse({
        name: "Fatura Energia Mar/2025",
        file_path: "documents/2025/03/fatura-energia.pdf",
        document_type: "invoice",
        supplier_id: UUID,
        is_password_protected: true,
      });
      expect(result.success).toBe(true);
    });

    it("T24: associação com documento persiste supplier_id", () => {
      const result = createDocumentSchema.parse({
        name: "Contrato Aluguel",
        file_path: "docs/contrato.pdf",
        supplier_id: UUID,
      });
      expect(result.supplier_id).toBe(UUID);
    });
  });

  // ── Períodos Financeiros ───────────────────────────────────────

  describe("Períodos Financeiros", () => {
    it("cria período com datas válidas", () => {
      const result = createFinancialPeriodSchema.safeParse({
        start_date: "2025-03-20",
        end_date: "2025-04-19",
        label: "Mar/2025",
      });
      expect(result.success).toBe(true);
    });

    it("rejeita período com data final anterior à inicial", () => {
      const result = createFinancialPeriodSchema.safeParse({
        start_date: "2025-04-20",
        end_date: "2025-03-19",
      });
      expect(result.success).toBe(false);
    });
  });

  // ── Preferências Financeiras ───────────────────────────────────

  describe("Preferências Financeiras", () => {
    it("atualiza preferências com ciclo personalizado", () => {
      const result = updateFinancialPreferencesSchema.safeParse({
        financial_cycle_start_day: 20,
        default_currency: "BRL",
      });
      expect(result.success).toBe(true);
    });

    it("rejeita dia de ciclo > 31", () => {
      const result = updateFinancialPreferencesSchema.safeParse({
        financial_cycle_start_day: 32,
      });
      expect(result.success).toBe(false);
    });
  });
});
