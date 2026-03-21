import { z } from "zod";

// ── Enums ──
export const institutionTypeSchema = z.enum(["bank", "fintech", "broker", "other"]);

export const financialProductTypeSchema = z.enum([
  "checking_account",
  "savings_account",
  "credit_card",
  "overdraft",
  "personal_loan",
  "mortgage",
  "investment",
  "other",
]);

export const transactionTypeSchema = z.enum([
  "income",
  "expense",
  "refund",
  "adjustment",
  "interest_charge",
  "fee",
  "statement_payment",
  "liability_payment",
]);

export const originTypeSchema = z.enum(["manual", "import", "recurring", "statement_link"]);

export const prioritySchema = z.enum(["essential", "high", "medium", "low", "optional"]);

export const statementCycleStatusSchema = z.enum(["open", "closed", "paid", "partial", "overdue"]);

export const liabilityTypeSchema = z.enum([
  "personal_loan",
  "mortgage",
  "overdraft",
  "installment_plan",
  "other",
]);

export const liabilityStatusSchema = z.enum(["active", "paid_off", "renegotiated", "defaulted"]);

export const amortizationSystemSchema = z.enum(["sac", "price", "mixed", "other", "none"]);

export const rateTypeSchema = z.enum(["monthly", "annual"]);

export const installmentStatusSchema = z.enum(["pending", "paid", "partial", "overdue", "waived"]);

export const recurringInstanceStatusSchema = z.enum([
  "pending",
  "paid",
  "partial",
  "skipped",
  "overdue",
  "cancelled",
]);

export const recurringFrequencySchema = z.enum([
  "monthly",
  "weekly",
  "biweekly",
  "quarterly",
  "annual",
  "custom",
]);

export const documentTypeSchema = z.enum([
  "receipt",
  "invoice",
  "statement",
  "contract",
  "proof",
  "other",
]);

export const importSourceTypeSchema = z.enum(["csv", "xlsx", "manual", "api"]);
export const importStatusSchema = z.enum([
  "pending",
  "processing",
  "completed",
  "failed",
  "partial",
]);

export const supplierTypeSchema = z.enum([
  "company",
  "individual",
  "government",
  "utility",
  "telecom",
  "saas",
  "platform",
  "other",
]);

export const supplierAliasTypeSchema = z.enum([
  "former_name",
  "abbreviation",
  "trade_name",
  "billing_name",
  "other",
]);

export const supplierContractTypeSchema = z.enum([
  "subscription",
  "installment",
  "on_demand",
  "prepaid",
  "other",
]);
