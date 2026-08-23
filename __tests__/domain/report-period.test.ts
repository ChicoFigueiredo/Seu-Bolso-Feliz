/**
 * Intervalo dos relatórios (issue #17).
 *
 * O caso de fevereiro é o motivo de este arquivo existir: a implementação
 * inline anterior transbordava o dia 31 para março e devolvia um período que
 * começava DEPOIS de terminar — um relatório vazio, sem erro, que pareceria
 * "não tenho gastos neste ciclo".
 */
import { describe, expect, it } from "vitest";
import {
  paraDataIso,
  resolverIntervalo,
} from "../../apps/web/src/app/dashboard/reports/report-period";

describe("paraDataIso", () => {
  it("formata no fuso local, sem passar por UTC", () => {
    // `toISOString()` converte para UTC e, a oeste de Greenwich, devolve o dia
    // anterior para qualquer data à meia-noite local.
    expect(paraDataIso(new Date(2026, 2, 1))).toBe("2026-03-01");
    expect(paraDataIso(new Date(2026, 11, 31))).toBe("2026-12-31");
  });
});

describe("resolverIntervalo", () => {
  const hoje = new Date(2026, 2, 15); // 15/03/2026

  it("usa o mês civil corrente quando não há parâmetro", () => {
    expect(resolverIntervalo({}, null, hoje)).toEqual({
      from: "2026-03-01",
      to: "2026-03-31",
      rotulo: null,
    });
  });

  it("respeita o intervalo explícito da URL", () => {
    const r = resolverIntervalo({ from: "2026-01-10", to: "2026-02-09" }, 20, hoje);
    expect(r.from).toBe("2026-01-10");
    expect(r.to).toBe("2026-02-09");
  });

  it("completa o `to` até o fim do mês do `from`", () => {
    expect(resolverIntervalo({ from: "2026-02-01" }, null, hoje).to).toBe("2026-02-28");
  });

  it("usa o ciclo financeiro quando o modo pede", () => {
    // Ciclo que vira no dia 20: em 15/03 o período corrente é 20/02 → 19/03.
    const r = resolverIntervalo({ mode: "financial_period" }, 20, hoje);

    expect(r.from).toBe("2026-02-20");
    expect(r.to).toBe("2026-03-19");
    expect(r.rotulo).toBeTruthy();
  });

  it("não produz período invertido em fevereiro com ciclo no dia 31", () => {
    // A regressão que motivou a issue #17. Fevereiro de 2026 tem 28 dias; o
    // ciclo do dia 31 tem de fechar no dia 28.
    const r = resolverIntervalo({ mode: "financial_period" }, 31, new Date(2026, 1, 15));

    expect(r.from < r.to).toBe(true);
    expect(r.from).toBe("2026-01-31");
    expect(r.to).toBe("2026-02-27");
  });

  it("cai para o mês civil quando o modo pede ciclo mas não há preferência salva", () => {
    const r = resolverIntervalo({ mode: "financial_period" }, null, hoje);
    expect(r.from).toBe("2026-03-01");
  });
});
