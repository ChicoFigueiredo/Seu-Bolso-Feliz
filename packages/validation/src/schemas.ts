import { z } from "zod";
import {
  institutionTypeSchema,
  financialProductTypeSchema,
  transactionTypeSchema,
  originTypeSchema,
  prioritySchema,
  statementCycleStatusSchema,
  liabilityTypeSchema,
  liabilityStatusSchema,
  amortizationSystemSchema,
  rateTypeSchema,
  installmentStatusSchema,
  recurringInstanceStatusSchema,
  recurringFrequencySchema,
  documentTypeSchema,
  supplierTypeSchema,
  supplierAliasTypeSchema,
  supplierContractTypeSchema,
} from "./enums";

// ── Helpers ──
const uuidSchema = z.string().uuid();
const dateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const dayOfMonthSchema = z.number().int().min(1).max(31);
const monetarySchema = z.number().min(0);

// ══════════════════════════════════════════════════════════════
// Instituições
// ══════════════════════════════════════════════════════════════

export const createInstitutionSchema = z.object({
  name: z.string().min(1).max(200),
  type: institutionTypeSchema.default("bank"),
  icon_url: z.string().url().nullish(),
  color: z.string().max(20).nullish(),
  is_active: z.boolean().default(true),
  display_order: z.number().int().nullish(),
});

export const updateInstitutionSchema = createInstitutionSchema.partial();

// ══════════════════════════════════════════════════════════════
// Produtos financeiros
// ══════════════════════════════════════════════════════════════

export const createFinancialProductSchema = z.object({
  institution_id: uuidSchema,
  name: z.string().min(1).max(200),
  type: financialProductTypeSchema,
  current_balance: monetarySchema.nullish(),
  credit_limit: monetarySchema.nullish(),
  is_active: z.boolean().default(true),
  display_order: z.number().int().nullish(),
  metadata: z.record(z.unknown()).nullish(),
});

export const updateFinancialProductSchema = createFinancialProductSchema.partial();

// ══════════════════════════════════════════════════════════════
// Cartões
// ══════════════════════════════════════════════════════════════

export const createCardSchema = z.object({
  financial_product_id: uuidSchema,
  last_four_digits: z.string().length(4).regex(/^\d+$/).nullish(),
  card_brand: z.string().max(50).nullish(),
  is_primary: z.boolean().default(true),
  holder_name: z.string().max(200).nullish(),
  credit_limit: monetarySchema.nullish(),
  closing_day: dayOfMonthSchema.nullish(),
  due_day: dayOfMonthSchema.nullish(),
  is_active: z.boolean().default(true),
});

export const updateCardSchema = createCardSchema.partial();

// ══════════════════════════════════════════════════════════════
// Categorias
// ══════════════════════════════════════════════════════════════

export const createCategorySchema = z.object({
  name: z.string().min(1).max(200),
  parent_id: uuidSchema.nullish(),
  icon: z.string().max(50).nullish(),
  color: z.string().max(20).nullish(),
  display_order: z.number().int().nullish(),
  is_active: z.boolean().default(true),
});

export const updateCategorySchema = createCategorySchema.partial();

// ══════════════════════════════════════════════════════════════
// Tags
// ══════════════════════════════════════════════════════════════

export const createTagSchema = z.object({
  name: z.string().min(1).max(100),
  color: z.string().max(20).nullish(),
  influences_priority: z.boolean().default(false),
  suggested_priority: prioritySchema.nullish(),
});

export const updateTagSchema = createTagSchema.partial();

// ══════════════════════════════════════════════════════════════
// Transações
// ══════════════════════════════════════════════════════════════

export const createTransactionSchema = z.object({
  financial_product_id: uuidSchema,
  type: transactionTypeSchema,
  amount: z.number(),
  description: z.string().max(500).nullish(),
  event_date: dateSchema,
  competence_date: dateSchema.nullish(),
  financial_period_id: uuidSchema.nullish(),
  statement_cycle_id: uuidSchema.nullish(),
  liability_installment_id: uuidSchema.nullish(),
  category_id: uuidSchema.nullish(),
  priority: prioritySchema.nullish(),
  is_confirmed: z.boolean().default(false),
  origin_type: originTypeSchema.default("manual"),
  recurring_instance_id: uuidSchema.nullish(),
  supplier_id: uuidSchema.nullish(),
  notes: z.string().max(2000).nullish(),
  metadata: z.record(z.unknown()).nullish(),
  tag_ids: z.array(uuidSchema).optional(),
});

export const updateTransactionSchema = createTransactionSchema.partial();

// ══════════════════════════════════════════════════════════════
// Transferências internas
// ══════════════════════════════════════════════════════════════

