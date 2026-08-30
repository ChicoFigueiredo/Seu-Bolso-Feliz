import type { CSSProperties, ReactNode } from "react";

export interface MetricLabelProps {
  children?: ReactNode;
  /** `onHero` clareia o rotulo para o card verde escuro. */
  variant?: "default" | "onHero";
  style?: CSSProperties;
}

/**
 * Rotulo curto acima de um número — versalete, 11.5px, peso 700.
 *
 * O tracking aberto (`.09em`) e o que permite ler "DISPONIVEL AGORA" num
 * tamanho tao pequeno sem virar mancha.
 */
export function MetricLabel({ children, variant = "default", style }: MetricLabelProps) {
  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        fontSize: 11.5,
        fontWeight: 700,
        letterSpacing: ".09em",
        textTransform: "uppercase",
        color: variant === "onHero" ? "var(--hero-sub)" : "var(--text-3)",
        ...style,
      }}
    >
      {children}
    </div>
  );
}
