import type { CSSProperties } from "react";

export interface ChipOption {
  /** Chave estavel — o valor devolvido em `onChange`. */
  value: string;
  label: string;
  /** Contador opcional a direita do rotulo. */
  count?: number;
}

export interface ChipGroupProps {
  options: ChipOption[];
  /** Valor selecionado. */
  value?: string;
  onChange?: (value: string) => void;
  /** Rolagem horizontal em vez de quebra de linha — o comportamento no mobile. */
  scroll?: boolean;
  style?: CSSProperties;
}

/**
 * Filtros rapidos em pilula — `Todas`, `Pagas`, `Pendentes`, `Cartao`.
 *
 * No mobile a fila rola na horizontal em vez de quebrar, para não empurrar o
 * conteudo da tela para baixo.
 */
export function ChipGroup({ options, value, onChange, scroll = false, style }: ChipGroupProps) {
  return (
    <div
      style={{
        fontFamily: "var(--sbf-font-sans)",
        display: "flex",
        gap: 8,
        ...(scroll ? { overflowX: "auto", paddingBottom: 2 } : { flexWrap: "wrap" }),
        ...style,
      }}
    >
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            onClick={() => onChange?.(o.value)}
            aria-pressed={active}
            style={{
              fontFamily: "inherit",
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              flex: "0 0 auto",
              borderRadius: "var(--sbf-radius-pill)",
              padding: "8px 14px",
              minHeight: 38,
              fontSize: 12.5,
              fontWeight: 700,
              cursor: "pointer",
              whiteSpace: "nowrap",
              background: active ? "var(--primary)" : "var(--surface)",
              color: active ? "var(--on-primary)" : "var(--text-3)",
              border: `1px solid ${active ? "var(--primary)" : "var(--line)"}`,
            }}
          >
            {o.label}
            {typeof o.count === "number" ? (
              <span style={{ fontWeight: 800, opacity: 0.75 }}>{o.count}</span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
