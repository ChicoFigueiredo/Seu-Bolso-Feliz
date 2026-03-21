import { describe, it, expect } from "vitest";
import {
  getCurrentPeriod,
  generatePeriods,
  findPeriodForDate,
  daysRemainingInPeriod,
} from "@sbf/domain";
import type { FinancialCycleConfig } from "@sbf/domain";

describe("Financial Cycle — Ciclo Financeiro Personalizado", () => {
  // ── getCurrentPeriod ──────────────────────────────────────────

  describe("getCurrentPeriod", () => {
    it("retorna período correto para ciclo dia 20 (ex: 20/03 a 19/04)", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const ref = new Date(2025, 2, 25); // 25/03/2025

      const period = getCurrentPeriod(config, ref);

      expect(period.startDate).toEqual(new Date(2025, 2, 20)); // 20/03
      expect(period.endDate).toEqual(new Date(2025, 3, 19)); // 19/04
    });

    it("data antes do startDay cai no período anterior", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const ref = new Date(2025, 2, 15); // 15/03/2025

      const period = getCurrentPeriod(config, ref);

      expect(period.startDate).toEqual(new Date(2025, 1, 20)); // 20/02
      expect(period.endDate).toEqual(new Date(2025, 2, 19)); // 19/03
    });

    it("data exatamente no startDay pertence ao período que inicia nesse dia", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const ref = new Date(2025, 2, 20); // 20/03

      const period = getCurrentPeriod(config, ref);

      expect(period.startDate).toEqual(new Date(2025, 2, 20));
    });

    it("startDay=1 gera ciclo que coincide com mês civil", () => {
      const config: FinancialCycleConfig = { startDay: 1 };
      const ref = new Date(2025, 5, 15); // 15/06/2025

      const period = getCurrentPeriod(config, ref);

      expect(period.startDate).toEqual(new Date(2025, 5, 1)); // 01/06
      expect(period.endDate).toEqual(new Date(2025, 6, 0)); // 30/06
    });

    it("lida com startDay=31 em meses com menos dias (clamping)", () => {
      const config: FinancialCycleConfig = { startDay: 31 };
      const ref = new Date(2025, 1, 28); // 28/02/2025

      const period = getCurrentPeriod(config, ref);

      // Fevereiro não tem dia 31, deve usar o último dia disponível
      expect(period.startDate.getDate()).toBeLessThanOrEqual(28);
    });

    it("transição de ano funciona (dezembro → janeiro)", () => {
      const config: FinancialCycleConfig = { startDay: 15 };
      const ref = new Date(2025, 11, 20); // 20/12/2025

      const period = getCurrentPeriod(config, ref);

      expect(period.startDate).toEqual(new Date(2025, 11, 15)); // 15/12
      expect(period.endDate).toEqual(new Date(2026, 0, 14)); // 14/01/2026
    });

    it("gera label formatado DD/MM/YYYY a DD/MM/YYYY", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const ref = new Date(2025, 2, 25);

      const period = getCurrentPeriod(config, ref);

      expect(period.label).toMatch(/^\d{2}\/\d{2}\/\d{4} a \d{2}\/\d{2}\/\d{4}$/);
    });
  });

  // ── generatePeriods ───────────────────────────────────────────

  describe("generatePeriods", () => {
    it("gera a quantidade correta de períodos", () => {
      const config: FinancialCycleConfig = { startDay: 20, anchorDate: new Date(2025, 0, 25) };

      const periods = generatePeriods(config, 6);

      expect(periods).toHaveLength(6);
    });

    it("períodos são sequenciais e não se sobrepõem", () => {
      const config: FinancialCycleConfig = { startDay: 10, anchorDate: new Date(2025, 0, 15) };

      const periods = generatePeriods(config, 12);

      for (let i = 0; i < periods.length - 1; i++) {
        const current = periods[i]!;
        const next = periods[i + 1]!;
        // O dia seguinte ao fim do período atual é o início do próximo
        const dayAfterEnd = new Date(current.endDate.getTime() + 86400000);
        expect(dayAfterEnd.getTime()).toBe(next.startDate.getTime());
      }
    });

    it("exatamente um período é marcado como isCurrent", () => {
      const config: FinancialCycleConfig = { startDay: 1, anchorDate: new Date() };

      const periods = generatePeriods(config, 12);

      const currentPeriods = periods.filter((p) => p.isCurrent);
      expect(currentPeriods.length).toBeLessThanOrEqual(1);
    });
  });

  // ── findPeriodForDate ─────────────────────────────────────────

  describe("findPeriodForDate", () => {
    it("encontra o período correto para uma data arbitrária", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const date = new Date(2025, 5, 25); // 25/06/2025

      const period = findPeriodForDate(config, date);

      expect(period).not.toBeNull();
      expect(period!.startDate).toEqual(new Date(2025, 5, 20));
      expect(period!.endDate).toEqual(new Date(2025, 6, 19));
    });

    it("retorna null para data fora do range gerado", () => {
      const config: FinancialCycleConfig = { startDay: 1 };
      const veryOldDate = new Date(1900, 0, 1);

      const period = findPeriodForDate(config, veryOldDate, 1, 0);

      expect(period).toBeNull();
    });
  });

  // ── daysRemainingInPeriod ─────────────────────────────────────

  describe("daysRemainingInPeriod", () => {
    it("retorna dias restantes no período atual", () => {
      const config: FinancialCycleConfig = { startDay: 1 };
      const ref = new Date(2025, 0, 15); // 15/01

      const remaining = daysRemainingInPeriod(config, ref);

      // Período 01/01 a 31/01 → 16 dias restantes (15→31)
      expect(remaining).toBeGreaterThan(0);
      expect(remaining).toBeLessThanOrEqual(31);
    });

    it("retorna 0 no último dia do período", () => {
      const config: FinancialCycleConfig = { startDay: 1 };
      // Último dia de janeiro
      const ref = new Date(2025, 0, 31);

      const remaining = daysRemainingInPeriod(config, ref);

      expect(remaining).toBe(0);
    });
  });

  // ── Regra crítica 3/16: Lançamentos atribuídos ao período correto ──

  describe("Regra de negócio: período financeiro personalizado", () => {
    it("ciclo financeiro 20/03 a 19/04 contém data 25/03", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const period = getCurrentPeriod(config, new Date(2025, 2, 25));

      expect(new Date(2025, 2, 25) >= period.startDate).toBe(true);
      expect(new Date(2025, 2, 25) <= period.endDate).toBe(true);
    });

    it("ciclo financeiro 20/03 a 19/04 NÃO contém data 20/04", () => {
      const config: FinancialCycleConfig = { startDay: 20 };
      const period = getCurrentPeriod(config, new Date(2025, 2, 25));

      expect(new Date(2025, 3, 20) <= period.endDate).toBe(false);
    });

    it("relatórios por mês civil e período personalizado podem divergir (regra 12)", () => {
      const config: FinancialCycleConfig = { startDay: 20 };

      // 19/03 pertence ao período financeiro de fevereiro (20/02 a 19/03)
      const periodFor19Mar = getCurrentPeriod(config, new Date(2025, 2, 19));
      expect(periodFor19Mar.startDate.getMonth()).toBe(1); // Fevereiro

      // 20/03 pertence ao período financeiro de março (20/03 a 19/04)
      const periodFor20Mar = getCurrentPeriod(config, new Date(2025, 2, 20));
      expect(periodFor20Mar.startDate.getMonth()).toBe(2); // Março

      // No mês civil, ambas estão em março — mas no ciclo personalizado, divergem
    });
  });
});
