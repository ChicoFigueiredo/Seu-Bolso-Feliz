/**
 * Rótulo curto de um período financeiro.
 *
 * `getCurrentPeriod` (@sbf/domain) devolve `01/08/2026 a 31/08/2026` — correto,
 * mas longo demais para caber num rótulo de métrica ou num título de seção a
 * 390px, onde ele quebra em três linhas e empurra o número para fora da
 * primeira dobra.
 *
 * Quando o período coincide com o mês civil — o caso de quem não configurou
 * ciclo próprio — o nome do mês diz tudo: "agosto". Quando não coincide, os
 * dias importam e aparecem, mas sem o ano repetido duas vezes: "20/03 a 19/04".
 */

const MESES = [
  "janeiro",
  "fevereiro",
  "março",
  "abril",
  "maio",
  "junho",
  "julho",
  "agosto",
  "setembro",
  "outubro",
  "novembro",
  "dezembro",
] as const;

/** Lê `2026-08-01` sem passar por fuso: `new Date("2026-08-01")` é UTC. */
function partes(iso: string): { ano: number; mes: number; dia: number } | null {
  const casamento = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!casamento) return null;
  const [, ano, mes, dia] = casamento;
  return { ano: Number(ano), mes: Number(mes), dia: Number(dia) };
}

function ultimoDiaDoMes(ano: number, mes: number): number {
  return new Date(ano, mes, 0).getDate();
}

/**
 * @param inicio primeiro dia do período, em `AAAA-MM-DD`
 * @param fim último dia do período, em `AAAA-MM-DD`
 * @param anoDeReferencia ano "de hoje" — o ano só aparece quando é outro
 */
export function rotuloCurtoDoPeriodo(
  inicio: string,
  fim: string,
  anoDeReferencia: number = new Date().getFullYear(),
): string {
  const a = partes(inicio);
  const b = partes(fim);
  if (!a || !b) return `${inicio} a ${fim}`;

  const mesCivil =
    a.dia === 1 && a.ano === b.ano && a.mes === b.mes && b.dia === ultimoDiaDoMes(b.ano, b.mes);

  if (mesCivil) {
    const nome = MESES[a.mes - 1] ?? String(a.mes);
    return a.ano === anoDeReferencia ? nome : `${nome} de ${String(a.ano)}`;
  }

  const curto = (p: { ano: number; mes: number; dia: number }) =>
    `${String(p.dia).padStart(2, "0")}/${String(p.mes).padStart(2, "0")}`;
  const sufixo = a.ano === anoDeReferencia && b.ano === anoDeReferencia ? "" : `/${String(b.ano)}`;
  return `${curto(a)} a ${curto(b)}${sufixo}`;
}
