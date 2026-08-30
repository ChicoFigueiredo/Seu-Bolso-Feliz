import { Card, MetricLabel, MetricValue, formatBRL } from "@sbf/design-system";

export const Superficie = () => (
  <div style={{ maxWidth: 300 }}>
    <Card>
      <MetricLabel>Saldo devedor</MetricLabel>
      <MetricValue size="strong">{formatBRL(12170)}</MetricValue>
      <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>
        Compromisso mensal {formatBRL(920)} · quitação prevista para set/2027
      </div>
    </Card>
  </div>
);

export const Hero = () => (
  <div style={{ maxWidth: 300 }}>
    <Card variant="hero">
      <MetricLabel variant="onHero">Disponível agora</MetricLabel>
      <MetricValue size="hero">{formatBRL(4350)}</MetricValue>
      <div style={{ fontSize: 12.5, color: "var(--hero-sub)" }}>Somando 3 contas correntes</div>
    </Card>
  </div>
);

export const Discreto = () => (
  <div style={{ maxWidth: 300 }}>
    <Card variant="quiet">
      <div style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.5 }}>
        Saldo livre estimado. Existem 3 despesas ainda sem valor confirmado.
      </div>
    </Card>
  </div>
);

export const Clicavel = () => (
  <div style={{ maxWidth: 300 }}>
    <Card onClick={() => {}}>
      <MetricLabel>Edulingo</MetricLabel>
      <MetricValue size="medium">{formatBRL(2480)}</MetricValue>
      <div style={{ fontSize: 12, color: "var(--text-3)" }}>Toque para abrir o centro</div>
    </Card>
  </div>
);
