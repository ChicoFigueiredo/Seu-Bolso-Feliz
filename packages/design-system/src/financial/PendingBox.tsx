import type { CSSProperties } from "react";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";
import type { SbfTone } from "../tokens";
import { toneStyles } from "../tokens";

export interface PendingItem {
  /** O que falta — `Sem comprovante`, `Possível duplicidade`. */
  label: string;
  /** Uma frase dizendo por que isso importa. */
  hint?: string;
  count: number;
  /** Urgencia do item. O ponto colorido segue este tom. */
  tone?: SbfTone;
}

export interface PendingBoxProps {
  items: PendingItem[];
  /** Título. Padrão `Pendências`. */
  title?: string;
  /** Aberto por padrão? A tela Hoje abre fechado, para não pesar. */
  open?: boolean;
  onToggle?: () => void;
  onSelect?: (item: PendingItem) => void;
  style?: CSSProperties;
}

/**
 * Caixa recolhivel de pendências.
 *
 * Recolhida por padrão de proposito: a lista de tudo que falta e útil quando
 * procurada e ansiogena quando imposta. O contador no título da a dimensao
 * sem obrigar a encarar a lista.
 */
export function PendingBox({
  items,
  title = "Pendências",
  open = false,
  onToggle,
  onSelect,
  style,
}: PendingBoxProps) {
  const total = items.reduce((acc, i) => acc + i.count, 0);

  return (
    <Card padding={18} style={{ gap: 0, ...style }}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          width: "100%",
          border: 0,
          background: "transparent",
          cursor: "pointer",
          padding: 0,
          textAlign: "left",
          fontFamily: "var(--sbf-font-sans)",
          color: "var(--text)",
        }}
      >
        <Icon name="rule" size={21} style={{ color: "var(--warn)" }} />
        <span style={{ fontSize: 15.5, fontWeight: 700, flex: "1 1 auto" }}>
          {title} <span style={{ color: "var(--text-3)", fontWeight: 600 }}>({total})</span>
        </span>
        <Icon
          name={open ? "expand_less" : "expand_more"}
          size={22}
          style={{ color: "var(--text-3)" }}
        />
      </button>

      {open ? (
        <div style={{ display: "flex", flexDirection: "column", marginTop: 12 }}>
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              onClick={() => onSelect?.(item)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 11,
                padding: "11px 0",
                border: 0,
                borderTop: "1px solid var(--line-soft)",
                background: "transparent",
                cursor: "pointer",
                textAlign: "left",
                width: "100%",
                fontFamily: "var(--sbf-font-sans)",
                color: "var(--text)",
              }}
            >
              <span
                style={{
                  width: 8,
                  height: 8,
                  flex: "0 0 8px",
                  borderRadius: "var(--sbf-radius-pill)",
                  background: toneStyles[item.tone ?? "warn"].fg,
                }}
              />
              <span style={{ flex: "1 1 auto", minWidth: 0 }}>
                <span style={{ display: "block", fontSize: 13.5, fontWeight: 600 }}>
                  {item.label}
                </span>
                {item.hint ? (
                  <span style={{ display: "block", fontSize: 12, color: "var(--text-3)" }}>
                    {item.hint}
                  </span>
                ) : null}
              </span>
              <span style={{ fontSize: 13, fontWeight: 800, color: "var(--text-3)" }}>
                {item.count}
              </span>
              <Icon name="chevron_right" size={19} style={{ color: "var(--text-5)" }} />
            </button>
          ))}
        </div>
      ) : null}
    </Card>
  );
}
