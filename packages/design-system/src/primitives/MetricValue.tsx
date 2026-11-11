import type { CSSProperties, ReactNode } from "react";
import { tabular } from "../tokens";

export interface MetricValueProps {
  children?: ReactNode;
  /**
   * - `hero` — Instrument Serif 40px. Reservado ao número mais importante.
   *   Não fixa cor: sobre o card verde, passe `color="var(--hero-text)"`.
   * - `display` — Instrument Serif 38px, sobre superficie clara.
   * - `strong` — sans 24px peso 800. O número de um card comum.
   * - `medium` — sans 20px peso 800. Numeros de um grupo de quatro.
   * - `compact` — sans 15px peso 800. Valor dentro de uma linha de lista.
   */
  size?: "hero" | "display" | "strong" | "medium" | "compact";
  /** Cor explicita; por padrão herda. Use tokens (`var(--success)`). */
  color?: string;
  style?: CSSProperties;
}

const sizes: Record<string, CSSProperties> = {
  hero: { fontFamily: "var(--sbf-font-serif)", fontSize: 40, lineHeight: 1 },
  display: { fontFamily: "var(--sbf-font-serif)", fontSize: 38, lineHeight: 1 },
  strong: {
    fontFamily: "var(--sbf-font-sans)",
    fontSize: 24,
    fontWeight: 800,
    letterSpacing: "-.02em",
  },
  medium: { fontFamily: "var(--sbf-font-sans)", fontSize: 20, fontWeight: 800 },
  compact: { fontFamily: "var(--sbf-font-sans)", fontSize: 15, fontWeight: 800 },
};

/**
 * Um número de dinheiro.
 *
 * Os dois tamanhos grandes usam Instrument Serif — a serifa da ao saldo um ar
 * de extrato calmo em vez de painel de risco. Todos usam `tabular-nums`, então
 * uma coluna de valores alinha por digito.
 */
export function MetricValue({ children, size = "strong", color, style }: MetricValueProps) {
  return (
    <div style={{ ...tabular, ...sizes[size], ...(color ? { color } : {}), ...style }}>
      {children}
    </div>
  );
}
