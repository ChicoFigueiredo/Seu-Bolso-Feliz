import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { MetricValue } from "../primitives/MetricValue";
import { ProgressBar } from "../primitives/ProgressBar";

export interface DebtProgressCardProps {
  /** Nome da divida — `Emprestimo Caixa`. */
  name: string;
  /** Linha de contexto: credor, origem, data de inicio. */
  meta?: string;
  /** Saldo devedor formatado. */
  balance: string;
  /** Fracao já quitada, de 0 a 1. */
  paid: number;
  /** Parcelas — `18 de 48 pagas`. */
  installments?: string;
  /** Fatos secundarios em pilula neutra: próxima parcela, taxa, previsao de fim. */
  facts?: string[];
  style?: CSSProperties;
}

/**
 * Divida com progresso de quitacao.
 *
 * O card e construido para comunicar avanco, não culpa: a barra mostra o
 * quanto já foi quitado (não o quanto falta), o rotulo diz "X% quitado", e
 * taxa e prazo ficam em pilulas neutras — presentes para quem procura, sem
 * dramatizar.
 */
export function DebtProgressCard({
  name,
  meta,
  balance,
  paid,
  installments,
  facts = [],
  style,
}: DebtProgressCardProps) {
  const pct = Math.max(0, Math.min(1, paid));
  return (
    <Card style={{ gap: 14, ...style }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12, flexWrap: "wrap" }}>
        <div style={{ flex: "1 1 200px", minWidth: 0 }}>
          <div
            style={{ fontSize: 16, fontWeight: 800, letterSpacing: "-.01em", color: "var(--text)" }}
          >
            {name}
          </div>
          {meta ? <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>{meta}</div> : null}
        </div>
        <div style={{ textAlign: "right" }}>
          <div style={{ fontSize: 11.5, color: "var(--text-3)", fontWeight: 600 }}>Falta pagar</div>
          <MetricValue size="strong" style={{ fontSize: 22 }}>
            {balance}
          </MetricValue>
        </div>
      </div>

      <ProgressBar
        value={pct}
        size="md"
        label={`${Math.round(pct * 100)}% quitado`}
        hint={installments}
      />

      {facts.length > 0 ? (
        <div
          style={{
            display: "flex",
            gap: 10,
            flexWrap: "wrap",
            fontSize: 12.5,
            color: "var(--text-3)",
            fontFamily: "var(--sbf-font-sans)",
          }}
        >
          {facts.map((f) => (
            <span
              key={f}
              style={{
                background: "var(--muted)",
                borderRadius: "var(--sbf-radius-pill)",
                padding: "5px 11px",
              }}
            >
              {f}
            </span>
          ))}
        </div>
      ) : null}
    </Card>
  );
}
