"use client";

/**
 * Como o gasto do centro evoluiu dentro do período.
 *
 * Duas leituras no mesmo desenho, de propósito: as barras dizem **quando** o
 * dinheiro saiu (uma compra grande no dia 3 é uma barra alta, não uma
 * inclinação suave) e a linha diz **quanto já foi** até ali. Só a linha
 * esconderia a concentração; só as barras esconderiam o total.
 *
 * A série chega densa — um ponto por dia do período, inclusive os dias
 * parados. Sem isso a linha subiria em ritmo constante mesmo com duas semanas
 * sem nada, e a leitura "o projeto consumiu tudo na primeira semana"
 * desapareceria.
 */

import { formatBRL } from "@sbf/design-system";
import type { PontoEvolucao } from "@/app/actions/painel-centros";

export interface EvolucaoGastoProps {
  pontos: readonly PontoEvolucao[];
  /** Data de hoje em ISO, para marcar até onde o período já aconteceu. */
  hojeIso: string;
}

const ALTURA = 100;

function diaCurto(iso: string): string {
  const data = new Date(`${iso}T12:00:00`);
  return `${String(data.getDate()).padStart(2, "0")}/${String(data.getMonth() + 1).padStart(2, "0")}`;
}

export function EvolucaoGasto({ pontos, hojeIso }: EvolucaoGastoProps) {
  const total = pontos.at(-1)?.acumulado ?? 0;
  const maiorDia = pontos.reduce((maior, ponto) => Math.max(maior, ponto.saiu), 0);
  const primeiro = pontos[0];
  const ultimo = pontos.at(-1);

  if (!primeiro || !ultimo || total <= 0) {
    return (
      <div style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.5 }}>
        Nenhuma saída rateada para este centro neste período — não há curva para desenhar ainda.
      </div>
    );
  }

  const largura = pontos.length;
  const alturaDaBarra = (valor: number) => (maiorDia > 0 ? (valor / maiorDia) * ALTURA : 0);
  const linha = pontos
    .map((ponto, indice) => {
      const x = pontos.length === 1 ? largura / 2 : (indice / (pontos.length - 1)) * largura;
      const y = ALTURA - (ponto.acumulado / total) * ALTURA;
      return `${String(x)},${String(y)}`;
    })
    .join(" ");

  const diaDeMaiorGasto = pontos.reduce((maior, ponto) =>
    ponto.saiu > maior.saiu ? ponto : maior,
  );

  return (
    <div style={{ display: "grid", gap: 8 }}>
      <svg
        viewBox={`0 0 ${String(largura)} ${String(ALTURA)}`}
        preserveAspectRatio="none"
        role="img"
        aria-label={`Evolução do gasto: ${formatBRL(total)} no período, com o maior dia em ${diaCurto(diaDeMaiorGasto.dia)} (${formatBRL(diaDeMaiorGasto.saiu)}).`}
        style={{ width: "100%", height: 120, display: "block", overflow: "visible" }}
      >
        {pontos.map((ponto, indice) => {
          const altura = alturaDaBarra(ponto.saiu);
          if (altura <= 0) return null;
          return (
            <rect
              key={ponto.dia}
              x={indice + 0.15}
              y={ALTURA - altura}
              width={0.7}
              height={altura}
              fill={ponto.dia > hojeIso ? "var(--line)" : "var(--primary-tint)"}
              rx={0.2}
            />
          );
        })}
        <polyline
          points={linha}
          fill="none"
          stroke="var(--primary)"
          strokeWidth={2}
          strokeLinejoin="round"
          strokeLinecap="round"
          // Sem isto o `preserveAspectRatio="none"` esticaria a espessura do
          // traço junto com o eixo X e a linha ficaria disforme.
          vectorEffect="non-scaling-stroke"
        />
      </svg>

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          gap: 10,
          fontFamily: "var(--sbf-font-sans)",
          fontSize: 11.5,
          color: "var(--text-4)",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        <span>{diaCurto(primeiro.dia)}</span>
        <span style={{ color: "var(--text-3)", fontWeight: 700 }}>
          {`${formatBRL(total)} acumulados`}
        </span>
        <span>{diaCurto(ultimo.dia)}</span>
      </div>

      <div style={{ fontSize: 12, color: "var(--text-3)", lineHeight: 1.45 }}>
        {`Maior saída do período: ${formatBRL(diaDeMaiorGasto.saiu)} em ${diaCurto(diaDeMaiorGasto.dia)}. As barras mostram o gasto de cada dia; a linha, o total acumulado.`}
      </div>
    </div>
  );
}
