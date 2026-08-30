import { MoneyMetricCard, formatBRL } from "@sbf/design-system";

export const Hero = () => (
  <div style={{ maxWidth: 280 }}>
    <MoneyMetricCard
      variant="hero"
      label="Disponível agora"
      value={formatBRL(4350)}
      caption="Somando 3 contas correntes · atualizado hoje, 08:12"
      actionLabel="Como chegamos neste valor?"
    />
  </div>
);

export const Estimado = () => (
  <div style={{ maxWidth: 280 }}>
    <MoneyMetricCard
      label="Livre após compromissos"
      value={formatBRL(985)}
      estimated
      note="3 despesas ainda sem valor confirmado. O número pode mudar."
    />
  </div>
);

export const Simples = () => (
  <div style={{ maxWidth: 280 }}>
    <MoneyMetricCard
      label="Resultado do mês"
      value={formatBRL(1820)}
      caption="Entrou R$ 8.400 · saiu R$ 6.580"
    />
  </div>
);

export const FaixaDaTelaHoje = () => (
  <div
    style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit,minmax(232px,1fr))", gap: 14 }}
  >
    <MoneyMetricCard
      variant="hero"
      label="Disponível agora"
      value={formatBRL(4350)}
      caption="Somando 3 contas correntes · atualizado hoje, 08:12"
      actionLabel="Como chegamos neste valor?"
    />
    <MoneyMetricCard
      label="Livre após compromissos"
      value={formatBRL(985)}
      estimated
      note="3 despesas ainda sem valor confirmado. O número pode mudar."
    />
  </div>
);
