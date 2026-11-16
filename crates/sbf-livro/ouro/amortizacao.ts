// Gera a fixture de ouro da amortização: generateSchedule, getOutstandingBalanceAfter,
// totalInterestPaid e simulateEarlyPayoff de packages/domain/src/amortization/index.ts,
// para casos escolhidos à mão e casos sorteados com semente fixa. Valores fictícios.
// Dinheiro sai em centavos (`Math.round(x * 100)` sobre os reais do TS).
import {
  generateSchedule,
  getOutstandingBalanceAfter,
  simulateEarlyPayoff,
  totalInterestPaid,
  type AmortizationConfig,
  type AmortizationSystem,
} from "../../../packages/domain/src/amortization/index.ts";

const c = (reais: number) => Math.round(reais * 100);

interface Entrada {
  sistema: AmortizationSystem;
  principal: number; // centavos
  taxaMilionesimos: number;
  parcelas: number;
  seguro: number; // centavos
  tarifa: number; // centavos
}

const MANUAIS: Entrada[] = [
  { sistema: "sac", principal: 10_000_000, taxaMilionesimos: 10_000, parcelas: 12, seguro: 0, tarifa: 0 },
  { sistema: "price", principal: 10_000_000, taxaMilionesimos: 10_000, parcelas: 12, seguro: 0, tarifa: 0 },
  { sistema: "mixed", principal: 10_000_000, taxaMilionesimos: 10_000, parcelas: 12, seguro: 0, tarifa: 0 },
  // Resíduo positivo e negativo do arredondamento da amortização constante.
  { sistema: "sac", principal: 100_000, taxaMilionesimos: 0, parcelas: 3, seguro: 0, tarifa: 0 },
  { sistema: "sac", principal: 100_000, taxaMilionesimos: 20_000, parcelas: 6, seguro: 0, tarifa: 0 },
  { sistema: "price", principal: 100_000, taxaMilionesimos: 0, parcelas: 7, seguro: 0, tarifa: 0 },
  // Financiamento longo com seguro e taxa administrativa.
  { sistema: "price", principal: 25_000_000, taxaMilionesimos: 8_500, parcelas: 360, seguro: 4_590, tarifa: 2_500 },
  { sistema: "sac", principal: 25_000_000, taxaMilionesimos: 8_500, parcelas: 360, seguro: 4_590, tarifa: 2_500 },
  { sistema: "mixed", principal: 25_000_000, taxaMilionesimos: 8_500, parcelas: 360, seguro: 4_590, tarifa: 2_500 },
  // Misto com uma parcela só (a fase SAC fica vazia) e com número ímpar.
  { sistema: "mixed", principal: 5_000_000, taxaMilionesimos: 15_000, parcelas: 1, seguro: 0, tarifa: 0 },
  { sistema: "mixed", principal: 5_000_000, taxaMilionesimos: 15_000, parcelas: 7, seguro: 0, tarifa: 0 },
  { sistema: "price", principal: 1_234_567, taxaMilionesimos: 29_900, parcelas: 24, seguro: 0, tarifa: 0 },
  { sistema: "sac", principal: 1_234_567, taxaMilionesimos: 29_900, parcelas: 24, seguro: 0, tarifa: 0 },
  { sistema: "price", principal: 800_000, taxaMilionesimos: 123_456, parcelas: 48, seguro: 0, tarifa: 0 },
  { sistema: "mixed", principal: 800_000, taxaMilionesimos: 123_456, parcelas: 48, seguro: 0, tarifa: 0 },
];

// Gerador congruencial com semente fixa: a fixture é reproduzível.
let semente = 20261113;
const sorteio = (max: number) => {
  semente = (semente * 1_103_515_245 + 12_345) % 2_147_483_648;
  return semente % max;
};
const SISTEMAS: AmortizationSystem[] = ["sac", "price", "mixed"];
const SORTEADOS: Entrada[] = Array.from({ length: 60 }, () => ({
  sistema: SISTEMAS[sorteio(3)]!,
  principal: 1_000 + sorteio(50_000_000),
  taxaMilionesimos: sorteio(4) === 0 ? 0 : sorteio(50_000),
  parcelas: 1 + sorteio(120),
  seguro: sorteio(3) === 0 ? sorteio(20_000) : 0,
  tarifa: sorteio(3) === 0 ? sorteio(5_000) : 0,
}));

const casos = [...MANUAIS, ...SORTEADOS].map((e) => {
  const config: AmortizationConfig = {
    system: e.sistema,
    principalAmount: e.principal / 100,
    monthlyRate: e.taxaMilionesimos / 1_000_000,
    totalInstallments: e.parcelas,
    monthlyInsurance: e.seguro / 100,
    monthlyFee: e.tarifa / 100,
  };
  const cronograma = generateSchedule(config).map((p) => [
    p.installmentNumber,
    c(p.totalAmount),
    c(p.principalAmount),
    c(p.interestAmount),
    c(p.insuranceAmount),
    c(p.feeAmount),
    c(p.outstandingBalance),
  ]);
  const marcos = [...new Set([0, 1, Math.floor(e.parcelas / 2), e.parcelas - 1, e.parcelas, e.parcelas + 1])]
    .filter((k) => k >= 0)
    .sort((a, b) => a - b);
  return {
    entrada: e,
    cronograma,
    totalDeJuros: c(totalInterestPaid(config)),
    marcos: marcos.map((k) => {
      const q = simulateEarlyPayoff(config, k);
      return {
        apos: k,
        saldoDevedorApos: c(getOutstandingBalanceAfter(config, k)),
        quitacao: [c(q.outstandingBalance), c(q.totalPaid), c(q.totalInterest), c(q.savedInterest)],
      };
    }),
  };
});

console.log(JSON.stringify({ fonte: "packages/domain/src/amortization/index.ts", casos }));
