import { AccountBalanceCard, formatBRL } from "@sbf/design-system";

export const ContaConciliada = () => (
  <div style={{ maxWidth: 320 }}>
    <AccountBalanceCard
      name="Conta Corrente Caixa"
      balance={formatBRL(2350)}
      caption="Último movimento hoje, 08:12"
      reconciled
    />
  </div>
);

export const NaoConciliada = () => (
  <div style={{ maxWidth: 320 }}>
    <AccountBalanceCard
      name="Conta Nubank"
      balance={formatBRL(1200)}
      icon="account_balance_wallet"
      caption="3 lançamentos ainda sem comprovante"
    />
  </div>
);

export const ForaDoCalculo = () => (
  <div style={{ maxWidth: 320 }}>
    <AccountBalanceCard
      name="Poupança (reserva)"
      balance={formatBRL(5000)}
      icon="savings"
      caption="Reserva de emergência"
      excluded
    />
  </div>
);

export const PosicaoCompleta = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 340 }}>
    <AccountBalanceCard
      name="Conta Corrente Caixa"
      balance={formatBRL(2350)}
      caption="Último movimento hoje, 08:12"
      reconciled
    />
    <AccountBalanceCard
      name="Conta Nubank"
      balance={formatBRL(1200)}
      icon="account_balance_wallet"
      caption="3 lançamentos sem comprovante"
    />
    <AccountBalanceCard
      name="Poupança (reserva)"
      balance={formatBRL(5000)}
      icon="savings"
      caption="Reserva de emergência"
      excluded
    />
  </div>
);
