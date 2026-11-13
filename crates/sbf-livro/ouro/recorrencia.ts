// Gera a fixture de ouro do equivalente mensal da recorrência: `equivalenteMensal` de
// apps/web/src/app/actions/painel-recorrentes.ts, para cada frequência e vários valores.
// Rodar com `bun --preload ./preparo.ts recorrencia.ts` (o arquivo é "use server").
import { equivalenteMensal } from "../../../apps/web/src/app/actions/painel-recorrentes.ts";

const FREQUENCIAS = ["monthly", "weekly", "biweekly", "quarterly", "annual", "custom"] as const;
const VALORES = [0, 1, 10, 99.99, 100, 123.45, 1200, 49.9, 0.03, 1234567.89, 7.77];
const INTERVALOS = [null, 0, -5, 1, 7, 10, 15, 30, 45, 90, 365];

const casos = [];
for (const frequencia of FREQUENCIAS) {
  for (const valor of VALORES) {
    for (const intervalo of frequencia === "custom" ? INTERVALOS : [null]) {
      const r = await equivalenteMensal(valor, frequencia, intervalo);
      casos.push({
        frequencia,
        valorCentavos: Math.round(valor * 100),
        intervalo,
        // Em centavos, sem arredondar: o Rust compara a fração exata com este número.
        equivalenteCentavos: r == null ? null : r * 100,
      });
    }
  }
}

console.log(
  JSON.stringify({ fonte: "apps/web/src/app/actions/painel-recorrentes.ts#equivalenteMensal", casos }),
);
