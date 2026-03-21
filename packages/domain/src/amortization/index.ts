// @sbf/domain — Amortization Logic (Amortização: SAC, Price, Misto)
//
// Lógica pura para cálculos de amortização de dívidas.
// Suporta: SAC (parcela decrescente), Price (parcela fixa), e Misto.

export type AmortizationSystem = "sac" | "price" | "mixed";

export interface AmortizationConfig {
  system: AmortizationSystem;
  /** Valor original do empréstimo/financiamento */
  principalAmount: number;
  /** Taxa de juros mensal (ex: 0.01 = 1% a.m.) */
  monthlyRate: number;
  /** Número total de parcelas */
  totalInstallments: number;
  /** Seguro mensal (valor fixo, opcional) */
  monthlyInsurance?: number;
  /** Taxa administrativa mensal (valor fixo, opcional) */
  monthlyFee?: number;
}

export interface InstallmentBreakdown {
  installmentNumber: number;
  totalAmount: number;
  principalAmount: number;
  interestAmount: number;
  insuranceAmount: number;
  feeAmount: number;
  outstandingBalance: number;
}

/**
 * Gera o cronograma completo de amortização SAC.
 * Amortização constante, juros decrescentes, parcela decrescente.
 */
export function generateSACSchedule(config: AmortizationConfig): InstallmentBreakdown[] {
  const {
    principalAmount,
    monthlyRate,
    totalInstallments,
    monthlyInsurance = 0,
    monthlyFee = 0,
  } = config;
  const constantAmortization = principalAmount / totalInstallments;
  const schedule: InstallmentBreakdown[] = [];
  let balance = principalAmount;

  for (let i = 1; i <= totalInstallments; i++) {
    const interest = roundMoney(balance * monthlyRate);
    const principal = roundMoney(constantAmortization);
    const total = roundMoney(principal + interest + monthlyInsurance + monthlyFee);
    balance = roundMoney(balance - principal);

    schedule.push({
      installmentNumber: i,
      totalAmount: total,
      principalAmount: principal,
      interestAmount: interest,
      insuranceAmount: monthlyInsurance,
      feeAmount: monthlyFee,
      outstandingBalance: Math.max(0, balance),
    });
  }

  return schedule;
}

/**
 * Gera o cronograma completo de amortização Price (Tabela Price).
 * Parcela fixa, amortização crescente, juros decrescentes.
 */
export function generatePriceSchedule(config: AmortizationConfig): InstallmentBreakdown[] {
  const {
    principalAmount,
    monthlyRate,
    totalInstallments,
    monthlyInsurance = 0,
    monthlyFee = 0,
  } = config;

  // Parcela fixa (excluindo seguro e taxa)
  const fixedPayment =
    monthlyRate === 0
      ? principalAmount / totalInstallments
      : (principalAmount * monthlyRate * Math.pow(1 + monthlyRate, totalInstallments)) /
        (Math.pow(1 + monthlyRate, totalInstallments) - 1);

  const schedule: InstallmentBreakdown[] = [];
  let balance = principalAmount;

  for (let i = 1; i <= totalInstallments; i++) {
    const interest = roundMoney(balance * monthlyRate);
    const principal = roundMoney(fixedPayment - interest);
    const total = roundMoney(fixedPayment + monthlyInsurance + monthlyFee);
    balance = roundMoney(balance - principal);

    schedule.push({
      installmentNumber: i,
      totalAmount: total,
      principalAmount: principal,
      interestAmount: interest,
      insuranceAmount: monthlyInsurance,
      feeAmount: monthlyFee,
      outstandingBalance: Math.max(0, balance),
    });
  }

  return schedule;
}

/**
 * Gera cronograma de amortização baseado no sistema escolhido.
 */
export function generateSchedule(config: AmortizationConfig): InstallmentBreakdown[] {
  switch (config.system) {
    case "sac":
      return generateSACSchedule(config);
    case "price":
      return generatePriceSchedule(config);
    case "mixed":
      // Sistema misto: metade SAC + metade Price
      // Primeira metade em SAC, segunda em Price com saldo restante
      return generateMixedSchedule(config);
    default:
      throw new Error(`Sistema de amortização desconhecido: ${config.system}`);
  }
}

function generateMixedSchedule(config: AmortizationConfig): InstallmentBreakdown[] {
  const midpoint = Math.floor(config.totalInstallments / 2);

  // Primeira metade: SAC
  const sacConfig = { ...config, totalInstallments: midpoint };
  const sacSchedule = generateSACSchedule(sacConfig);

  // Saldo restante após a fase SAC
  const remainingBalance =
    sacSchedule.length > 0
      ? sacSchedule[sacSchedule.length - 1]!.outstandingBalance
      : config.principalAmount;
  const remainingInstallments = config.totalInstallments - midpoint;

  // Segunda metade: Price com saldo restante
  const priceConfig = {
    ...config,
    principalAmount: remainingBalance,
    totalInstallments: remainingInstallments,
  };
  const priceSchedule = generatePriceSchedule(priceConfig);

  // Renumerar parcelas da fase Price
  return [
    ...sacSchedule,
    ...priceSchedule.map((inst) => ({
      ...inst,
      installmentNumber: inst.installmentNumber + midpoint,
    })),
  ];
}

/**
 * Calcula o saldo devedor após N parcelas pagas.
 */
export function getOutstandingBalanceAfter(
  config: AmortizationConfig,
  paidInstallments: number,
): number {
  const schedule = generateSchedule(config);
  if (paidInstallments >= schedule.length) return 0;
  if (paidInstallments === 0) return config.principalAmount;
  return schedule[paidInstallments - 1]!.outstandingBalance;
}

/**
 * Calcula o total de juros pagos em um cronograma.
 */
export function totalInterestPaid(config: AmortizationConfig): number {
  const schedule = generateSchedule(config);
  return roundMoney(schedule.reduce((sum, inst) => sum + inst.interestAmount, 0));
}

/**
 * Simula quitação antecipada: calcula o saldo devedor + juros pro-rata.
 */
export function simulateEarlyPayoff(
  config: AmortizationConfig,
  afterInstallment: number,
): { outstandingBalance: number; totalPaid: number; totalInterest: number; savedInterest: number } {
  const schedule = generateSchedule(config);
  const totalScheduleInterest = schedule.reduce((sum, inst) => sum + inst.interestAmount, 0);

  if (afterInstallment >= schedule.length) {
    return {
      outstandingBalance: 0,
      totalPaid: roundMoney(schedule.reduce((sum, inst) => sum + inst.totalAmount, 0)),
      totalInterest: roundMoney(totalScheduleInterest),
      savedInterest: 0,
    };
  }

  const paidInstallments = schedule.slice(0, afterInstallment);
  const totalPaid = roundMoney(paidInstallments.reduce((sum, inst) => sum + inst.totalAmount, 0));
  const paidInterest = roundMoney(
    paidInstallments.reduce((sum, inst) => sum + inst.interestAmount, 0),
  );
  const outstandingBalance =
    afterInstallment > 0
      ? schedule[afterInstallment - 1]!.outstandingBalance
      : config.principalAmount;
  const savedInterest = roundMoney(totalScheduleInterest - paidInterest);

  return {
    outstandingBalance: roundMoney(outstandingBalance),
    totalPaid: roundMoney(totalPaid + outstandingBalance),
    totalInterest: roundMoney(paidInterest),
    savedInterest,
  };
}

function roundMoney(value: number): number {
  return Math.round(value * 100) / 100;
}
