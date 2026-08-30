import { MetricLabel, MetricValue, formatBRL } from "@sbf/design-system";

export const SobreSuperficie = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
    <div>
      <MetricLabel>Disponível agora</MetricLabel>
      <MetricValue size="strong">{formatBRL(4350)}</MetricValue>
    </div>
    <div>
      <MetricLabel>Livre após compromissos</MetricLabel>
      <MetricValue size="strong">{formatBRL(985)}</MetricValue>
    </div>
  </div>
);

export const SobreOHero = () => (
  <div style={{ background: "var(--hero-bg)", borderRadius: 22, padding: 20, maxWidth: 280 }}>
    <MetricLabel variant="onHero">Disponível agora</MetricLabel>
    <MetricValue size="hero">{formatBRL(4350)}</MetricValue>
  </div>
);
