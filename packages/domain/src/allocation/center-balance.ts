import { fromCents, toCents } from "./cents";

/** Total já somado, ou a lista de valores a somar. */
export type MoneyTotal = number | readonly number[];

export interface CenterBalance {
  /** Quanto entrou no centro, em reais. */
  inflow: number;
  /** Quanto saiu do centro, em reais. */
  outflow: number;
  /** Entradas menos saídas. Pode ser negativo. */
  balance: number;
}

function totalCents(value: MoneyTotal, label: string): number {
  const amounts = typeof value === "number" ? [value] : value;
  let total = 0;
  for (const amount of amounts) {
    const cents = toCents(amount);
    if (cents < 0) {
      throw new RangeError(
        `${label} não pode ser negativa: a direção vem do argumento, não do sinal do valor.`,
      );
    }
    total += cents;
  }
  return total;
}

/**
 * Saldo do centro tratado como conta corrente virtual: entradas menos saídas.
 *
 * Aceita tanto totais já somados (o caso normal, quando a soma veio do SQL)
 * quanto as listas de valores. Soma sempre em centavos.
 *
 * Restrito ao período de um mês, `balance` é o **resultado do mês** do centro
 * (CONTEXT.md: "entrou menos saiu dentro do período").
 */
export function centerBalance(inflows: MoneyTotal, outflows: MoneyTotal): CenterBalance {
  const inflowCents = totalCents(inflows, "Entrada");
  const outflowCents = totalCents(outflows, "Saída");
  return {
    inflow: fromCents(inflowCents),
    outflow: fromCents(outflowCents),
    balance: fromCents(inflowCents - outflowCents),
  };
}
