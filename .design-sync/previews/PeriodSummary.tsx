import { PeriodSummary, formatBRL } from "@sbf/design-system";

export const CicloCorrente = () => (
  <div style={{ maxWidth: 480 }}>
    <PeriodSummary
      income={formatBRL(8400)}
      spent={formatBRL(4750)}
      committed={formatBRL(1835)}
      projected={formatBRL(1815)}
      bar={{ income: 8400, spent: 4750, committed: 1835 }}
    />
  </div>
);

export const CicloApertado = () => (
  <div style={{ maxWidth: 480 }}>
    <PeriodSummary
      title="Situação de setembro"
      income={formatBRL(8400)}
      spent={formatBRL(6100)}
      committed={formatBRL(2100)}
      projected={formatBRL(200)}
      bar={{ income: 8400, spent: 6100, committed: 2100 }}
    />
  </div>
);

export const SemGrafico = () => (
  <div style={{ maxWidth: 480 }}>
    <PeriodSummary
      income={formatBRL(8400)}
      spent={formatBRL(4750)}
      committed={formatBRL(1835)}
      projected={formatBRL(1815)}
    />
  </div>
);
