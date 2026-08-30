import type { CSSProperties } from "react";
import { Icon } from "./Icon";

export interface ToastProps {
  children?: string;
  /** Glifo a esquerda. Padrão `check_circle`. */
  icon?: string;
  style?: CSSProperties;
}

/**
 * Aviso passageiro, em pilula escura.
 *
 * Confirma que algo aconteceu ("Documento vinculado ao lançamento") sem
 * roubar o foco. Nunca comunica erro que exija decisão — isso pede um lugar
 * fixo na tela, não uma mensagem que some.
 */
export function Toast({ children, icon = "check_circle", style }: ToastProps) {
  return (
    <div
      role="status"
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "inline-flex",
        alignItems: "center",
        gap: 9,
        background: "var(--toast-bg)",
        color: "var(--toast-text)",
        borderRadius: "var(--sbf-radius-pill)",
        padding: "12px 20px",
        fontSize: 13.5,
        fontWeight: 600,
        boxShadow: "0 14px 34px -18px rgba(24,35,31,.8)",
        animation: "sbfUp .25s ease both",
        ...style,
      }}
    >
      <Icon name={icon} size={19} style={{ color: "var(--toast-icon)" }} />
      {children}
    </div>
  );
}
