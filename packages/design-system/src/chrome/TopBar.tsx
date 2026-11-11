import type { CSSProperties } from "react";
import { Button } from "../primitives/Button";
import { Icon } from "../primitives/Icon";

export interface TopBarProps {
  /** Título da tela — `Hoje`, `Movimentacoes`. */
  title: string;
  /** Data por extenso e o ciclo vigente. */
  subtitle?: string;
  /** Período selecionado — `20 ago – 19 set`. */
  period?: string;
  onPeriodClick?: () => void;
  /** Texto do campo de busca em repouso. */
  searchPlaceholder?: string;
  onSearchClick?: () => void;
  /** Rotulo da ação primaria. Padrão `Registrar`. */
  actionLabel?: string;
  onAction?: () => void;
  /** Rotulo do assistente. Padrão `Assistente`. */
  assistantLabel?: string;
  onAssistant?: () => void;
  style?: CSSProperties;
}

/**
 * Cabecalho do desktop.
 *
 * O seletor de período fica aqui, e não dentro de cada tela, porque quase todo
 * número do produto e relativo a um ciclo — deixa-lo global evita que o
 * usuário leia dois cards em janelas de tempo diferentes sem perceber.
 */
export function TopBar({
  title,
  subtitle,
  period,
  onPeriodClick,
  searchPlaceholder = "Buscar lançamento…",
  onSearchClick,
  actionLabel = "Registrar",
  onAction,
  assistantLabel = "Assistente",
  onAssistant,
  style,
}: TopBarProps) {
  const chip: CSSProperties = {
    display: "flex",
    alignItems: "center",
    gap: 8,
    border: "1px solid var(--line)",
    background: "var(--surface)",
    borderRadius: "var(--sbf-radius-pill)",
    padding: "8px 14px",
    fontSize: 13,
    fontWeight: 600,
    color: "var(--text-2)",
    whiteSpace: "nowrap",
    cursor: "pointer",
    fontFamily: "var(--sbf-font-sans)",
  };

  return (
    <header
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        alignItems: "center",
        gap: 14,
        flexWrap: "wrap",
        padding: "16px 24px",
        borderBottom: "1px solid var(--line)",
        background: "var(--header-bg)",
        backdropFilter: "blur(8px)",
        color: "var(--text)",
        ...style,
      }}
    >
      <div style={{ flex: "1 1 240px", minWidth: 0 }}>
        <div style={{ fontSize: 19, fontWeight: 700, letterSpacing: "-.015em" }}>{title}</div>
        {subtitle ? (
          <div
            style={{
              fontSize: 12.5,
              color: "var(--text-3)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {subtitle}
          </div>
        ) : null}
      </div>

      <div
        style={{
          marginLeft: "auto",
          display: "flex",
          alignItems: "center",
          gap: 10,
          flexWrap: "wrap",
        }}
      >
        {period ? (
          <button type="button" onClick={onPeriodClick} style={chip}>
            <Icon name="date_range" size={18} style={{ color: "var(--text-3)" }} />
            {period}
            <Icon name="expand_more" size={18} style={{ color: "var(--text-3)" }} />
          </button>
        ) : null}

        <button
          type="button"
          onClick={onSearchClick}
          style={{
            ...chip,
            color: "var(--text-3)",
            fontWeight: 400,
            flex: "1 1 140px",
            maxWidth: 230,
            overflow: "hidden",
          }}
        >
          <Icon name="search" size={18} />
          {searchPlaceholder}
        </button>

        <Button variant="primary" size="lg" icon="add" onClick={onAction}>
          {actionLabel}
        </Button>
        <Button variant="secondary" size="lg" icon="auto_awesome" onClick={onAssistant}>
          {assistantLabel}
        </Button>
      </div>
    </header>
  );
}
