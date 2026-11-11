// Qual lado do centro uma movimentação alimenta, e quais movimentações não
// aceitam rateio. Fica no domínio para que a UI, as Server Actions e o SQL de
// agregação usem exatamente a mesma regra.

/**
 * Tipos de movimentação que entram no centro. Todo o resto sai.
 *
 * Exportado como lista para o SQL de agregação montar o `CASE` a partir dela
 * em vez de repetir os literais.
 */
export const INFLOW_TRANSACTION_TYPES = ["income", "refund"] as const;

/**
 * Tipos que não aceitam rateio.
 *
 * `statement_payment` (pagar a fatura do cartão) é o único: as compras dentro
 * da fatura já foram rateadas uma a uma, e ratear também o pagamento contaria
 * o mesmo dinheiro duas vezes no centro.
 */
export const NON_ALLOCATABLE_TRANSACTION_TYPES = ["statement_payment"] as const;

export type AllocationDirection = "inflow" | "outflow";

const INFLOW = new Set<string>(INFLOW_TRANSACTION_TYPES);
const NON_ALLOCATABLE = new Set<string>(NON_ALLOCATABLE_TRANSACTION_TYPES);

/**
 * De que lado do centro a movimentação entra. Tipo desconhecido conta como
 * saída — é o lado conservador: superestima o consumo do projeto em vez de
 * inflar o saldo dele.
 */
export function allocationDirection(transactionType: string): AllocationDirection {
  return INFLOW.has(transactionType) ? "inflow" : "outflow";
}

/** Se a movimentação pode ser rateada entre centros. */
export function canAllocateTransactionType(transactionType: string): boolean {
  return !NON_ALLOCATABLE.has(transactionType);
}
