import type { CSSProperties } from "react";
import { Icon } from "../primitives/Icon";

export interface SidebarNavItem {
  /** Chave estavel — comparada com `active`. */
  id: string;
  label: string;
  /** Glifo Material Symbols. */
  icon: string;
  /** Contador discreto a direita — pendências daquela area. */
  badge?: number | string;
}

export interface AppSidebarProps {
  /** Navegação financeira principal, na ordem da arquitetura de informação. */
  items: SidebarNavItem[];
  /** `id` do item ativo. */
  active?: string;
  onNavigate?: (id: string) => void;
  /** Itens do rodape — Conexoes, Configuracoes. */
  secondary?: SidebarNavItem[];
  /** Estado do worker local, fixado ao pe da barra. */
  workerName?: string;
  workerStatus?: string;
  /** Worker offline pinta o ponto de cinza em vez de verde. */
  workerOnline?: boolean;
  style?: CSSProperties;
}

/**
 * Barra lateral do desktop.
 *
 * A ordem dos itens e uma decisão de produto: Hoje, Movimentacoes, Contas &
 * Cartoes, Dividas, Recorrentes, Projetos & Centros, Documentos,
 * Inteligencia. Rotas tecnicas de ingestao não ocupam a navegação financeira
 * principal — elas vivem sob Conexoes.
 *
 * O bloco do worker local no rodape existe porque o computador que varre
 * pastas e parte do produto, não um detalhe de infraestrutura.
 */
export function AppSidebar({
  items,
  active,
  onNavigate,
  secondary = [],
  workerName,
  workerStatus,
  workerOnline = true,
  style,
}: AppSidebarProps) {
  const row = (item: SidebarNavItem, isActive: boolean, small = false): CSSProperties => ({
    display: "flex",
    alignItems: "center",
    gap: 11,
    width: "100%",
    border: 0,
    cursor: "pointer",
    textAlign: "left",
    padding: small ? "9px 12px" : "10px 12px",
    borderRadius: "var(--sbf-radius-md)",
    fontSize: small ? 13.5 : 14,
    fontWeight: 600,
    fontFamily: "var(--sbf-font-sans)",
    background: isActive ? "var(--primary-soft)" : "transparent",
    color: isActive ? "var(--primary-ink)" : small ? "var(--text-3)" : "var(--text-2)",
    transition: "background var(--sbf-motion-fast) var(--sbf-ease)",
  });

  return (
    <aside
      style={{
        fontFamily: "var(--sbf-font-sans)",
        width: 252,
        flex: "0 0 252px",
        borderRight: "1px solid var(--line)",
        background: "var(--surface-2)",
        padding: "22px 14px",
        display: "flex",
        flexDirection: "column",
        gap: 22,
        color: "var(--text)",
        ...style,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "0 8px" }}>
        <div
          style={{
            width: 34,
            height: 34,
            borderRadius: 11,
            background: "var(--hero-bg)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--hero-accent)",
            fontWeight: 800,
            fontSize: 15,
          }}
        >
          sb
        </div>
        <div style={{ lineHeight: 1.15 }}>
          <div style={{ fontWeight: 800, fontSize: 14, letterSpacing: "-.01em" }}>
            Seu Bolso Feliz
          </div>
          <div style={{ fontSize: 11.5, color: "var(--text-3)" }}>Central financeira</div>
        </div>
      </div>

      <nav style={{ display: "flex", flexDirection: "column", gap: 3 }}>
        {items.map((item) => {
          const isActive = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onNavigate?.(item.id)}
              style={row(item, isActive)}
            >
              <Icon
                name={item.icon}
                size={20}
                style={{ color: isActive ? "var(--primary)" : "var(--text-3)" }}
              />
              <span>{item.label}</span>
              {item.badge ? (
                <span
                  style={{
                    marginLeft: "auto",
                    fontSize: 11,
                    fontWeight: 700,
                    background: "var(--accent-soft)",
                    color: "var(--accent-ink)",
                    borderRadius: "var(--sbf-radius-pill)",
                    padding: "2px 7px",
                  }}
                >
                  {item.badge}
                </span>
              ) : null}
            </button>
          );
        })}
      </nav>

      <div
        style={{
          marginTop: "auto",
          display: "flex",
          flexDirection: "column",
          gap: 3,
          borderTop: "1px solid var(--line)",
          paddingTop: 14,
        }}
      >
        {secondary.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => onNavigate?.(item.id)}
            style={row(item, item.id === active, true)}
          >
            <Icon name={item.icon} size={19} />
            <span>{item.label}</span>
          </button>
        ))}

        {workerName ? (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 9,
              padding: "11px 12px",
              marginTop: 6,
              background: "var(--secondary-soft)",
              borderRadius: "var(--sbf-radius-md)",
            }}
          >
            <span
              style={{
                width: 8,
                height: 8,
                flex: "0 0 8px",
                borderRadius: "var(--sbf-radius-pill)",
                background: workerOnline ? "var(--success)" : "var(--text-4)",
              }}
            />
            <div style={{ fontSize: 11.5, color: "var(--secondary-text)", lineHeight: 1.35 }}>
              {workerName}
              {workerStatus ? (
                <>
                  <br />
                  <span style={{ color: "var(--text-3)" }}>{workerStatus}</span>
                </>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>
    </aside>
  );
}