export const createTransferSchema = z
  .object({
    source_product_id: uuidSchema,
    target_product_id: uuidSchema,
    amount: z.number().positive(),
    description: z.string().max(500).nullish(),
    event_date: dateSchema,
    competence_date: dateSchema.nullish(),
    financial_period_id: uuidSchema.nullish(),
    is_confirmed: z.boolean().default(false),
    notes: z.string().max(2000).nullish(),
  })
  .refine((data) => data.source_product_id !== data.target_product_id, {
    message: "Conta de origem e destino não podem ser iguais",
    path: ["target_product_id"],
  });

export const updateTransferSchema = z.object({
  amount: z.number().positive().optional(),
  description: z.string().max(500).nullish(),
  event_date: dateSchema.optional(),
  competence_date: dateSchema.nullish(),
  financial_period_id: uuidSchema.nullish(),
  is_confirmed: z.boolean().optional(),
  notes: z.string().max(2000).nullish(),
});

// ══════════════════════════════════════════════════════════════
// Templates recorrentes
// ══════════════════════════════════════════════════════════════

export const createRecurringTemplateSchema = z.object({
  financial_product_id: uuidSchema.nullish(),
  name: z.string().min(1).max(200),
  type: z.enum(["income", "expense", "liability_payment", "statement_payment"]),
  amount: monetarySchema.nullish(),
  is_variable_amount: z.boolean().default(false),
  frequency: recurringFrequencySchema,
  day_of_month: dayOfMonthSchema.nullish(),
  custom_interval_days: z.number().int().positive().nullish(),
  category_id: uuidSchema.nullish(),
  priority: prioritySchema.nullish(),
  starts_at: dateSchema.nullish(),
  ends_at: dateSchema.nullish(),
  is_active: z.boolean().default(true),
  supplier_id: uuidSchema.nullish(),
  notes: z.string().max(2000).nullish(),
  tag_ids: z.array(uuidSchema).optional(),
});

export const updateRecurringTemplateSchema = createRecurringTemplateSchema.partial();

// ══════════════════════════════════════════════════════════════
// Instâncias de recorrência
// ══════════════════════════════════════════════════════════════

export const updateRecurringInstanceSchema = z.object({
  actual_amount: monetarySchema.nullish(),
  status: recurringInstanceStatusSchema.optional(),
  paid_date: dateSchema.nullish(),
  transaction_id: uuidSchema.nullish(),
  notes: z.string().max(2000).nullish(),
});

// ══════════════════════════════════════════════════════════════
// Ciclos de fatura
// ══════════════════════════════════════════════════════════════

export const createStatementCycleSchema = z.object({
  card_id: uuidSchema,
  reference_month: dateSchema,
  cycle_start_date: dateSchema,
  cycle_end_date: dateSchema,
  due_date: dateSchema,
  total_amount: monetarySchema.default(0),
  paid_amount: monetarySchema.default(0),
  status: statementCycleStatusSchema.default("open"),
});

export const updateStatementCycleSchema = createStatementCycleSchema.partial();

// ══════════════════════════════════════════════════════════════
// Itens de fatura
// ══════════════════════════════════════════════════════════════

export const createStatementItemSchema = z.object({
  statement_cycle_id: uuidSchema,
  transaction_id: uuidSchema.nullish(),
  description: z.string().max(500).nullish(),
  amount: z.number(),
  transaction_date: dateSchema.nullish(),
  installment_number: z.number().int().positive().nullish(),
  total_installments: z.number().int().positive().nullish(),
  supplier_id: uuidSchema.nullish(),
});

export const updateStatementItemSchema = createStatementItemSchema.partial();

// ══════════════════════════════════════════════════════════════
// Dívidas/Passivos
// ══════════════════════════════════════════════════════════════

export const createLiabilitySchema = z.object({
  financial_product_id: uuidSchema,
  name: z.string().min(1).max(200),
  type: liabilityTypeSchema,
  original_amount: z.number().positive(),
  outstanding_balance: z.number(),
  interest_rate: z.number().min(0).nullish(),
  rate_type: rateTypeSchema.nullish(),
  amortization_system: amortizationSystemSchema.nullish(),
  total_installments: z.number().int().positive().nullish(),
  paid_installments: z.number().int().min(0).default(0),
  start_date: dateSchema.nullish(),
  end_date: dateSchema.nullish(),
  status: liabilityStatusSchema.default("active"),
  supplier_id: uuidSchema.nullish(),
  metadata: z.record(z.unknown()).nullish(),
  tag_ids: z.array(uuidSchema).optional(),
});

export const updateLiabilitySchema = createLiabilitySchema.partial();

// ══════════════════════════════════════════════════════════════
// Parcelas de dívida
// ══════════════════════════════════════════════════════════════

