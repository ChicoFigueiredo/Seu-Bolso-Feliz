import type { CSSProperties, ReactNode } from "react";
import { Icon } from "./Icon";

export type ButtonVariant = "primary" | "secondary" | "soft" | "outline" | "quiet" | "onHero";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps {
  children?: ReactNode;
  /**
   * - `primary` — a ação da tela. Uma por vista.
   * - `secondary` — assistente e caminhos de informação (azul ardosia).
   * - `soft` — confirmação de baixo atrito, tipo "Paguei" numa linha.
   * - `outline` — alternativa neutra ao lado de uma ação de verdade.
   * - `quiet` — texto puro, sem caixa.
   * - `onHero` — sobre o card verde escuro; nenhuma outra funciona ali.
   */
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Glifo Material Symbols antes do rotulo. */
  icon?: string;
  /** Ocupa toda a largura disponível. */
  block?: boolean;
  disabled?: boolean;
  type?: "button" | "submit" | "reset";
  onClick?: () => void;
  "aria-label"?: string;
  style?: CSSProperties;
}

const variants: Record<ButtonVariant, CSSProperties> = {
  primary: {
    background: "var(--primary)",
    color: "var(--on-primary)",
    border: "1px solid transparent",
  },
  secondary: {
    background: "var(--secondary-soft)",
    color: "var(--secondary-ink)",
    border: "1px solid var(--secondary-line)",
  },
  soft: {
    background: "var(--primary-soft)",
    color: "var(--primary-ink)",
    border: "1px solid var(--primary-line)",
  },
  outline: {
    background: "var(--surface)",
    color: "var(--text-3)",
    border: "1px solid var(--line)",
  },
  quiet: { background: "transparent", color: "var(--text-3)", border: "1px solid transparent" },
  onHero: {
    background: "transparent",
    color: "var(--hero-accent)",
    border: "1px solid rgba(245,239,217,.35)",
  },
};

const sizes: Record<ButtonSize, CSSProperties & { iconSize: number }> = {
  sm: { padding: "6px 12px", fontSize: 12, minHeight: 32, iconSize: 16 },
  md: { padding: "8px 14px", fontSize: 12.5, minHeight: 38, iconSize: 18 },
  lg: { padding: "10px 16px", fontSize: 13.5, minHeight: 44, iconSize: 18 },
};

/**
 * Botao em pilula — a única forma de botao do sistema.
 *
 * Cantos arredondados totais e peso 700 são o que faz uma ação parecer
 * convidativa em vez de burocratica; e a razao de não existir variante
 * retangular aqui.
 */
export function Button({
  children,
  variant = "primary",
  size = "md",
  icon,
  block = false,
  disabled = false,
  type = "button",
  onClick,
  style,
  ...rest
}: ButtonProps) {
  const { iconSize, ...sizeStyle } = sizes[size];
  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      aria-label={rest["aria-label"]}
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: block ? "flex" : "inline-flex",
        width: block ? "100%" : undefined,
        alignItems: "center",
        justifyContent: "center",
        gap: 7,
        borderRadius: "var(--sbf-radius-pill)",
        fontWeight: 700,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.5 : 1,
        transition: "background var(--sbf-motion-fast) var(--sbf-ease)",
        whiteSpace: "nowrap",
        ...variants[variant],
        ...sizeStyle,
        ...style,
      }}
    >
      {icon ? <Icon name={icon} size={iconSize} /> : null}
      {children}
    </button>
  );
}
