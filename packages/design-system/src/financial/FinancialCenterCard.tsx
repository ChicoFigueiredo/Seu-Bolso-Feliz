import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { MetricValue } from "../primitives/MetricValue";

export interface FinancialCenterCardProps {
  /** Nome do centro — `Edulingo`, `Imob Max`, `Daiane`. */
  name: string;
  /** Saldo acumulado do centro, formatado. */
  balance: string;
  /** Cor de identidade do centro. Padrão `var(--primary)`. */
  color?: string;
  /** Movimento do período — `saiu R$ 1.240 · entrou R$ 3.000`. */
  meta?: string;
  /** Saldo negativo pinta o número de vermelho. */
  negative?: boolean;
  onClick?: () => void;
  style?: CSSProperties;
}

/**
 * Centro financeiro como conta corrente virtual.
 *
 * Um centro (projeto, pessoa, frente) tem entradas, saidas e saldo proprios,
 * derivados de alocações sobre os lançamentos reais — nunca de lançamentos
 * duplicados. O quadradinho colorido e a identidade do centro, repetida em
 * gráfico e alocação para que a leitura seja a mesma em toda parte.
 */
export function FinancialCenterCard({
  name,
  balance,
  color = "var(--primary)",
  meta,
  negative = false,
  onClick,
  style,
}: FinancialCenterCardProps) {
  return (
    <Card
      padding={15}
      onClick={onClick}
      style={{ borderRadius: "var(--sbf-radius-lg)", gap: 7, minHeight: 104, ...style }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span
          style={{ width: 10, height: 10, borderRadius: 4, background: color, flex: "0 0 10px" }}
        />
        <span style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)" }}>{name}</span>
      </div>
      <MetricValue
        size="medium"
        style={{ fontSize: 19 }}
        color={negative ? "var(--danger)" : undefined}
      >
        {balance}
      </MetricValue>
      {meta ? (
        <div style={{ fontSize: 12, color: "var(--text-3)", fontFamily: "var(--sbf-font-sans)" }}>
          {meta}
        </div>
      ) : null}
    </Card>
  );
}
