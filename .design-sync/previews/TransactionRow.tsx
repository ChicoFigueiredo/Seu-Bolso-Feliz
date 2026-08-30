import { Card, TransactionRow, formatBRL } from "@sbf/design-system";

export const DiaDeMovimentacoes = () => (
  <div style={{ maxWidth: 560 }}>
    <Card variant="flush">
      <TransactionRow
        description="Salário Edulingo"
        meta="Conta Corrente Caixa · Receita"
        amount={formatBRL(6400)}
        direction="in"
        centers={["Edulingo"]}
        hasDocument
      />
      <TransactionRow
        description="Hetzner Online"
        meta="Cartão C6 ·8891 · Infraestrutura"
        amount={`- ${formatBRL(1000)}`}
        centers={["Edulingo", "Imob Max", "Seu Bolso Feliz"]}
        hasDocument
      />
      <TransactionRow
        description="Energia CEMIG"
        meta="Conta Corrente Caixa · Casa"
        amount={`- ${formatBRL(385)}`}
        centers={["Casa"]}
      />
      <TransactionRow
        description="Mercado Extra"
        meta="Cartão Nubank ·4567 · Alimentação"
        amount={`- ${formatBRL(243)}`}
        pending
      />
    </Card>
  </div>
);

export const Receita = () => (
  <div style={{ maxWidth: 560 }}>
    <Card variant="flush">
      <TransactionRow
        description="Salário Edulingo"
        meta="Conta Corrente Caixa · Receita"
        amount={formatBRL(6400)}
        direction="in"
        centers={["Edulingo"]}
        hasDocument
      />
    </Card>
  </div>
);

export const NaoConfirmada = () => (
  <div style={{ maxWidth: 560 }}>
    <Card variant="flush">
      <TransactionRow
        description="Mercado Extra"
        meta="Cartão Nubank ·4567 · Alimentação"
        amount={`- ${formatBRL(243)}`}
        pending
      />
    </Card>
  </div>
);
