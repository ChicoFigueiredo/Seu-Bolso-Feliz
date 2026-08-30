import type { CSSProperties } from "react";

export interface ProgressBarProps {
  /** Fracao preenchida, de 0 a 1. Valores fora da faixa são aparados. */
  value: number;
  /** Altura do trilho. `sm` 7px (dentro de card), `md` 12px (destaque). */
  size?: "sm" | "md";
  /** Cor do preenchimento. Padrão `var(--primary)`. */
  fill?: string;
  /** Rotulo a esquerda, sob a barra. */
  label?: string;
  /** Rotulo a direita, sob a barra. */
  hint?: string;
  /**
   * Cor do rotulo da esquerda. Padrão `var(--primary-ink)`. Quando a barra usa
   * outro pigmento (limite de cartao em `--secondary`), passe o `-ink`
   * correspondente — rotulo verde sobre barra azul le como dois assuntos.
   */
  labelColor?: string;
  style?: CSSProperties;
}

/**
 * Barra de progresso.
 *
 * Usada para quitacao de divida e limite de cartao. O produto trata progresso
 * como boa noticia: por isso o padrão e verde, e não ha variante vermelha —
 * "faltar" não e falha.
 */
export function ProgressBar({
  value,
  size = "sm",
  fill = "var(--primary)",
  label,
  hint,
  labelColor = "var(--primary-ink)",
  style,
}: ProgressBarProps) {
  const pct = Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
  const height = size === "md" ? 12 : 7;
  return (
    <div style={{ fontFamily: "var(--sbf-font-sans)", ...style }}>
      <div
        role="progressbar"
        aria-valuenow={Math.round(pct * 100)}
        aria-valuemin={0}
        aria-valuemax={100}
        style={{
          height,
          borderRadius: "var(--sbf-radius-pill)",
          background: "var(--line-soft)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            height: "100%",
            width: `${pct * 100}%`,
            background: fill,
            borderRadius: "var(--sbf-radius-pill)",
          }}
        />
      </div>
      {label || hint ? (
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 10,
            marginTop: size === "md" ? 7 : 6,
            fontSize: 12.5,
          }}
        >
          <span style={{ fontWeight: 700, color: labelColor }}>{label}</span>
          <span style={{ color: "var(--text-3)" }}>{hint}</span>
        </div>
      ) : null}
    </div>
  );
}
