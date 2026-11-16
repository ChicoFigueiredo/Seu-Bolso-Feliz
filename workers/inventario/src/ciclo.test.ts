import { getCurrentPeriod } from "@sbf/domain";
import { describe, expect, it } from "vitest";
import { cicloDe, ciclosEntre, limitesDoCiclo } from "./ciclo";

describe("cicloDe", () => {
  it.each([
    ["2026-11-11", "2026-10"],
    ["2026-11-19", "2026-10"],
    ["2026-11-20", "2026-11"],
    ["2026-10-20", "2026-10"],
    ["2026-01-05", "2025-12"],
    ["2026-01-20", "2026-01"],
    ["2024-02-29", "2024-02"],
  ])("%s cai no ciclo %s", (data, ciclo) => {
    expect(cicloDe(data)).toBe(ciclo);
  });

  it("recusa data inexistente ou mal formatada", () => {
    expect(() => cicloDe("2026-02-30")).toThrow("inexistente");
    expect(() => cicloDe("11/11/2026")).toThrow("formato");
  });

  it("concorda com @sbf/domain em todos os dias de 2024 a 2026", () => {
    const divergencias: string[] = [];
    for (let d = new Date(2024, 0, 1, 12); d.getFullYear() < 2027; d.setDate(d.getDate() + 1)) {
      const iso = d.toLocaleDateString("sv-SE");
      const inicio = getCurrentPeriod({ startDay: 20 }, new Date(d)).startDate;
      const esperado = `${inicio.getFullYear()}-${String(inicio.getMonth() + 1).padStart(2, "0")}`;
      if (cicloDe(iso) !== esperado) divergencias.push(`${iso}: ${cicloDe(iso)} ≠ ${esperado}`);
    }
    expect(divergencias).toEqual([]);
  });
});

describe("limitesDoCiclo", () => {
  it("vai do dia 20 ao dia 19 seguinte, inclusive", () => {
    expect(limitesDoCiclo("2026-10")).toEqual({
      id: "2026-10",
      inicio: "2026-10-20",
      fim: "2026-11-19",
    });
    expect(limitesDoCiclo("2025-12").fim).toBe("2026-01-19");
  });

  it("com corte no dia 1 vira o mês civil", () => {
    expect(limitesDoCiclo("2024-02", 1)).toEqual({
      id: "2024-02",
      inicio: "2024-02-01",
      fim: "2024-02-29",
    });
  });
});

describe("ciclosEntre", () => {
  it("lista 12 ciclos completos mais o atual, sem buraco nem sobreposição", () => {
    const ciclos = ciclosEntre("2025-10", "2026-10");
    expect(ciclos).toHaveLength(13);
    for (let i = 1; i < ciclos.length; i++) {
      const anterior = ciclos[i - 1]!;
      const atual = ciclos[i]!;
      const diaSeguinte = new Date(`${anterior.fim}T12:00:00Z`);
      diaSeguinte.setUTCDate(diaSeguinte.getUTCDate() + 1);
      expect(atual.inicio).toBe(diaSeguinte.toISOString().slice(0, 10));
    }
  });
});
