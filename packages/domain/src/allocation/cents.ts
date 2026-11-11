// @sbf/domain — Aritmética de centavos para rateio
//
// Todo cálculo de rateio acontece em centavos inteiros. Somar reais em ponto
// flutuante perde centavo de formas silenciosas e assimétricas (0.1 + 0.2 ===
// 0.30000000000000004; 0.29 * 100 === 28.999999999999996), e o rateio é
// exatamente o lugar onde um centavo perdido vira "a soma dos rateios não
// bate com a movimentação".

/** Maior valor absoluto, em centavos, que cabe em `numeric(14,2)`. */
const MAX_CENTS = 99_999_999_999_999;

function requireFinite(value: number, label: string): void {
  if (!Number.isFinite(value)) {
    throw new RangeError(`${label} precisa ser um número finito (recebido: ${String(value)})`);
  }
}

/**
 * Converte reais em centavos inteiros, arredondando meio centavo para longe
 * do zero.
 *
 * O arredondamento é feito sobre a representação decimal do produto
 * (`toPrecision(15)`) e não sobre o produto cru: `8.165 * 100` vale
 * `816.4999999999999` em IEEE-754, e um `Math.round` ingênuo devolveria 816
 * em vez de 817.
 */
export function toCents(amount: number): number {
  requireFinite(amount, "Valor");
  const scaled = Number((amount * 100).toPrecision(15));
  const rounded = Math.sign(scaled) * Math.round(Math.abs(scaled));
  if (Math.abs(rounded) > MAX_CENTS) {
    throw new RangeError(`Valor fora do limite suportado (recebido: ${String(amount)})`);
  }
  // Normaliza -0 para 0: `Object.is(-0, 0)` é falso e vazaria para a UI.
  return rounded === 0 ? 0 : rounded;
}

/** Converte centavos inteiros de volta para reais. */
export function fromCents(cents: number): number {
  requireFinite(cents, "Centavos");
  if (!Number.isInteger(cents)) {
    throw new RangeError(`Centavos precisa ser inteiro (recebido: ${String(cents)})`);
  }
  const reais = cents / 100;
  return reais === 0 ? 0 : reais;
}

/** Soma uma lista de valores em reais sem acumular erro de ponto flutuante. */
export function sumCents(amounts: readonly number[]): number {
  let total = 0;
  for (const amount of amounts) total += toCents(amount);
  return total;
}
