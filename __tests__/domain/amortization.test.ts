import { describe, it, expect } from "vitest";
import {
  generateSACSchedule,
  generatePriceSchedule,
  generateSchedule,
  getOutstandingBalanceAfter,
  totalInterestPaid,
  simulateEarlyPayoff,
} from "@sbf/domain";
import type { AmortizationConfig } from "@sbf/domain";

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

const DEFAULT_CONFIG: AmortizationConfig = {
  system: "sac",
  principalAmount: 100000,
  monthlyRate: 0.01, // 1% a.m.
  totalInstallments: 12,
};

function roundMoney(value: number): number {
  return Math.round(value * 100) / 100;
}

describe("Amortization — Amortização de Dívidas", () => {
  // ── SAC (Sistema de Amortização Constante) ─────────────────────

  describe("SAC", () => {
    it("gera o número correto de parcelas", () => {
      const schedule = generateSACSchedule(DEFAULT_CONFIG);
      expect(schedule).toHaveLength(12);
    });

    it("amortização é constante em todas as parcelas", () => {
      const schedule = generateSACSchedule(DEFAULT_CONFIG);
      const expectedAmortization = roundMoney(100000 / 12);

      for (const inst of schedule) {
        expect(inst.principalAmount).toBeCloseTo(expectedAmortization, 0);
      }
    });

    it("juros são decrescentes", () => {
      const schedule = generateSACSchedule(DEFAULT_CONFIG);

      for (let i = 0; i < schedule.length - 1; i++) {
        expect(schedule[i]!.interestAmount).toBeGreaterThanOrEqual(schedule[i + 1]!.interestAmount);
      }
    });

    it("parcela total é decrescente", () => {
      const schedule = generateSACSchedule(DEFAULT_CONFIG);

      for (let i = 0; i < schedule.length - 1; i++) {
        expect(schedule[i]!.totalAmount).toBeGreaterThanOrEqual(schedule[i + 1]!.totalAmount);
      }
    });

    it("saldo devedor final é zero (ou muito próximo)", () => {
      const schedule = generateSACSchedule(DEFAULT_CONFIG);
      const lastInst = schedule[schedule.length - 1]!;
      expect(lastInst.outstandingBalance).toBeCloseTo(0, 0);
    });

    it("regra 6: cada parcela separa amortização, juros e encargos", () => {
      const config: AmortizationConfig = {
        ...DEFAULT_CONFIG,
        monthlyInsurance: 50,
        monthlyFee: 25,
      };
      const schedule = generateSACSchedule(config);

      for (const inst of schedule) {
        expect(inst.principalAmount).toBeGreaterThan(0);
        expect(inst.interestAmount).toBeGreaterThanOrEqual(0);
        expect(inst.insuranceAmount).toBe(50);
        expect(inst.feeAmount).toBe(25);
        expect(inst.totalAmount).toBeCloseTo(
          inst.principalAmount + inst.interestAmount + inst.insuranceAmount + inst.feeAmount,
          2,
        );
      }
    });
  });

  // ── Price (Tabela Price) ───────────────────────────────────────

  describe("Price", () => {
    const priceConfig: AmortizationConfig = { ...DEFAULT_CONFIG, system: "price" };

    it("gera o número correto de parcelas", () => {
      const schedule = generatePriceSchedule(priceConfig);
      expect(schedule).toHaveLength(12);
    });

    it("parcela total é constante (excluindo seguro/taxa)", () => {
      const schedule = generatePriceSchedule(priceConfig);
      const firstTotal = schedule[0]!.totalAmount;

      for (const inst of schedule) {
        expect(inst.totalAmount).toBeCloseTo(firstTotal, 0);
      }
    });

    it("amortização é crescente", () => {
      const schedule = generatePriceSchedule(priceConfig);

      for (let i = 0; i < schedule.length - 1; i++) {
        expect(schedule[i + 1]!.principalAmount).toBeGreaterThanOrEqual(
          schedule[i]!.principalAmount,
        );
      }
    });

    it("juros são decrescentes", () => {
      const schedule = generatePriceSchedule(priceConfig);

      for (let i = 0; i < schedule.length - 1; i++) {
        expect(schedule[i]!.interestAmount).toBeGreaterThanOrEqual(schedule[i + 1]!.interestAmount);
      }
    });

    it("saldo devedor final é zero (ou muito próximo)", () => {
      const schedule = generatePriceSchedule(priceConfig);
      const lastInst = schedule[schedule.length - 1]!;
      expect(lastInst.outstandingBalance).toBeCloseTo(0, 0);
    });

    it("Price com taxa 0% divide igualmente", () => {
      const zeroRate: AmortizationConfig = {
        system: "price",
        principalAmount: 12000,
        monthlyRate: 0,
        totalInstallments: 12,
      };
      const schedule = generatePriceSchedule(zeroRate);

      for (const inst of schedule) {
        expect(inst.totalAmount).toBeCloseTo(1000, 1);
        expect(inst.interestAmount).toBe(0);
      }
    });
  });

  // ── Mixed (Misto: SAC + Price) ─────────────────────────────────

  describe("Mixed (Misto)", () => {
    const mixedConfig: AmortizationConfig = { ...DEFAULT_CONFIG, system: "mixed" };

    it("gera o número correto de parcelas", () => {
      const schedule = generateSchedule(mixedConfig);
      expect(schedule).toHaveLength(12);
    });

    it("parcelas são numeradas sequencialmente", () => {
      const schedule = generateSchedule(mixedConfig);
      for (let i = 0; i < schedule.length; i++) {
        expect(schedule[i]!.installmentNumber).toBe(i + 1);
      }
    });

    it("saldo devedor final é zero ou próximo", () => {
      const schedule = generateSchedule(mixedConfig);
      const lastInst = schedule[schedule.length - 1]!;
      expect(lastInst.outstandingBalance).toBeCloseTo(0, 0);
    });
  });

  // ── getOutstandingBalanceAfter ─────────────────────────────────

  describe("getOutstandingBalanceAfter", () => {
    it("saldo após 0 parcelas é o principal", () => {
      expect(getOutstandingBalanceAfter(DEFAULT_CONFIG, 0)).toBe(100000);
    });

    it("saldo após todas as parcelas é 0", () => {
      expect(getOutstandingBalanceAfter(DEFAULT_CONFIG, 12)).toBe(0);
    });

    it("saldo diminui com cada parcela paga", () => {
      const after3 = getOutstandingBalanceAfter(DEFAULT_CONFIG, 3);
      const after6 = getOutstandingBalanceAfter(DEFAULT_CONFIG, 6);
      const after9 = getOutstandingBalanceAfter(DEFAULT_CONFIG, 9);

      expect(after3).toBeGreaterThan(after6);
      expect(after6).toBeGreaterThan(after9);
    });
  });

  // ── totalInterestPaid ──────────────────────────────────────────

  describe("totalInterestPaid", () => {
    it("juros totais são maiores que 0 para taxa > 0", () => {
      const total = totalInterestPaid(DEFAULT_CONFIG);
      expect(total).toBeGreaterThan(0);
    });

    it("juros totais são 0 para taxa 0", () => {
      const zeroRate: AmortizationConfig = {
        ...DEFAULT_CONFIG,
        monthlyRate: 0,
      };
      const total = totalInterestPaid(zeroRate);
      expect(total).toBe(0);
    });

    it("Price gera mais juros que SAC para mesmos parâmetros", () => {
      const sacInterest = totalInterestPaid({ ...DEFAULT_CONFIG, system: "sac" });
      const priceInterest = totalInterestPaid({ ...DEFAULT_CONFIG, system: "price" });

      expect(priceInterest).toBeGreaterThan(sacInterest);
    });
  });

  // ── simulateEarlyPayoff ────────────────────────────────────────

  describe("simulateEarlyPayoff — Regra 11: quitação antecipada", () => {
    it("quitação antecipada gera economia de juros", () => {
      const result = simulateEarlyPayoff(DEFAULT_CONFIG, 6); // quita após 6 parcelas

      expect(result.savedInterest).toBeGreaterThan(0);
      expect(result.outstandingBalance).toBeGreaterThan(0);
    });

    it("quitação após última parcela tem saldo 0 e juros salvos 0", () => {
      const result = simulateEarlyPayoff(DEFAULT_CONFIG, 12);

      expect(result.outstandingBalance).toBe(0);
      expect(result.savedInterest).toBe(0);
    });

    it("quanto mais cedo quita, mais juros economiza", () => {
      const earlyPayoff = simulateEarlyPayoff(DEFAULT_CONFIG, 3);
      const laterPayoff = simulateEarlyPayoff(DEFAULT_CONFIG, 9);

      expect(earlyPayoff.savedInterest).toBeGreaterThan(laterPayoff.savedInterest);
    });

    it("recalcula corretamente saldo e impacto (regra 11)", () => {
      const result = simulateEarlyPayoff(DEFAULT_CONFIG, 6);

      // O saldo devedor + juros pagos + economia deve ser consistente
      const totalScheduleInterest = totalInterestPaid(DEFAULT_CONFIG);
      expect(roundMoney(result.totalInterest + result.savedInterest)).toBeCloseTo(
        totalScheduleInterest,
        2,
      );
    });
  });

  // ── generateSchedule dispatcher ────────────────────────────────

  describe("generateSchedule", () => {
    it("despacha corretamente para SAC", () => {
      const schedule = generateSchedule({ ...DEFAULT_CONFIG, system: "sac" });
      // SAC: parcela decrescente
      expect(schedule[0]!.totalAmount).toBeGreaterThan(schedule[schedule.length - 1]!.totalAmount);
    });

    it("despacha corretamente para Price", () => {
      const schedule = generateSchedule({ ...DEFAULT_CONFIG, system: "price" });
      // Price: parcelas iguais
      expect(schedule[0]!.totalAmount).toBeCloseTo(schedule[schedule.length - 1]!.totalAmount, 0);
    });

    it("sistema desconhecido lança erro", () => {
      expect(() => generateSchedule({ ...DEFAULT_CONFIG, system: "xyz" as "sac" })).toThrow();
    });
  });

  // ── Financiamento habitacional complexo ──────────────────────

  describe("financiamento habitacional (cenário realista)", () => {
    const mortgageConfig: AmortizationConfig = {
      system: "sac",
      principalAmount: 300000,
      monthlyRate: 0.0075, // 0.75% a.m. (~9% a.a.)
      totalInstallments: 360, // 30 anos
      monthlyInsurance: 120,
      monthlyFee: 25,
    };

    it("gera 360 parcelas", () => {
      const schedule = generateSchedule(mortgageConfig);
      expect(schedule).toHaveLength(360);
    });

    it("primeira parcela é maior que a última", () => {
      const schedule = generateSchedule(mortgageConfig);
      expect(schedule[0]!.totalAmount).toBeGreaterThan(schedule[359]!.totalAmount);
    });

    it("saldo final é zero (tolerância de arredondamento em 360 parcelas)", () => {
      const schedule = generateSchedule(mortgageConfig);
      expect(schedule[359]!.outstandingBalance).toBeLessThan(5);
    });

    it("seguro e taxa são constantes", () => {
      const schedule = generateSchedule(mortgageConfig);
      for (const inst of schedule) {
        expect(inst.insuranceAmount).toBe(120);
        expect(inst.feeAmount).toBe(25);
      }
    });

    it("quitação antecipada após 5 anos gera economia significativa", () => {
      const result = simulateEarlyPayoff(mortgageConfig, 60); // 5 anos

      expect(result.savedInterest).toBeGreaterThan(50000); // economia significativa
      expect(result.outstandingBalance).toBeGreaterThan(0);
      expect(result.outstandingBalance).toBeLessThan(300000);
    });
  });
});
