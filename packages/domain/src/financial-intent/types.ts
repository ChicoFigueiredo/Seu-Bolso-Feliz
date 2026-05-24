/**
 * FinancialIntent definido localmente para evitar dependência circular
 * com @sbf/ingestion-types. O mesmo tipo é re-exportado por ambos.
 */
export type FinancialIntent =
  | "bill_to_pay"
  | "bill_reminder"
  | "invoice_statement"
  | "bank_statement"
  | "payment_receipt"
  | "payment_confirmation"
  | "transaction_history"
  | "contract_or_debt"
  | "recurring_charge"
  | "unknown";
