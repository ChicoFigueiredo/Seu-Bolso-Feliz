import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import { Pill } from "../primitives/Pill";

export interface RecurringItemProps {
  /** Nome do servico ou conta — `Hetzner`, `Energia CEMIG`. */
  name: string;
  /** Média historica, formatada. */
  average: string;
  /** Proximo valor previsto, formatado. */
  next?: string;
  /** Quando cai — `todo dia 10`. */
  cadence?: string;
  /** Conta ou cartao onde costuma cair. */
  source?: string;
  /**
   * Variação contra a média, em fracao (`0.34` = 34% acima). Positiva sai em
   * ambar; negativa, em verde.
   */
  variation?: number;
  /** Detectada pela IA e ainda não confirmada pelo usuário. */
  suggested?: boolean;
  onConfirm?: () => void;
  style?: CSSProperties;
}

/**
 * Uma recorrência.
 *
 * Nunca chamada de "template" na interface — o usuário reconhece "assinatura"
 * é "conta que chega todo mes", não o nome da tabela.
 *
 * A variação contra a média e o sinal mais útil da tela: e o que revela uma
 * assinatura que subiu de preco sem aviso.
 */
export function RecurringItem({
  name,
  average,
  next,
  cadence,
  source,
  variation,
  suggested = false,
  onConfirm,
  style,
}: RecurringItemProps) {
  const up = typeof variation === "number" && variation > 0;
  const meta = [cadence, source].filter(Boolean).join(" · ");

  return (
    <Card padding={16} style={{ borderRadius: "var(--sbf-radius-lg)", gap: 10, ...style }}>
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: 10,
          fontFamily: "var(--sbf-font-sans)",
        }}
      >
        <div style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 7, flexWrap: "wrap" }}>
            <span style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>{name}</span>
            {suggested ? (
              <Pill tone="secondary" size="xs" icon="auto_awesome">
                detectada
              </Pill>
            ) : null}
          </div>
          {meta ? (
            <div style={{ fontSize: 12.5, color: "var(--text-3)", marginTop: 2 }}>{meta}</div>
          ) : null}
        </div>
        <div style={{ textAlign: "right" }}>
          <div style={{ fontSize: 11.5, color: "var(--text-3)", fontWeight: 600 }}>Media</div>
          <div
            style={{
              fontSize: 17,
              fontWeight: 800,
              fontVariantNumeric: "tabular-nums",
              color: "var(--text)",
            }}
          >
            {average}
          </div>
        </div>
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          flexWrap: "wrap",
          fontSize: 12.5,
          fontFamily: "var(--sbf-font-sans)",
        }}
      >
        {next ? <span style={{ color: "var(--text-3)" }}>Proximo previsto {next}</span> : null}
        {typeof variation === "number" && variation !== 0 ? (
          <span
            style={{
              display: "flex",
              alignItems: "center",
              gap: 4,
              fontWeight: 700,
              color: up ? "var(--warn)" : "var(--success)",
            }}
          >
            <Icon name={up ? "trending_up" : "trending_down"} size={16} />
            {up ? "+" : ""}
            {Math.round(variation * 100)}% vs média
          </span>
        ) : null}
      </div>

      {suggested ? (
        <button
          type="button"
          onClick={onConfirm}
          style={{
            alignSelf: "flex-start",
            fontFamily: "var(--sbf-font-sans)",
            border: "1px solid var(--primary-line)",
            background: "var(--primary-soft)",
            color: "var(--primary-ink)",
            borderRadius: "var(--sbf-radius-pill)",
            padding: "7px 13px",
            fontSize: 12.5,
            fontWeight: 700,
            cursor: "pointer",
          }}
        >
          Acompanhar como recorrente
        </button>
      ) : null}
    </Card>
  );
}