export const createLiabilityInstallmentSchema = z.object({
  liability_id: uuidSchema,
  installment_number: z.number().int().positive(),
  due_date: dateSchema,
  total_amount: z.number(),
  principal_amount: z.number().nullish(),
  interest_amount: z.number().nullish(),
  insurance_amount: z.number().nullish(),
  fee_amount: z.number().nullish(),
  paid_amount: monetarySchema.default(0),
  paid_date: dateSchema.nullish(),
  status: installmentStatusSchema.default("pending"),
});

export const updateLiabilityInstallmentSchema = createLiabilityInstallmentSchema.partial();

// ══════════════════════════════════════════════════════════════
// Fornecedores
// ══════════════════════════════════════════════════════════════

export const createSupplierSchema = z.object({
  name: z.string().min(1).max(200),
  type: supplierTypeSchema.default("company"),
  document_number: z.string().max(30).nullish(),
  website: z.string().url().nullish(),
  is_active: z.boolean().default(true),
  metadata: z.record(z.unknown()).nullish(),
});

export const updateSupplierSchema = createSupplierSchema.partial();

export const createSupplierAliasSchema = z.object({
  supplier_id: uuidSchema,
  alias_name: z.string().min(1).max(200),
  alias_type: supplierAliasTypeSchema.default("trade_name"),
  is_active: z.boolean().default(true),
  valid_from: dateSchema.nullish(),
  valid_until: dateSchema.nullish(),
});

export const updateSupplierAliasSchema = createSupplierAliasSchema.partial();

export const createSupplierContractSchema = z.object({
  supplier_id: uuidSchema,
  contract_type: supplierContractTypeSchema,
  description: z.string().max(500).nullish(),
  start_date: dateSchema.nullish(),
  end_date: dateSchema.nullish(),
  monthly_amount: monetarySchema.nullish(),
  is_active: z.boolean().default(true),
  metadata: z.record(z.unknown()).nullish(),
});

export const updateSupplierContractSchema = createSupplierContractSchema.partial();

export const createConsumptionMetricSchema = z
  .object({
    supplier_id: uuidSchema,
    transaction_id: uuidSchema.nullish(),
    document_id: uuidSchema.nullish(),
    reference_date: dateSchema,
    quantity: z.number().positive().nullish(),
    unit: z.string().max(20).nullish(),
    unit_price: z.number().min(0).nullish(),
    metadata: z.record(z.unknown()).nullish(),
  })
  .refine(
    (data) => {
      // ADR-002: quantity+unit OU metadata type=attribute
      const hasQuantity = data.quantity != null;
      const hasUnit = data.unit != null;
      const isAttribute =
        data.metadata != null &&
        typeof data.metadata === "object" &&
        "type" in data.metadata &&
        data.metadata.type === "attribute";

      if (hasQuantity && !hasUnit) return false;
      if (!hasQuantity && !isAttribute) return false;
      return true;
    },
    {
      message:
        "Métrica deve ter quantity+unit ou metadata.type='attribute' (ADR-002: chk_metric_or_attribute)",
    },
  );

// ══════════════════════════════════════════════════════════════
// Documentos
// ══════════════════════════════════════════════════════════════

export const createDocumentSchema = z.object({
  name: z.string().min(1).max(300),
  description: z.string().max(1000).nullish(),
  file_path: z.string().min(1),
  file_type: z.string().max(50).nullish(),
  file_size: z.number().int().positive().nullish(),
  document_type: documentTypeSchema.nullish(),
  entity_type: z.string().max(50).nullish(),
  entity_id: uuidSchema.nullish(),
  version: z.number().int().positive().default(1),
  is_password_protected: z.boolean().default(false),
  supplier_id: uuidSchema.nullish(),
});

export const updateDocumentSchema = createDocumentSchema.partial();

// ══════════════════════════════════════════════════════════════
// Preferências financeiras
// ══════════════════════════════════════════════════════════════

export const updateFinancialPreferencesSchema = z.object({
  financial_cycle_start_day: dayOfMonthSchema.nullish(),
  financial_cycle_anchor_date: dateSchema.nullish(),
  default_currency: z.string().length(3).default("BRL"),
});

// ══════════════════════════════════════════════════════════════
// Períodos financeiros
// ══════════════════════════════════════════════════════════════

export const createFinancialPeriodSchema = z
  .object({
    start_date: dateSchema,
    end_date: dateSchema,
    label: z.string().max(100).nullish(),
    is_current: z.boolean().default(false),
  })
  .refine((data) => data.end_date >= data.start_date, {
    message: "Data final deve ser maior ou igual à data inicial",
    path: ["end_date"],
  });

export const updateFinancialPeriodSchema = z.object({
  start_date: dateSchema.optional(),
  end_date: dateSchema.optional(),
  label: z.string().max(100).nullish(),
  is_current: z.boolean().optional(),
});
