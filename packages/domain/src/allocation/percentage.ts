import { toCents } from "./cents";
import type { AllocationInput } from "./types";

/** Aceita o rateio inteiro (ou qualquer objeto com `amount`) ou só o valor dele. */
export type AllocationAmount = number | (Partial<AllocationInput> & { readonly amount: number });

function amountOf(allocation: AllocationAmount): number {
  return typeof allocation === "number" ? allocation : allocation.amount;
}

/**
 * Percentual que um rateio representa da movimentação, de 0 a 100.
 *
 * **Só para exibição.** O rateio guarda valor, nunca percentual — este número
 * é derivado na hora de mostrar e não deve ser gravado nem usado como entrada
 * de cálculo (arredondar percentuais é justamente o caminho para perder
 * centavo).
 *
 * Usa o módulo do valor da movimentação: receita também tem percentual.
 * Pode passar de 100 quando o rateio excede a movimentação — quem barra isso
 * é `validateAllocations`, não a exibição.
 */
export function percentageOf(allocation: AllocationAmount, transactionAmount: number): number {
  const allocationCents = toCents(amountOf(allocation));
  const transactionCents = Math.abs(toCents(transactionAmount));
  if (transactionCents === 0) return 0;
  const percentual = (allocationCents / transactionCents) * 100;
  const arredondado = Math.round(percentual * 100) / 100;
  return arredondado === 0 ? 0 : arredondado;
}
