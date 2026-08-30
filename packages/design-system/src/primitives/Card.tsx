import type { CSSProperties, ReactNode } from "react";
import { baseType } from "../tokens";

export interface CardProps {
  children?: ReactNode;
  /**
   * - `surface` — o card padrão: branco, borda fina, raio 22.
   * - `hero` — verde profundo, para o número mais importante da tela.
   * - `quiet` — fundo `--muted` sem borda, para blocos de apoio.
   * - `flush` — sem padding, para listas que desenham as proprias linhas.
   */
  variant?: "surface" | "hero" | "quiet" | "flush";
  padding?: number;
  /** Vira `<button>` clicavel com realce de borda no hover. */
  onClick?: () => void;
  className?: string;
  style?: CSSProperties;
}

/**
 * Superficie base do sistema.
 *
 * Raio 22 e sombra quase invisivel (`--sbf-shadow-card`) são deliberados: o
 * produto trata dinheiro, e um card duro e pesado le como cobranca bancaria.
 */
export function Card({
  children,
  variant = "surface",
  padding,
  onClick,
  className,
  style,
}: CardProps) {
  const base: CSSProperties = {
    ...baseType,
    borderRadius: "var(--sbf-radius-xl)",
    display: "flex",
    flexDirection: "column",
    gap: 12,
    padding: padding ?? (variant === "flush" ? 0 : 20),
    textAlign: "left",
    boxSizing: "border-box",
  };

  const skins: Record<string, CSSProperties> = {
    surface: {
      background: "var(--surface)",
      border: "1px solid var(--line)",
      boxShadow: "var(--sbf-shadow-card)",
    },
    hero: {
      background: "var(--hero-bg)",
      color: "var(--hero-soft)",
      border: "1px solid transparent",
    },
    quiet: { background: "var(--muted)", border: "1px solid transparent" },
    flush: {
      background: "var(--surface)",
      border: "1px solid var(--line)",
      overflow: "hidden",
      gap: 0,
    },
  };

  const merged = { ...base, ...skins[variant], ...style };
  // A classe sbf-flush deixa tokens.css apagar o borderTop da primeira linha:
  // sem isso a borda do card e o separador da linha 1 desenham um traco duplo.
  const classes =
    [variant === "flush" ? "sbf-flush" : null, className].filter(Boolean).join(" ") || undefined;

  if (onClick) {
    return (
      <button
        type="button"
        onClick={onClick}
        className={classes}
        style={{ ...merged, cursor: "pointer", width: "100%" }}
      >
        {children}
      </button>
    );
  }
  return (
    <div className={classes} style={merged}>
      {children}
    </div>
  );
}
