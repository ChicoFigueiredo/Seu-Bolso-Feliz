import { SegmentedBar } from "@sbf/design-system";

export const SituacaoDoPeriodo = () => (
  <div style={{ maxWidth: 460 }}>
    <SegmentedBar
      total={8400}
      segments={[
        { label: "já pago", value: 4750, color: "var(--primary)" },
        { label: "comprometido", value: 1835, color: "var(--accent)" },
      ]}
    />
  </div>
);

export const CicloApertado = () => (
  <div style={{ maxWidth: 460 }}>
    <SegmentedBar
      total={8400}
      segments={[
        { label: "já pago", value: 6100, color: "var(--primary)" },
        { label: "comprometido", value: 2100, color: "var(--accent)" },
      ]}
    />
  </div>
);

export const GastoPorCentro = () => (
  <div style={{ maxWidth: 460 }}>
    <SegmentedBar
      remainderLabel="outros"
      total={4000}
      segments={[
        { label: "Edulingo", value: 1800, color: "var(--primary)" },
        { label: "Imob Max", value: 1100, color: "var(--secondary)" },
        { label: "IA", value: 600, color: "var(--accent)" },
      ]}
    />
  </div>
);
