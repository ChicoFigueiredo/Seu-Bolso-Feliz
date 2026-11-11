import type { CSSProperties, ReactNode } from "react";
import { Icon } from "./Icon";
import { type SbfTone, toneStyles } from "../tokens";

export interface PillProps {
  children?: ReactNode;
  /** Papel semântico. Ver `SbfTone`. */
  tone?: SbfTone;
  /** Glifo antes do rotulo — o segundo canal, para status não depender de cor. */
  icon?: string;
  /** `outline` troca o preenchimento por contorno. */
  variant?: "solid" | "outline";
  size?: "xs" | "sm";
  style?: CSSProperties;
}

/**
 * Etiqueta de status.
 *
 * O sistema exige que estado nunca seja comunicado só por cor (WCAG 2.2 AA):
 * o texto da pilula já e o segundo canal, é `icon` da um terceiro. Por isso
 * não existe pilula sem rotulo.
 *
 * Vocabulario em uso no produto: `estimado`, `vence amanha`, `caindo`,
 * `Pago`, `Precisa revisar`, `Possível duplicidade`, `Protegido por senha`.
 */
export function Pill({
  children,
  tone = "neutral",
  icon,
  variant = "solid",
  size = "sm",
  style,
}: PillProps) {
  const t = toneStyles[tone];
  const solid = variant === "solid";
  return (
    <span
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        borderRadius: "var(--sbf-radius-pill)",
        fontWeight: 700,
        fontSize: size === "xs" ? 10.5 : 11.5,
        padding: size === "xs" ? "2px 7px" : "4px 10px",
        whiteSpace: "nowrap",
        background: solid ? t.bg : "transparent",
        color: t.fg,
        border: `1px solid ${solid ? "transparent" : t.line}`,
        ...style,
      }}
    >
      {icon ? <Icon name={icon} size={size === "xs" ? 13 : 15} /> : null}
      {children}
    </span>
  );
}
