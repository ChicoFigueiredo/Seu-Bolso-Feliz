import type { CSSProperties, ReactNode } from "react";

export interface SectionTitleProps {
  children?: ReactNode;
  /** Rotulo do link a direita — normalmente "Ver tudo". */
  actionLabel?: string;
  onAction?: () => void;
  style?: CSSProperties;
}

/**
 * Título de bloco com um atalho opcional a direita.
 *
 * O link a direita e como o produto cumpre "progressive disclosure": o card
 * mostra o suficiente, é "Ver tudo" leva ao detalhe sem inchar a tela.
 */
export function SectionTitle({ children, actionLabel, onAction, style }: SectionTitleProps) {
  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        alignItems: "baseline",
        justifyContent: "space-between",
        gap: 10,
        ...style,
      }}
    >
      <div style={{ fontSize: 17, fontWeight: 700, letterSpacing: "-.01em", color: "var(--text)" }}>
        {children}
      </div>
      {actionLabel ? (
        <button
          type="button"
          onClick={onAction}
          style={{
            border: 0,
            background: "transparent",
            padding: 0,
            cursor: "pointer",
            fontFamily: "inherit",
            fontSize: 12.5,
            fontWeight: 700,
            color: "var(--primary)",
          }}
        >
          {actionLabel}
        </button>
      ) : null}
    </div>
  );
}
