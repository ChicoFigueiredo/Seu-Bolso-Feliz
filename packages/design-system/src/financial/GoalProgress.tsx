import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import { ProgressBar } from "../primitives/ProgressBar";

export interface GoalProgressProps {
  /** A meta — `Quitar o emprestimo Caixa`. */
  name: string;
  /** Fracao concluída, de 0 a 1. */
  progress: number;
  /** Quanto falta, formatado. Omitido quando concluída. */
  remaining?: string;
  /** Previsao — `no ritmo atual, set/2027`. */
  forecast?: string;
  /**
   * Meta batida. Troca a barra pela celebração: verde com acento dourado,
   * uma linha, sem confete.
   */
  achieved?: boolean;
  /** O que a conquista significa em número concreto. */
  achievement?: string;
  style?: CSSProperties;
}

/**
 * Progresso de uma meta.
 *
 * A celebração e deliberadamente contida — verde com um acento dourado e uma
 * frase de impacto concreto ("você reduziu R$ 18.400 desta divida em 11
 * meses"). O produto comemora resultado, não engajamento: nada de streak,
 * badge ou confete recorrente.
 */
export function GoalProgress({
  name,
  progress,
  remaining,
  forecast,
  achieved = false,
  achievement,
  style,
}: GoalProgressProps) {
  const pct = Math.max(0, Math.min(1, progress));

  return (
    <Card padding={18} style={{ gap: 12, ...style }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 9,
          fontFamily: "var(--sbf-font-sans)",
        }}
      >
        {achieved ? (
          <Icon name="workspace_premium" size={21} style={{ color: "var(--accent)" }} />
        ) : null}
        <div style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)", flex: "1 1 auto" }}>
          {name}
        </div>
      </div>

      {achieved ? (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 6,
            background: "var(--primary-soft)",
            borderLeft: "3px solid var(--accent)",
            borderRadius: "var(--sbf-radius-sm)",
            padding: "12px 14px",
            fontFamily: "var(--sbf-font-sans)",
          }}
        >
          <div style={{ fontSize: 13.5, fontWeight: 800, color: "var(--primary-ink)" }}>
            Meta concluída
          </div>
          {achievement ? (
            <div style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.45 }}>
              {achievement}
            </div>
          ) : null}
        </div>
      ) : (
        <>
          <ProgressBar
            value={pct}
            size="md"
            label={`${Math.round(pct * 100)}%`}
            hint={remaining ? `faltam ${remaining}` : undefined}
          />
          {forecast ? (
            <div
              style={{ fontSize: 12.5, color: "var(--text-3)", fontFamily: "var(--sbf-font-sans)" }}
            >
              {forecast}
            </div>
          ) : null}
        </>
      )}
    </Card>
  );
}
