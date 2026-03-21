// @sbf/domain — Financial Cycle (Ciclo Financeiro Personalizado)
//
// Lógica pura para cálculo de períodos financeiros personalizados.
// O ciclo financeiro do usuário NÃO segue necessariamente o mês civil.
// Exemplo: ciclo de 20/03 a 19/04 — o dinheiro precisa durar esse período.

export interface FinancialCycleConfig {
  /** Dia do mês em que o ciclo inicia (1-31) */
  startDay: number;
  /** Data âncora para cálculo (default: hoje) */
  anchorDate?: Date;
}

export interface FinancialPeriodRange {
  startDate: Date;
  endDate: Date;
  label: string;
  isCurrent: boolean;
}

/**
 * Calcula a data de início do ciclo para um dado mês/ano.
 * Se o startDay excede os dias do mês, usa o último dia do mês.
 */
function getStartDate(year: number, month: number, startDay: number): Date {
  const lastDayOfMonth = new Date(year, month + 1, 0).getDate();
  const clampedDay = Math.min(startDay, lastDayOfMonth);
  return new Date(year, month, clampedDay);
}

/**
 * Dado um startDay e uma data de referência, retorna o período financeiro
 * que contém essa data.
 */
export function getCurrentPeriod(
  config: FinancialCycleConfig,
  referenceDate?: Date,
): FinancialPeriodRange {
  const ref = referenceDate ?? new Date();
  const { startDay } = config;

  let year = ref.getFullYear();
  let month = ref.getMonth();

  // Se estamos antes do startDay deste mês, voltamos ao mês anterior
  const clampedDay = Math.min(startDay, new Date(year, month + 1, 0).getDate());
  if (ref.getDate() < clampedDay) {
    month -= 1;
    if (month < 0) {
      month = 11;
      year -= 1;
    }
  }

  const startDate = getStartDate(year, month, startDay);

  // End date: dia anterior ao startDay do mês seguinte
  let endYear = year;
  let endMonth = month + 1;
  if (endMonth > 11) {
    endMonth = 0;
    endYear += 1;
  }
  const nextStart = getStartDate(endYear, endMonth, startDay);
  const endDate = new Date(nextStart.getTime() - 86400000); // -1 dia

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const isCurrent = today >= startDate && today <= endDate;

  return {
    startDate,
    endDate,
    label: formatPeriodLabel(startDate, endDate),
    isCurrent,
  };
}

/**
 * Gera N períodos financeiros a partir de uma configuração.
 */
export function generatePeriods(
  config: FinancialCycleConfig,
  count: number,
): FinancialPeriodRange[] {
  const anchor = config.anchorDate ?? new Date();
  const { startDay } = config;
  const periods: FinancialPeriodRange[] = [];

  let year = anchor.getFullYear();
  let month = anchor.getMonth();

  // Ajustar para o início do ciclo atual
  const clampedDay = Math.min(startDay, new Date(year, month + 1, 0).getDate());
  if (anchor.getDate() < clampedDay) {
    month -= 1;
    if (month < 0) {
      month = 11;
      year -= 1;
    }
  }

  const today = new Date();
  today.setHours(0, 0, 0, 0);

  for (let i = 0; i < count; i++) {
    let currentMonth = month + i;
    let currentYear = year;
    while (currentMonth > 11) {
      currentMonth -= 12;
      currentYear += 1;
    }

    const startDate = getStartDate(currentYear, currentMonth, startDay);

    let nextMonth = currentMonth + 1;
    let nextYear = currentYear;
    if (nextMonth > 11) {
      nextMonth = 0;
      nextYear += 1;
    }
    const nextStart = getStartDate(nextYear, nextMonth, startDay);
    const endDate = new Date(nextStart.getTime() - 86400000);

    const isCurrent = today >= startDate && today <= endDate;

    periods.push({
      startDate,
      endDate,
      label: formatPeriodLabel(startDate, endDate),
      isCurrent,
    });
  }

  return periods;
}

/**
 * Determina em qual período financeiro uma data cai.
 * Retorna null se a data não está em nenhum período gerado.
 */
export function findPeriodForDate(
  config: FinancialCycleConfig,
  date: Date,
  lookbackMonths: number = 24,
  lookforwardMonths: number = 12,
): FinancialPeriodRange | null {
  // Gerar períodos cobrindo lookback e lookforward
  const totalMonths = lookbackMonths + lookforwardMonths;
  const startAnchor = new Date(date);
  startAnchor.setMonth(startAnchor.getMonth() - lookbackMonths);

  const periods = generatePeriods({ ...config, anchorDate: startAnchor }, totalMonths);

  return periods.find((p) => date >= p.startDate && date <= p.endDate) ?? null;
}

/**
 * Calcula quantos dias restam no período financeiro atual.
 */
export function daysRemainingInPeriod(config: FinancialCycleConfig, referenceDate?: Date): number {
  const ref = referenceDate ?? new Date();
  const period = getCurrentPeriod(config, ref);
  const diff = period.endDate.getTime() - ref.getTime();
  return Math.max(0, Math.ceil(diff / 86400000));
}

function formatPeriodLabel(start: Date, end: Date): string {
  const fmt = (d: Date) =>
    `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}/${d.getFullYear()}`;
  return `${fmt(start)} a ${fmt(end)}`;
}
