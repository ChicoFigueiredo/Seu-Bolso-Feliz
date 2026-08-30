import { CreditCardSummary, formatBRL } from "@sbf/design-system";

export const VenceAmanha = () => (
  <div style={{ maxWidth: 300 }}>
    <CreditCardSummary
      name="Cartão Nubank ·4567"
      invoice={formatBRL(2150)}
      available={formatBRL(5850)}
      limit={formatBRL(8000)}
      used={0.269}
      dueLabel="vence amanhã"
    />
  </div>
);

export const LimiteApertado = () => (
  <div style={{ maxWidth: 300 }}>
    <CreditCardSummary
      name="Cartão C6 ·8891"
      invoice={formatBRL(4720)}
      available={formatBRL(280)}
      limit={formatBRL(5000)}
      used={0.944}
      dueLabel="fecha em 2 dias"
    />
  </div>
);

export const SemAviso = () => (
  <div style={{ maxWidth: 300 }}>
    <CreditCardSummary
      name="Cartão Inter ·2210"
      invoice={formatBRL(340)}
      available={formatBRL(2660)}
      limit={formatBRL(3000)}
      used={0.113}
    />
  </div>
);

export const DoisCartoes = () => (
  <div
    style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(240px,1fr))", gap: 12 }}
  >
    <CreditCardSummary
      name="Cartão Nubank ·4567"
      invoice={formatBRL(2150)}
      available={formatBRL(5850)}
      limit={formatBRL(8000)}
      used={0.269}
      dueLabel="vence amanhã"
    />
    <CreditCardSummary
      name="Cartão C6 ·8891"
      invoice={formatBRL(4720)}
      available={formatBRL(280)}
      limit={formatBRL(5000)}
      used={0.944}
      dueLabel="fecha em 2 dias"
    />
  </div>
);
