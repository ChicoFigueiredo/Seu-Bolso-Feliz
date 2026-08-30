import { MetricLabel, MetricValue, formatBRL } from "@sbf/design-system";

export const Escala = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
    <div style={{ background: "var(--hero-bg)", borderRadius: 18, padding: 18 }}>
      <MetricLabel variant="onHero">hero — serifa 40px</MetricLabel>
      <MetricValue size="hero" color="var(--hero-text)">
        {formatBRL(4350)}
      </MetricValue>
    </div>
    <div>
      <MetricLabel>display — serifa 38px</MetricLabel>
      <MetricValue size="display">{formatBRL(985)}</MetricValue>
    </div>
    <div>
      <MetricLabel>strong — sans 24px</MetricLabel>
      <MetricValue size="strong">{formatBRL(2150)}</MetricValue>
    </div>
    <div>
      <MetricLabel>medium — sans 20px</MetricLabel>
      <MetricValue size="medium">{formatBRL(8400)}</MetricValue>
    </div>
    <div>
      <MetricLabel>compact — sans 15px</MetricLabel>
      <MetricValue size="compact">{formatBRL(385)}</MetricValue>
    </div>
  </div>
);

export const CoresSemanticas = () => (
  <div
    style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(130px,1fr))", gap: 14 }}
  >
    <div>
      <MetricLabel>Entrou</MetricLabel>
      <MetricValue size="medium" color="var(--success)">
        {formatBRL(8400)}
      </MetricValue>
    </div>
    <div>
      <MetricLabel>Saiu</MetricLabel>
      <MetricValue size="medium">{formatBRL(6580)}</MetricValue>
    </div>
    <div>
      <MetricLabel>Ainda vai sair</MetricLabel>
      <MetricValue size="medium" color="var(--warn)">
        {formatBRL(1835)}
      </MetricValue>
    </div>
    <div>
      <MetricLabel>Em atraso</MetricLabel>
      <MetricValue size="medium" color="var(--danger)">
        {formatBRL(242)}
      </MetricValue>
    </div>
  </div>
);

export const AlinhamentoTabular = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 4, maxWidth: 220 }}>
    <MetricValue size="compact">{formatBRL(1000)}</MetricValue>
    <MetricValue size="compact">{formatBRL(2150)}</MetricValue>
    <MetricValue size="compact">{formatBRL(385)}</MetricValue>
    <MetricValue size="compact">{formatBRL(12170)}</MetricValue>
  </div>
);
