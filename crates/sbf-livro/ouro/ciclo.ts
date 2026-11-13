// Gera a fixture de ouro do ciclo: o `getCurrentPeriod` do TS, dia a dia de 2024 a 2026,
// em vários horários de São Paulo e vários dias de corte, convertido para `yyyy-mm-dd` com
// o `paraIso` do web (o mesmo caminho de `painel-hoje.ts`).
//
// Rodar duas vezes (ver gerar.sh):
//   TZ=America/Sao_Paulo → fixtures/ciclo-sao-paulo.json (o que o Rust tem de bater)
//   TZ=UTC               → fixtures/ciclo-utc.json (o erro de fuso do web, documentado)
//
// Saída compacta: para cada (corte, hora), a lista de trechos de dias seguidos que caem no
// mesmo ciclo. O teste Rust expande dia a dia.
import { getCurrentPeriod } from "../../../packages/domain/src/financial-cycle/index.ts";
import { paraIso } from "../../../apps/web/src/lib/datas.ts";

const CORTES = [1, 15, 20, 28, 29, 30, 31];
// Horários de São Paulo (UTC−3, sem horário de verão desde 2019).
const HORAS: [number, number][] = [
  [0, 0],
  [12, 0],
  [20, 59],
  [21, 0],
  [23, 59],
];

interface Trecho {
  desde: string;
  inicio: string;
  fim: string;
  rotulo: string;
}

const casos = [];
for (const corte of CORTES) {
  for (const [h, min] of HORAS) {
    const trechos: Trecho[] = [];
    // Dias civis de São Paulo: o meio-dia UTC de cada dia dá o dia certo para iterar.
    for (let t = Date.UTC(2024, 0, 1, 12); t <= Date.UTC(2026, 11, 31, 12); t += 86_400_000) {
      const dia = new Date(t).toISOString().slice(0, 10);
      const [a, m, d] = dia.split("-").map(Number) as [number, number, number];
      const instante = new Date(Date.UTC(a, m - 1, d, h + 3, min));
      const p = getCurrentPeriod({ startDay: corte }, instante);
      const inicio = paraIso(p.startDate);
      const fim = paraIso(p.endDate);
      const ultimo = trechos[trechos.length - 1];
      if (!ultimo || ultimo.inicio !== inicio || ultimo.fim !== fim) {
        trechos.push({ desde: dia, inicio, fim, rotulo: p.label });
      }
    }
    casos.push({ corte, hora: `${String(h).padStart(2, "0")}:${String(min).padStart(2, "0")}`, trechos });
  }
}

console.log(
  JSON.stringify({
    fonte: "packages/domain/src/financial-cycle/index.ts",
    tz: process.env.TZ ?? "",
    casos,
  }),
);
