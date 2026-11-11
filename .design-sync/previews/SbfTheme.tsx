import { MoneyMetricCard, Pill, SbfTheme, formatBRL } from "@sbf/design-system";

const Amostra = ({ nome }: { nome: string }) => (
  <div style={{ display: "flex", flexDirection: "column", gap: 10, padding: 16, width: 250 }}>
    <div
      style={{
        fontSize: 11.5,
        fontWeight: 700,
        letterSpacing: ".09em",
        textTransform: "uppercase",
        color: "var(--text-3)",
      }}
    >
      {nome}
    </div>
    <MoneyMetricCard variant="hero" label="Disponível agora" value={formatBRL(4350)} />
    <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
      <Pill tone="primary">caindo</Pill>
      <Pill tone="secondary">estimado</Pill>
      <Pill tone="accent">meta</Pill>
      <Pill tone="warn">vence amanhã</Pill>
    </div>
  </div>
);

export const TemasClaros = () => (
  <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
    {(["salvia", "ocre", "azul"] as const).map((t) => (
      <SbfTheme key={t} theme={t} style={{ borderRadius: 18, border: "1px solid var(--line)" }}>
        <Amostra nome={t} />
      </SbfTheme>
    ))}
  </div>
);

export const MaisClaros = () => (
  <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
    {(["linho", "menta"] as const).map((t) => (
      <SbfTheme key={t} theme={t} style={{ borderRadius: 18, border: "1px solid var(--line)" }}>
        <Amostra nome={t} />
      </SbfTheme>
    ))}
  </div>
);

export const TemasEscuros = () => (
  <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
    {(["noite", "grafite"] as const).map((t) => (
      <SbfTheme key={t} theme={t} style={{ borderRadius: 18, border: "1px solid var(--line)" }}>
        <Amostra nome={t} />
      </SbfTheme>
    ))}
  </div>
);
