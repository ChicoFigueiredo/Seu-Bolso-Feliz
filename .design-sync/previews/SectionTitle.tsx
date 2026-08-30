import { SectionTitle, UpcomingCommitment, formatBRL } from "@sbf/design-system";

export const ComAtalho = () => (
  <div style={{ maxWidth: 460 }}>
    <SectionTitle actionLabel="Ver tudo">Próximos dias</SectionTitle>
  </div>
);

export const Sozinho = () => (
  <div style={{ maxWidth: 460 }}>
    <SectionTitle>Situação do período</SectionTitle>
  </div>
);

export const NoTopoDeUmBloco = () => (
  <div style={{ maxWidth: 460, display: "flex", flexDirection: "column", gap: 12 }}>
    <SectionTitle actionLabel="Ver tudo">Próximos dias</SectionTitle>
    <UpcomingCommitment
      dayLabel="Hoje"
      dateLabel="22/08"
      name="Energia CEMIG"
      meta="Conta Corrente Caixa · Casa"
      amount={formatBRL(385)}
    />
  </div>
);
