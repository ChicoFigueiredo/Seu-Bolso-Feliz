import { describe, it, expect } from "vitest";
import {
  deriveEffectivePriority,
  calculateSortScore,
  prioritizeItems,
  filterByPriority,
  groupByPriority,
  getPriorityRank,
  comparePriorities,
} from "@sbf/domain";
import type { PrioritizableItem, PriorityLevel } from "@sbf/domain";

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

function makeItem(overrides: Partial<PrioritizableItem> & { id: string }): PrioritizableItem {
  return {
    amount: 100,
    ...overrides,
  };
}

describe("Priority — Priorização de Pagamentos", () => {
  // ── deriveEffectivePriority ──────────────────────────────────

  describe("deriveEffectivePriority", () => {
    it("prioridade manual tem precedência", () => {
      const item = makeItem({
        id: "1",
        priority: "high",
        tags: [{ name: "essencial", suggestedPriority: "essential", influencesPriority: true }],
      });

      expect(deriveEffectivePriority(item)).toBe("high");
    });

    it("sem prioridade manual, deriva da tag mais prioritária", () => {
      const item = makeItem({
        id: "1",
        tags: [
          { name: "moradia", suggestedPriority: "essential", influencesPriority: true },
          { name: "trabalho", suggestedPriority: "high", influencesPriority: true },
        ],
      });

      expect(deriveEffectivePriority(item)).toBe("essential");
    });

    it("tags sem influencesPriority são ignoradas", () => {
      const item = makeItem({
        id: "1",
        tags: [{ name: "diversão", suggestedPriority: "low", influencesPriority: false }],
      });

      expect(deriveEffectivePriority(item)).toBe("medium"); // default
    });

    it("sem prioridade e sem tags retorna medium", () => {
      const item = makeItem({ id: "1" });

      expect(deriveEffectivePriority(item)).toBe("medium");
    });

    it("certas tags devem influenciar prioridade (regra 22)", () => {
      // Tags como essencial, moradia, pessoa_fisica devem elevar prioridade
      const item = makeItem({
        id: "1",
        tags: [{ name: "essencial", suggestedPriority: "essential", influencesPriority: true }],
      });

      expect(deriveEffectivePriority(item)).toBe("essential");
    });
  });

  // ── calculateSortScore ───────────────────────────────────────

  describe("calculateSortScore", () => {
    const refDate = new Date(2025, 2, 20);

    it("essential tem score menor que optional", () => {
      const essential = makeItem({ id: "1", priority: "essential", amount: 100 });
      const optional = makeItem({ id: "2", priority: "optional", amount: 100 });

      expect(calculateSortScore(essential, refDate)).toBeLessThan(
        calculateSortScore(optional, refDate),
      );
    });

    it("itens atrasados têm score menor que em dia", () => {
      const overdue = makeItem({ id: "1", priority: "medium", isOverdue: true, amount: 100 });
      const onTime = makeItem({
        id: "2",
        priority: "medium",
        dueDate: "2025-04-01",
        amount: 100,
      });

      expect(calculateSortScore(overdue, refDate)).toBeLessThan(
        calculateSortScore(onTime, refDate),
      );
    });

    it("vencimento próximo (≤3 dias) tem boost de urgência", () => {
      const dueSoon = makeItem({
        id: "1",
        priority: "medium",
        dueDate: "2025-03-22",
        amount: 100,
      });
      const dueLater = makeItem({
        id: "2",
        priority: "medium",
        dueDate: "2025-04-15",
        amount: 100,
      });

      expect(calculateSortScore(dueSoon, refDate)).toBeLessThan(
        calculateSortScore(dueLater, refDate),
      );
    });

    it("item essencial não pago tem prioridade sobre item não-essencial (regra 16)", () => {
      const essential = makeItem({ id: "1", priority: "essential", amount: 50 });
      const optional = makeItem({ id: "2", priority: "optional", amount: 500 });

      const scoreEssential = calculateSortScore(essential, refDate);
      const scoreOptional = calculateSortScore(optional, refDate);

      expect(scoreEssential).toBeLessThan(scoreOptional);
    });
  });

  // ── prioritizeItems ──────────────────────────────────────────

  describe("prioritizeItems", () => {
    it("ordena itens por urgência corretamente", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "opt", priority: "optional", amount: 100 }),
        makeItem({ id: "ess", priority: "essential", isOverdue: true, amount: 200 }),
        makeItem({ id: "med", priority: "medium", dueDate: "2025-03-25", amount: 150 }),
      ];

      const ref = new Date(2025, 2, 20);
      const result = prioritizeItems(items, ref);

      expect(result[0]!.id).toBe("ess"); // essential + overdue primeiro
      expect(result[result.length - 1]!.id).toBe("opt"); // optional por último
    });

    it("todos os itens recebem effectivePriority e sortScore", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "1", priority: "high", amount: 100 }),
        makeItem({ id: "2", amount: 50 }),
      ];

      const result = prioritizeItems(items);

      for (const item of result) {
        expect(item.effectivePriority).toBeDefined();
        expect(item.sortScore).toBeDefined();
      }
    });

    it("primeira tela reflete vencimento, prioridade e período (regra 17)", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "atrasado", priority: "essential", isOverdue: true, amount: 300 }),
        makeItem({ id: "amanha", priority: "high", dueDate: "2025-03-21", amount: 150 }),
        makeItem({ id: "mes-que-vem", priority: "low", dueDate: "2025-04-15", amount: 50 }),
      ];

      const ref = new Date(2025, 2, 20);
      const result = prioritizeItems(items, ref);

      // Atrasado essencial deve vir primeiro
      expect(result[0]!.id).toBe("atrasado");
      // Vencendo amanhã + alta prioridade deve vir em segundo
      expect(result[1]!.id).toBe("amanha");
    });
  });

  // ── filterByPriority ─────────────────────────────────────────

  describe("filterByPriority", () => {
    it("filtra itens até o nível de prioridade especificado", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "1", priority: "essential", amount: 100 }),
        makeItem({ id: "2", priority: "high", amount: 100 }),
        makeItem({ id: "3", priority: "medium", amount: 100 }),
        makeItem({ id: "4", priority: "low", amount: 100 }),
        makeItem({ id: "5", priority: "optional", amount: 100 }),
      ];

      const filtered = filterByPriority(items, "high");

      expect(filtered).toHaveLength(2); // essential + high
    });

    it("filtro 'optional' retorna todos os itens", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "1", priority: "essential", amount: 100 }),
        makeItem({ id: "2", priority: "optional", amount: 100 }),
      ];

      expect(filterByPriority(items, "optional")).toHaveLength(2);
    });
  });

  // ── groupByPriority ──────────────────────────────────────────

  describe("groupByPriority", () => {
    it("agrupa itens corretamente por nível de prioridade", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "1", priority: "essential", amount: 100 }),
        makeItem({ id: "2", priority: "essential", amount: 200 }),
        makeItem({ id: "3", priority: "low", amount: 50 }),
      ];

      const groups = groupByPriority(items);

      expect(groups.essential).toHaveLength(2);
      expect(groups.low).toHaveLength(1);
      expect(groups.high).toHaveLength(0);
      expect(groups.medium).toHaveLength(0);
      expect(groups.optional).toHaveLength(0);
    });
  });

  // ── Utilitários ──────────────────────────────────────────────

  describe("utilitários de prioridade", () => {
    it("getPriorityRank retorna rank numérico (0=mais urgente)", () => {
      expect(getPriorityRank("essential")).toBe(0);
      expect(getPriorityRank("optional")).toBe(4);
    });

    it("comparePriorities retorna negativo quando a é mais urgente", () => {
      expect(comparePriorities("essential", "optional")).toBeLessThan(0);
      expect(comparePriorities("optional", "essential")).toBeGreaterThan(0);
      expect(comparePriorities("medium", "medium")).toBe(0);
    });

    it("hierarquia completa: essential > high > medium > low > optional", () => {
      const levels: PriorityLevel[] = ["essential", "high", "medium", "low", "optional"];
      for (let i = 0; i < levels.length - 1; i++) {
        expect(getPriorityRank(levels[i]!)).toBeLessThan(getPriorityRank(levels[i + 1]!));
      }
    });
  });

  // ── Regra 15: Prioridades influenciam ordenação e alertas ──────

  describe("Regra crítica 15: prioridades influenciam ordenação", () => {
    it("items essenciais nunca ficam abaixo de opcionais na ordenação", () => {
      const items: PrioritizableItem[] = [
        makeItem({ id: "opt1", priority: "optional", amount: 1000 }),
        makeItem({ id: "opt2", priority: "optional", amount: 2000 }),
        makeItem({ id: "ess", priority: "essential", amount: 10 }),
      ];

      const result = prioritizeItems(items);

      const essentialIndex = result.findIndex((r) => r.id === "ess");
      const optionalIndexes = result
        .map((r, i) => (r.id.startsWith("opt") ? i : -1))
        .filter((i) => i >= 0);

      for (const optIdx of optionalIndexes) {
        expect(essentialIndex).toBeLessThan(optIdx);
      }
    });
  });
});
