/**
 * Intervalo de datas dos relatórios.
 *
 * ANTES: `page.tsx` reimplementava a matemática do ciclo financeiro em dez
 * linhas inline, enquanto `@sbf/domain/financial-cycle` — testado, com o
 * tratamento de mês curto já resolvido — não tinha um único chamador.
 *
 * A cópia inline não era só redundante, era ERRADA em fevereiro: fazia
 * `new Date(ano, mes, dia)` com `dia = 31` e deixava o JavaScript transbordar
 * para 3 de março, produzindo um "período financeiro" que começava depois de
 * terminar. `getCurrentPeriod` limita o dia ao último do mês, que é a regra
 * certa para quem fecha o ciclo no dia 31.
 */
import { getCurrentPeriod } from "@sbf/domain";

export interface IntervaloDeRelatorio {
  from: string;
  to: string;
  rotulo: string | null;
}

export interface ParametrosDeRelatorio {
  mode?: string;
  from?: string;
  to?: string;
}

/** `Date` → `YYYY-MM-DD` no fuso local, sem passar por UTC. */
export function paraDataIso(d: Date): string {
  const mes = String(d.getMonth() + 1).padStart(2, "0");
  const dia = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mes}-${dia}`;
}

/**
 * Resolve o intervalo a partir dos parâmetros da URL.
 *
 * Precedência: intervalo explícito > ciclo financeiro > mês civil corrente.
 * O `to` ausente completa para o fim do mês do `from`.
 */
export function resolverIntervalo(
  params: ParametrosDeRelatorio,
  diaDeInicioDoCiclo?: number | null,
  hoje: Date = new Date(),
): IntervaloDeRelatorio {
  if (params.mode === "financial_period" && diaDeInicioDoCiclo) {
    const periodo = getCurrentPeriod({ startDay: diaDeInicioDoCiclo }, hoje);
    return {
      from: paraDataIso(periodo.startDate),
      to: paraDataIso(periodo.endDate),
      rotulo: periodo.label,
    };
  }

  const from =
    params.from ?? `${hoje.getFullYear()}-${String(hoje.getMonth() + 1).padStart(2, "0")}-01`;

  if (params.to) return { from, to: params.to, rotulo: null };

  // Último dia do mês do `from`: dia 0 do mês seguinte.
  const [ano, mes] = from.split("-").map(Number);
  const ultimoDia = new Date(ano!, mes!, 0);

  return { from, to: paraDataIso(ultimoDia), rotulo: null };
}
