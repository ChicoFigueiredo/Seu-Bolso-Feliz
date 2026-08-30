import type { CSSProperties } from "react";

export interface SegmentedBarSegment {
  /** Rotulo na legenda — `já pago`, `comprometido`. */
  label: string;
  /** Peso do segmento. Os valores são normalizados pela soma. */
  value: number;
  /** Cor do segmento, em token. */
  color: string;
}

export interface SegmentedBarProps {
  segments: SegmentedBarSegment[];
  /** Rotulo do espaco não preenchido. Padrão `livre`. */
  remainderLabel?: string;
  /** Total de referência. Sem ele, a barra e preenchida pela soma dos segmentos. */
  total?: number;
  height?: number;
  style?: CSSProperties;
}

/**
 * Barra empilhada com legenda — quanto já saiu, quanto está comprometido,
 * quanto sobra.
 *
 * A legenda não e opcional: a barra sozinha comunicaria por cor, e o sistema
 * proibe cor como único indicador. O sobrando aparece como trilho vazio, não
 * como segmento, porque "livre" e ausência de compromisso.
 */
export function SegmentedBar({
  segments,
  remainderLabel = "livre",
  total,
  height = 14,
  style,
}: SegmentedBarProps) {
  const sum = segments.reduce((acc, s) => acc + Math.max(0, s.value), 0);
  const base = total && total > 0 ? total : sum;
  const pct = (v: number) => (base > 0 ? `${(Math.max(0, v) / base) * 100}%` : "0%");

  return (
    <div style={{ fontFamily: "var(--sbf-font-sans)", ...style }}>
      <div
        style={{
          display: "flex",
          height,
          borderRadius: "var(--sbf-radius-pill)",
          overflow: "hidden",
          background: "var(--line-soft)",
        }}
      >
        {segments.map((s) => (
          <div key={s.label} style={{ width: pct(s.value), background: s.color }} />
        ))}
      </div>
      <div
        style={{
          display: "flex",
          gap: 16,
          marginTop: 9,
          fontSize: 12,
          color: "var(--text-3)",
          flexWrap: "wrap",
        }}
      >
        {segments.map((s) => (
          <span key={s.label} style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ width: 9, height: 9, borderRadius: 3, background: s.color }} />
            {s.label}
          </span>
        ))}
        {remainderLabel ? (
          <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span
              style={{ width: 9, height: 9, borderRadius: 3, background: "var(--line-soft)" }}
            />
            {remainderLabel}
          </span>
        ) : null}
      </div>
    </div>
  );
}
