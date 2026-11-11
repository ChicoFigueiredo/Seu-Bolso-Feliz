import type { CSSProperties } from "react";

export interface Allocation {
  /** Centro que recebe a fatia. */
  name: string;
  /** Valor da fatia, formatado. */
  value: string;
  /** Peso da fatia (0 a 1) — usado para a largura. */
  share: number;
  /** Cor de identidade do centro. */
  color: string;
}

export interface AllocationBarProps {
  allocations: Allocation[];
  /** Título. Padrão `Divisão entre centros`. */
  title?: string;
  style?: CSSProperties;
}

/**
 * Como um lançamento se divide entre centros.
 *
 * Uma transação real de R$ 1.000 pode gerar três alocações analiticas sem
 * virar três lançamentos — e essa regra de dominio que o componente torna
 * visível. As fatias somam o valor original; se não somarem, a divisão está
 * incompleta e isso precisa aparecer.
 */
export function AllocationBar({
  allocations,
  title = "Divisão entre centros",
  style,
}: AllocationBarProps) {
  return (
    <div style={{ fontFamily: "var(--sbf-font-sans)", color: "var(--text)", ...style }}>
      <div style={{ fontSize: 13.5, fontWeight: 700, marginBottom: 10 }}>{title}</div>
      <div
        style={{
          display: "flex",
          height: 12,
          borderRadius: "var(--sbf-radius-pill)",
          overflow: "hidden",
          background: "var(--line-soft)",
        }}
      >
        {allocations.map((a) => (
          <div
            key={a.name}
            style={{ width: `${Math.max(0, Math.min(1, a.share)) * 100}%`, background: a.color }}
          />
        ))}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 12 }}>
        {allocations.map((a) => (
          <div key={a.name} style={{ display: "flex", alignItems: "center", gap: 9, fontSize: 13 }}>
            <span
              style={{
                width: 10,
                height: 10,
                borderRadius: 4,
                background: a.color,
                flex: "0 0 10px",
              }}
            />
            <span style={{ flex: "1 1 auto", minWidth: 0 }}>{a.name}</span>
            <span style={{ color: "var(--text-3)" }}>{Math.round(a.share * 100)}%</span>
            <span style={{ fontWeight: 800, fontVariantNumeric: "tabular-nums" }}>{a.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
