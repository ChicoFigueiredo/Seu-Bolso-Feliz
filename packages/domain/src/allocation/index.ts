// @sbf/domain — Rateio (allocation)
//
// Divisão do valor de uma movimentação entre centros. Lógica pura, sem I/O.
// Vocabulário em CONTEXT.md: Centro tem saldo, entradas e saídas e é a única
// das três classificações que aceita rateio; Tag não tem valor nem saldo.

export { fromCents, sumCents, toCents } from "./cents";
export { validateAllocations } from "./validate-allocations";
export { splitEvenly } from "./split-evenly";
export { percentageOf } from "./percentage";
export type { AllocationAmount } from "./percentage";
export { centerBalance } from "./center-balance";
export type { CenterBalance, MoneyTotal } from "./center-balance";
export {
  allocationDirection,
  canAllocateTransactionType,
  INFLOW_TRANSACTION_TYPES,
  NON_ALLOCATABLE_TRANSACTION_TYPES,
} from "./transaction-direction";
export type { AllocationDirection } from "./transaction-direction";
export type {
  AllocationInput,
  AllocationIssue,
  AllocationIssueCode,
  AllocationValidation,
} from "./types";
