import type { CSSProperties } from "react";
import { Avatar } from "../primitives/Avatar";
import { Icon } from "../primitives/Icon";

export interface MobileHeaderProps {
  /** Título da tela. */
  title: string;
  /** Saudacao acima do título — `Bom dia, Chico`. */
  greeting?: string;
  /** Nome do usuário; vira as iniciais do avatar. */
  userName: string;
  onAssistant?: () => void;
  style?: CSSProperties;
}

/**
 * Cabecalho do mobile.
 *
 * Sem busca e sem seletor de período — no celular esses controles moram nas
 * telas que precisam deles. Fica só o que sustenta orientacao (onde estou) e o
 * atalho para o assistente.
 */
export function MobileHeader({ title, greeting, userName, onAssistant, style }: MobileHeaderProps) {
  return (
    <header
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "18px 20px 12px",
        background: "var(--bg)",
        color: "var(--text)",
        ...style,
      }}
    >
      <div style={{ flex: "1 1 auto", minWidth: 0 }}>
        {greeting ? (
          <div style={{ fontSize: 12.5, color: "var(--text-3)", fontWeight: 600 }}>{greeting}</div>
        ) : null}
        <div style={{ fontSize: 21, fontWeight: 800, letterSpacing: "-.02em" }}>{title}</div>
      </div>

      <button
        type="button"
        onClick={onAssistant}
        aria-label="Abrir assistente"
        style={{
          width: 40,
          height: 40,
          borderRadius: "var(--sbf-radius-pill)",
          border: "1px solid var(--secondary-line)",
          background: "var(--secondary-soft)",
          color: "var(--secondary-ink)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          cursor: "pointer",
        }}
      >
        <Icon name="auto_awesome" size={21} />
      </button>

      <Avatar name={userName} size={40} />
    </header>
  );
}
