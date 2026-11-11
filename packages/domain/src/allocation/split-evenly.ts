import { fromCents, toCents } from "./cents";
import type { AllocationInput } from "./types";

/**
 * Divide o valor de uma movimentação igualmente entre centros, em centavos
 * inteiros. A soma das partes é sempre exatamente o valor recebido: o resto
 * da divisão vai inteiro para o **primeiro** centro da lista.
 *
 * Usa o módulo do valor — receita divide igual a despesa, o sinal fica na
 * movimentação.
 *
 * Lança quando não há um centavo por centro: um rateio de valor zero é
 * proibido pelo domínio (e pelo `check (amount > 0)` da tabela), então
 * dividir R$ 0,02 entre 3 centros não tem resposta correta — é melhor
 * reclamar alto do que devolver um conjunto com menos centros do que o
 * usuário pediu.
 */
export function splitEvenly(amount: number, centerIds: readonly string[]): AllocationInput[] {
  if (centerIds.length === 0) {
    throw new RangeError("Informe ao menos um centro para dividir o valor.");
  }
  if (new Set(centerIds).size !== centerIds.length) {
    throw new RangeError("O mesmo centro aparece mais de uma vez na divisão.");
  }
  if (!Number.isFinite(amount)) {
    throw new RangeError(`Valor da movimentação inválido (recebido: ${String(amount)})`);
  }

  const totalCents = Math.abs(toCents(amount));
  if (totalCents <= 0) {
    throw new RangeError("Não há valor para dividir entre os centros.");
  }
  if (totalCents < centerIds.length) {
    throw new RangeError(
      `Valor insuficiente para dividir entre ${String(centerIds.length)} centros: ` +
        `sobram só ${String(totalCents)} centavo(s).`,
    );
  }

  const share = Math.floor(totalCents / centerIds.length);
  const remainder = totalCents - share * centerIds.length;

  return centerIds.map((financialCenterId, index) => ({
    financialCenterId,
    amount: fromCents(index === 0 ? share + remainder : share),
  }));
}
