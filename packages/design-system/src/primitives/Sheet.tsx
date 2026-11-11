import type { CSSProperties, ReactNode } from "react";
import { Icon } from "./Icon";

export interface SheetProps {
  title: string;
  children?: ReactNode;
  onClose?: () => void;
  /** Desenha o veu escuro atras. Desligue ao renderizar dentro de um card. */
  withScrim?: boolean;
  style?: CSSProperties;
}

/**
 * Folha inferior — o recipiente de ação do mobile.
 *
 * Substitui o dialogo centrado de proposito: no celular a mao alcanca a base
 * da tela, e a folha mantem o contexto visível atras dela. A alca no topo
 * anuncia que da para arrastar para fechar.
 */
export function Sheet({ title, children, onClose, withScrim = true, style }: SheetProps) {
  return (
    <div
      style={{
        position: "relative",
        width: "100%",
        minHeight: 260,
        display: "flex",
        alignItems: "flex-end",
        justifyContent: "center",
      }}
    >
      {withScrim ? (
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: "rgba(24,35,31,.35)",
            animation: "sbfFade .2s ease both",
          }}
        />
      ) : null}
      <div
        role="dialog"
        aria-label={title}
        style={{
          position: "relative",
          width: "100%",
          maxWidth: 520,
          background: "var(--surface)",
          borderRadius: "26px 26px 0 0",
          padding: "20px 20px 26px",
          boxShadow: "var(--sbf-shadow-sheet)",
          animation: "sbfSheet var(--sbf-motion-base) cubic-bezier(.2,.8,.2,1) both",
          display: "flex",
          flexDirection: "column",
          gap: 14,
          fontFamily: "var(--sbf-font-sans)",
          color: "var(--text)",
          ...style,
        }}
      >
        <div
          style={{
            width: 44,
            height: 5,
            borderRadius: "var(--sbf-radius-pill)",
            background: "var(--line)",
            alignSelf: "center",
          }}
        />
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <div
            style={{ fontSize: 17, fontWeight: 800, letterSpacing: "-.015em", flex: "1 1 auto" }}
          >
            {title}
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Fechar"
            style={{
              border: 0,
              background: "var(--muted)",
              borderRadius: "var(--sbf-radius-pill)",
              width: 34,
              height: 34,
              cursor: "pointer",
              color: "var(--text-2)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <Icon name="close" size={20} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
