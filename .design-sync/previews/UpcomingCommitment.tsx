import { UpcomingCommitment, formatBRL } from "@sbf/design-system";

export const Pendente = () => (
  <UpcomingCommitment
    dayLabel="Hoje"
    dateLabel="22/08"
    name="Energia CEMIG"
    meta="Conta Corrente Caixa · Casa"
    amount={formatBRL(385)}
    status="pending"
  />
);

export const Pago = () => (
  <UpcomingCommitment
    dayLabel="Ontem"
    dateLabel="21/08"
    name="Internet Vivo Fibra"
    meta="Cartão Nubank ·4567 · Casa"
    amount={formatBRL(129)}
    status="paid"
  />
);

export const Vencido = () => (
  <UpcomingCommitment
    dayLabel="Venceu"
    dateLabel="18/08"
    name="IPTU parcela 8/10"
    meta="Prefeitura · vencida há 4 dias"
    amount={formatBRL(242)}
    status="overdue"
  />
);

export const FilaDoPeriodo = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
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
      status="pending"
    />
    <UpcomingCommitment
      dayLabel="Amanhã"
      dateLabel="23/08"
      name="Fatura Nubank"
      meta="Cartão ·4567 · fecha em 5 dias"
      amount={formatBRL(2150)}
      status="pending"
    />
    <UpcomingCommitment
      dayLabel="Seg"
      dateLabel="25/08"
      name="Hetzner Online"
      meta="Cartão C6 ·8891 · Edulingo, Imob Max"
      amount={formatBRL(1000)}
      status="pending"
    />
  </div>
);
