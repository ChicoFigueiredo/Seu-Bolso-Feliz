import type { CSSProperties } from "react";
import { Icon } from "../primitives/Icon";

export interface BottomNavTab {
  id: string;
  label: string;
  icon: string;
}

export interface BottomNavProps {
  /** As abas. Quatro cabem confortavelmente; cinco já aperta. */
  tabs: BottomNavTab[];
  active?: string;
  onNavigate?: (id: string) => void;
  /** Botao central de ação. Some quando `false`. */
  fab?: boolean;
  /** Glifo do botao central. Padrão `add`. */
  fabIcon?: string;
  onFab?: () => void;
  style?: CSSProperties;
}

/**
 * Navegação inferior do mobile, com a ação central elevada.
 *
 * O `+` sobe 18px acima da barra e ganha sombra própria porque registrar um
 * gasto ou fotografar um comprovante e a ação mais frequente no celular — ela
 * precisa cair sob o polegar sem procura.
 *
 * O FAB e inserido no meio da lista de abas, então um número par de abas
 * mantem a simetria.
 */
export function BottomNav({
  tabs,
  active,
  onNavigate,
  fab = true,
  fabIcon = "add",
  onFab,
  style,
}: BottomNavProps) {
  const half = Math.ceil(tabs.length / 2);
  const left = fab ? tabs.slice(0, half) : tabs;
  const right = fab ? tabs.slice(half) : [];

  const tabButton = (t: BottomNavTab) => {
    const isActive = t.id === active;
    return (
      <button
        key={t.id}
        type="button"
        onClick={() => onNavigate?.(t.id)}
        style={{
          flex: "1 1 0",
          minWidth: 0,
          border: 0,
          background: "transparent",
          cursor: "pointer",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 3,
          padding: "6px 2px",
          minHeight: 48,
          fontFamily: "var(--sbf-font-sans)",
          color: isActive ? "var(--primary)" : "var(--text-3)",
        }}
      >
        <Icon name={t.icon} size={23} filled={isActive} />
        <span style={{ fontSize: 11, fontWeight: 700, letterSpacing: "-.01em" }}>{t.label}</span>
      </button>
    );
  };

  return (
    <nav
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-around",
        gap: 2,
        background: "var(--nav-bg)",
        backdropFilter: "blur(10px)",
        borderTop: "1px solid var(--line)",
        padding: "8px 10px 14px",
        fontFamily: "var(--sbf-font-sans)",
        ...style,
      }}
    >
      {left.map(tabButton)}

      {fab ? (
        <button
          type="button"
          onClick={onFab}
          aria-label="Nova ação"
          style={{
            width: 56,
            height: 56,
            flex: "0 0 56px",
            borderRadius: "var(--sbf-radius-pill)",
            border: 0,
            background: "var(--primary)",
            color: "var(--on-primary)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "pointer",
            boxShadow: "0 8px 20px -8px rgba(36,92,74,.7)",
            marginTop: -18,
          }}
        >
          <Icon name={fabIcon} size={28} />
        </button>
      ) : null}

      {right.map(tabButton)}
    </nav>
  );
}
