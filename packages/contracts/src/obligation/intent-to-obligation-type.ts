/**
 * Tradução de intenção financeira → tipo de obrigação.
 *
 * `classifyFinancialIntent` reconhece 10 intenções; `financial_obligations`
 * aceita 6 tipos. Nem toda intenção gera obrigação: um extrato bancário ou um
 * histórico de transações descrevem movimentos passados, não algo a pagar.
 * Para esses, o mapeamento é `null` e o pipeline segue sem criar obrigação.
 */

/** As 10 intenções de `classifyFinancialIntent`. */
export type FinancialIntentName =
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

/** Os 6 valores aceitos pela CHECK de `financial_obligations.obligation_type`. */
export type ObligationType =
  | "bill_to_pay"
  | "bill_reminder"
  | "invoice_statement"
  | "recurring_charge"
  | "liability_installment"
  | "unknown";

/**
 * `satisfies` garante exaustividade: acrescentar uma intenção nova sem decidir
 * o que ela vira quebra o typecheck, em vez de cair silenciosamente em
 * "unknown".
 */
export const INTENT_TO_OBLIGATION_TYPE = {
  bill_to_pay: "bill_to_pay",
  bill_reminder: "bill_reminder",
  invoice_statement: "invoice_statement",
  recurring_charge: "recurring_charge",
  contract_or_debt: "liability_installment",

  // Um comprovante atesta o pagamento de uma obrigação que já existe; ele
  // deve se anexar a ela como evidência, não fundar uma nova.
  payment_receipt: "unknown",

  // Descrevem movimento já ocorrido: não há obrigação a criar.
  bank_statement: null,
  payment_confirmation: null,
  transaction_history: null,

  unknown: "unknown",
} satisfies Record<FinancialIntentName, ObligationType | null>;

/** Devolve o tipo de obrigação, ou null quando a intenção não gera obrigação. */
export function toObligationType(intent: string | null | undefined): ObligationType | null {
  if (!intent) return "unknown";
  const mapped = (INTENT_TO_OBLIGATION_TYPE as Record<string, ObligationType | null>)[intent];
  return mapped === undefined ? "unknown" : mapped;
}

/** Verdadeiro quando a intenção deve produzir uma obrigação canônica. */
export function shouldCreateObligation(intent: string | null | undefined): boolean {
  return toObligationType(intent) !== null;
}
