import { FinancialCenterCard, formatBRL } from "@sbf/design-system";

export const CentroPositivo = () => (
  <div style={{ maxWidth: 220 }}>
    <FinancialCenterCard
      name="Edulingo"
      balance={formatBRL(2480)}
      meta="entrou R$ 4.200 · saiu R$ 1.720"
    />
  </div>
);

export const CentroNegativo = () => (
  <div style={{ maxWidth: 220 }}>
    <FinancialCenterCard
      name="Imob Max"
      balance={formatBRL(-860)}
      color="var(--secondary)"
      meta="entrou R$ 400 · saiu R$ 1.260"
      negative
    />
  </div>
);

export const CentrosPrincipais = () => (
  <div
    style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(170px,1fr))", gap: 12 }}
  >
    <FinancialCenterCard name="Edulingo" balance={formatBRL(2480)} meta="gasto do mês R$ 1.720" />
    <FinancialCenterCard
      name="Imob Max"
      balance={formatBRL(-860)}
      color="var(--secondary)"
      meta="gasto do mês R$ 1.260"
      negative
    />
    <FinancialCenterCard
      name="IA"
      balance={formatBRL(-410)}
      color="var(--accent)"
      meta="gasto do mês R$ 410"
      negative
    />
    <FinancialCenterCard
      name="Casa"
      balance={formatBRL(-2130)}
      color="var(--warn)"
      meta="gasto do mês R$ 2.130"
      negative
    />
  </div>
);
