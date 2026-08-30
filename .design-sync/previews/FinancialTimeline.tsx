import { FinancialTimeline, UpcomingCommitment, formatBRL } from "@sbf/design-system";

export const ProximosDias = () => (
  <div style={{ maxWidth: 480 }}>
    <FinancialTimeline
      actionLabel="Ver tudo"
      footnote={
        <>
          Até 19/09 você ainda precisa de <strong>{formatBRL(1835)}</strong> para fechar o período.
        </>
      }
    >
      <UpcomingCommitment
        dayLabel="Hoje"
        dateLabel="22/08"
        name="Energia CEMIG"
        meta="Conta Corrente Caixa · Casa"
        amount={formatBRL(385)}
      />
      <UpcomingCommitment
        dayLabel="Amanhã"
        dateLabel="23/08"
        name="Fatura Nubank"
        meta="Cartão ·4567 · fecha em 5 dias"
        amount={formatBRL(2150)}
      />
      <UpcomingCommitment
        dayLabel="Seg"
        dateLabel="25/08"
        name="Hetzner Online"
        meta="Cartão C6 ·8891 · Edulingo, Imob Max"
        amount={formatBRL(1000)}
      />
    </FinancialTimeline>
  </div>
);

export const ComVencido = () => (
  <div style={{ maxWidth: 480 }}>
    <FinancialTimeline
      title="O que vence primeiro"
      actionLabel="Ver tudo"
      footnote={
        <>
          Uma conta venceu. Regularizar hoje evita juros de <strong>{formatBRL(9)}</strong> por dia.
        </>
      }
      footnoteIcon="priority_high"
    >
      <UpcomingCommitment
        dayLabel="Venceu"
        dateLabel="18/08"
        name="IPTU parcela 8/10"
        meta="Prefeitura · vencida há 4 dias"
        amount={formatBRL(242)}
        status="overdue"
      />
      <UpcomingCommitment
        dayLabel="Hoje"
        dateLabel="22/08"
        name="Energia CEMIG"
        meta="Conta Corrente Caixa · Casa"
        amount={formatBRL(385)}
      />
    </FinancialTimeline>
  </div>
);

export const SemFecho = () => (
  <div style={{ maxWidth: 480 }}>
    <FinancialTimeline title="Já pagos nesta semana">
      <UpcomingCommitment
        dayLabel="Ontem"
        dateLabel="21/08"
        name="Internet Vivo Fibra"
        meta="Cartão Nubank ·4567"
        amount={formatBRL(129)}
        status="paid"
      />
      <UpcomingCommitment
        dayLabel="Qua"
        dateLabel="20/08"
        name="Mensalidade academia"
        meta="Conta Corrente Caixa"
        amount={formatBRL(149)}
        status="paid"
      />
    </FinancialTimeline>
  </div>
);
