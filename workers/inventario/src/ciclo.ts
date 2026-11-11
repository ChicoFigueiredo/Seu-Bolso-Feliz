/**
 * Ciclo financeiro do inventário (CONTEXT.md › Ciclo): do dia de corte à
 * véspera do mesmo dia no mês seguinte, inclusive, chamado pelo mês em que
 * começa — o ciclo 2026-10 vai de 2026-10-20 a 2026-11-19.
 *
 * Trabalha só com datas ISO (AAAA-MM-DD), sem `Date` nem fuso: a data de um
 * documento é um dia do calendário, não um instante. A regra é a mesma de
 * `getCurrentPeriod` em @sbf/domain (o teste confere dia a dia).
 */

export const DIA_DE_CORTE = 20;

export interface Ciclo {
  /** AAAA-MM do mês em que o ciclo começa. */
  id: string;
  /** AAAA-MM-DD, inclusive. */
  inicio: string;
  /** AAAA-MM-DD, inclusive. */
  fim: string;
}

const DATA_ISO = /^(\d{4})-(\d{2})-(\d{2})$/;
const ID_CICLO = /^(\d{4})-(\d{2})$/;

function doisDigitos(n: number): string {
  return String(n).padStart(2, "0");
}

function diasNoMes(ano: number, mes: number): number {
  return new Date(Date.UTC(ano, mes, 0)).getUTCDate();
}

function validarCorte(diaDeCorte: number): void {
  // Até 28 o dia existe em todo mês; acima disso o ciclo dependeria de
  // arredondamento em fevereiro, e o corte do usuário é 20.
  if (!Number.isInteger(diaDeCorte) || diaDeCorte < 1 || diaDeCorte > 28) {
    throw new Error(`dia de corte deve ser inteiro de 1 a 28: ${diaDeCorte}`);
  }
}

function mesAnterior(ano: number, mes: number): [number, number] {
  return mes === 1 ? [ano - 1, 12] : [ano, mes - 1];
}

function mesSeguinte(ano: number, mes: number): [number, number] {
  return mes === 12 ? [ano + 1, 1] : [ano, mes + 1];
}

/** Ciclo (AAAA-MM do início) ao qual uma data pertence. */
export function cicloDe(dataIso: string, diaDeCorte = DIA_DE_CORTE): string {
  validarCorte(diaDeCorte);
  const m = DATA_ISO.exec(dataIso);
  if (!m) throw new Error(`data fora do formato AAAA-MM-DD: ${dataIso}`);
  const [ano, mes, dia] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (mes < 1 || mes > 12 || dia < 1 || dia > diasNoMes(ano, mes)) {
    throw new Error(`data inexistente: ${dataIso}`);
  }
  const [a, mm] = dia < diaDeCorte ? mesAnterior(ano, mes) : [ano, mes];
  return `${a}-${doisDigitos(mm)}`;
}

/** Primeiro e último dia (inclusive) de um ciclo. */
export function limitesDoCiclo(id: string, diaDeCorte = DIA_DE_CORTE): Ciclo {
  validarCorte(diaDeCorte);
  const m = ID_CICLO.exec(id);
  if (!m) throw new Error(`ciclo fora do formato AAAA-MM: ${id}`);
  const [ano, mes] = [Number(m[1]), Number(m[2])];
  if (mes < 1 || mes > 12) throw new Error(`ciclo inexistente: ${id}`);

  const inicio = `${ano}-${doisDigitos(mes)}-${doisDigitos(diaDeCorte)}`;
  let fim: string;
  if (diaDeCorte === 1) {
    fim = `${ano}-${doisDigitos(mes)}-${doisDigitos(diasNoMes(ano, mes))}`;
  } else {
    const [a, mm] = mesSeguinte(ano, mes);
    fim = `${a}-${doisDigitos(mm)}-${doisDigitos(diaDeCorte - 1)}`;
  }
  return { id, inicio, fim };
}

/** Todos os ciclos de `primeiro` a `ultimo`, inclusive, em ordem. */
export function ciclosEntre(primeiro: string, ultimo: string, diaDeCorte = DIA_DE_CORTE): Ciclo[] {
  const ciclos: Ciclo[] = [];
  let atual = limitesDoCiclo(primeiro, diaDeCorte);
  const fim = limitesDoCiclo(ultimo, diaDeCorte);
  while (atual.id <= fim.id) {
    ciclos.push(atual);
    const [a, mm] = mesSeguinte(Number(atual.id.slice(0, 4)), Number(atual.id.slice(5, 7)));
    atual = limitesDoCiclo(`${a}-${doisDigitos(mm)}`, diaDeCorte);
  }
  return ciclos;
}

/** Data de hoje no fuso do usuário (o servidor pode estar em UTC). */
export function hojeIso(fuso = "America/Sao_Paulo"): string {
  return new Date().toLocaleDateString("sv-SE", { timeZone: fuso });
}
