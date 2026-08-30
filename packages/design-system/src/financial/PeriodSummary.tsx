import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { MetricLabel } from "../primitives/MetricLabel";
import { MetricValue } from "../primitives/MetricValue";
import { SegmentedBar } from "../primitives/SegmentedBar";

export interface PeriodSummaryProps {
  /** Título. Padrão `Situação do período`. */
  title?: string;
  /** Entrou no ciclo, formatado. */
  income: string;
  /** Já saiu, formatado. */
  spent: string;
  /** Ainda vai sair, formatado. */
  committed: string;
  /** Livre projetado, formatado. */
  projected: string;
  /** Valores brutos que alimentam a barra empilhada. */
  bar?: { income: number; spent: number; committed: number };
  style?: CSSProperties;
}

/**
 * Os quatro números do ciclo, com a barra que os relaciona.
 *
 * A ordem e uma afirmacao de dominio: entrou, saiu, ainda vai sair, livre
 * projetado. São quatro conceitos distintos, e confundi-los e exatamente o
 * erro que o produto existe para eliminar — por isso o gráfico vem depois dos
 * números e nunca no lugar deles.
 */
export function PeriodSummary({
  title = "Situação do período",
  income,
  spent,
  committed,
  projected,
  bar,
  style,
}: PeriodSummaryProps) {
  const cells: Array<{ label: string; value: string; color?: string }> = [
    { label: "Entrou", value: income, color: "var(--success)" },
    { label: "Saiu", value: spent },
    { label: "Ainda vai sair", value: committed, color: "var(--warn)" },
    { label: "Livre projetado", value: projected },
  ];

  return (
    <Card style={{ gap: 16, ...style }}>
      <div
        style={{
          fontSize: 17,
          fontWeight: 700,
          letterSpacing: "-.01em",
          color: "var(--text)",
          fontFamily: "var(--sbf-font-sans)",
        }}
      >
        {title}
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit,minmax(120px,1fr))",
          gap: 12,
        }}
      >
        {cells.map((c) => (
          <div key={c.label}>
            <MetricLabel style={{ letterSpacing: ".07em" }}>{c.label}</MetricLabel>
            <MetricValue size="medium" color={c.color}>
              {c.value}
            </MetricValue>
          </div>
        ))}
      </div>

      {bar ? (
        <SegmentedBar
          total={bar.income}
          segments={[
            { label: "ja pago", value: bar.spent, color: "var(--primary)" },
            { label: "comprometido", value: bar.committed, color: "var(--accent)" },
          ]}
        />
      ) : null}
    </Card>
  );
}
