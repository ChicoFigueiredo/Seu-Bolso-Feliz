import { fromCents, toCents } from "./cents";
import type { AllocationInput, AllocationIssue, AllocationValidation } from "./types";

/** Formata reais em pt-BR para as mensagens de erro. */
function brl(amount: number): string {
  return amount.toLocaleString("pt-BR", {
    style: "currency",
    currency: "BRL",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

/**
 * Valida o conjunto de rateios de uma movimentação e devolve quanto ficou
 * não alocado.
 *
 * Regras (decididas pelo CEO, ver CONTEXT.md):
 * - Cada rateio guarda **valor**, sempre positivo. O percentual é derivado só
 *   na exibição (`percentageOf`).
 * - A soma dos rateios **pode ser menor** que o valor da movimentação: o
 *   restante é "não alocado", um estado legítimo. Nunca pode ser maior.
 * - Receita rateia igual a despesa: o sinal vem da movimentação, por isso a
 *   comparação usa o **módulo** de `transactionAmount`.
 * - O mesmo centro não aparece duas vezes no mesmo conjunto (é o que a
 *   unicidade `(transaction_id, financial_center_id)` garante no banco).
 *
 * Nunca lança: devolve todos os problemas encontrados de uma vez, para a
 * interface conseguir mostrar a lista inteira em vez de um erro por vez.
 */
export function validateAllocations(
  transactionAmount: number,
  allocations: readonly AllocationInput[],
): AllocationValidation {
  const issues: AllocationIssue[] = [];

  if (!Number.isFinite(transactionAmount)) {
    return {
      ok: false,
      allocated: 0,
      unallocated: 0,
      issues: [
        {
          code: "movimentacao_invalida",
          message: "O valor da movimentação não é um número válido.",
        },
      ],
    };
  }

  const transactionCents = Math.abs(toCents(transactionAmount));
  const seen = new Set<string>();
  let allocatedCents = 0;

  for (const allocation of allocations) {
    const centerId = allocation.financialCenterId?.trim() ?? "";

    if (centerId === "") {
      issues.push({
        code: "centro_ausente",
        message: "Todo rateio precisa apontar para um centro.",
      });
    } else if (seen.has(centerId)) {
      issues.push({
        code: "centro_repetido",
        message:
          "O mesmo centro aparece mais de uma vez no rateio — junte as partes em um valor só.",
        financialCenterId: centerId,
      });
    } else {
      seen.add(centerId);
    }

    if (!Number.isFinite(allocation.amount)) {
      issues.push({
        code: "valor_invalido",
        message: "O valor do rateio não é um número válido.",
        ...(centerId === "" ? {} : { financialCenterId: centerId }),
      });
      continue;
    }

    const cents = toCents(allocation.amount);
    if (cents <= 0) {
      issues.push({
        code: "valor_nao_positivo",
        message: `O valor do rateio precisa ser maior que zero (recebido: ${brl(allocation.amount)}).`,
        ...(centerId === "" ? {} : { financialCenterId: centerId }),
      });
      continue;
    }

    allocatedCents += cents;
  }

  if (allocatedCents > transactionCents) {
    issues.push({
      code: "soma_excede_movimentacao",
      message:
        `A soma dos rateios (${brl(fromCents(allocatedCents))}) é maior que o valor da ` +
        `movimentação (${brl(fromCents(transactionCents))}).`,
    });
  }

  return {
    ok: issues.length === 0,
    allocated: fromCents(allocatedCents),
    unallocated: fromCents(transactionCents - allocatedCents),
    issues,
  };
}
