// @sbf/shared-types — Tipos de domínio derivados do schema do banco
// Estes tipos são aliases ergonômicos para uso no app, derivados de Database.

import type { Database } from "../database.types";

// ── Aliases de tabelas ──
type Tables = Database["public"]["Tables"];
type Views = Database["public"]["Views"];
type Functions = Database["public"]["Functions"];

// ── Helper: Row / Insert / Update ──
export type Row<T extends keyof Tables> = Tables[T]["Row"];
export type Insert<T extends keyof Tables> = Tables[T]["Insert"];
export type Update<T extends keyof Tables> = Tables[T]["Update"];

// ── Entidades principais (Row) ──
export type Institution = Row<"institutions">;
export type FinancialProduct = Row<"financial_products">;
export type Card = Row<"cards">;
export type Category = Row<"categories">;
export type Tag = Row<"tags">;
export type FinancialPeriod = Row<"financial_periods">;
export type Transaction = Row<"transactions">;
export type Transfer = Row<"transfers">;
export type StatementCycle = Row<"statement_cycles">;
export type StatementItem = Row<"statement_items">;
export type RecurringTemplate = Row<"recurring_templates">;
export type RecurringInstance = Row<"recurring_instances">;
export type Liability = Row<"liabilities">;
export type LiabilityInstallment = Row<"liability_installments">;
export type Document = Row<"documents">;
export type UserSecret = Row<"user_secrets">;
export type ImportJob = Row<"import_jobs">;
export type AuditLog = Row<"audit_logs">;
export type UserFinancialPreferences = Row<"user_financial_preferences">;

// ── Fornecedores ──
export type Supplier = Row<"suppliers">;
export type SupplierAlias = Row<"supplier_aliases">;
export type SupplierContract = Row<"supplier_contracts">;
export type ConsumptionMetric = Row<"consumption_metrics">;

// ── Tabelas de junção ──
export type TransactionTag = Row<"transaction_tags">;
export type RecurringTemplateTag = Row<"recurring_template_tags">;
export type LiabilityTag = Row<"liability_tags">;
export type SupplierTag = Row<"supplier_tags">;

// ── View deduplicada (ADR-001) ──
export type DeduplicatedExpense = Views["v_expenses_deduplicated"]["Row"];

// ── Enums de domínio (extraídos dos CHECK constraints) ──
export type InstitutionType = "bank" | "fintech" | "broker" | "other";

export type FinancialProductType =
  | "checking_account"
  | "savings_account"
  | "credit_card"
  | "overdraft"
  | "personal_loan"
  | "mortgage"
  | "investment"
  | "other";

export type TransactionType =
  | "income"
  | "expense"
  | "refund"
  | "adjustment"
  | "interest_charge"
  | "fee"
  | "statement_payment"
  | "liability_payment";

export type TransactionOriginType = "manual" | "import" | "recurring" | "statement_link";

export type Priority = "essential" | "high" | "medium" | "low" | "optional";

export type StatementCycleStatus = "open" | "closed" | "paid" | "partial" | "overdue";

export type LiabilityType =
  | "personal_loan"
  | "mortgage"
  | "overdraft"
  | "installment_plan"
  | "other";

export type LiabilityStatus = "active" | "paid_off" | "renegotiated" | "defaulted";

export type AmortizationSystem = "sac" | "price" | "mixed" | "other" | "none";

export type RateType = "monthly" | "annual";

export type InstallmentStatus = "pending" | "paid" | "partial" | "overdue" | "waived";

export type RecurringInstanceStatus =
  | "pending"
  | "paid"
  | "partial"
  | "skipped"
  | "overdue"
  | "cancelled";

export type RecurringFrequency =
  | "monthly"
  | "weekly"
  | "biweekly"
  | "quarterly"
  | "annual"
  | "custom";

export type DocumentType = "receipt" | "invoice" | "statement" | "contract" | "proof" | "other";

export type ImportSourceType = "csv" | "xlsx" | "manual" | "api";
export type ImportStatus = "pending" | "processing" | "completed" | "failed" | "partial";

export type SupplierType =
  | "company"
  | "individual"
  | "government"
  | "utility"
  | "telecom"
  | "saas"
  | "platform"
  | "other";

export type SupplierAliasType =
  | "former_name"
  | "abbreviation"
  | "trade_name"
  | "billing_name"
  | "other";

export type SupplierContractType =
  | "subscription"
  | "installment"
  | "on_demand"
  | "prepaid"
  | "other";

// ── RPC types ──
export type SearchSupplierResult = Functions["search_suppliers"]["Returns"][number];

// ── Deduplication source types (ADR-001) ──
export type DeduplicatedSourceType = "statement_item" | "transaction";
