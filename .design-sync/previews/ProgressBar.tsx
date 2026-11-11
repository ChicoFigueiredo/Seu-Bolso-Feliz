import { ProgressBar } from "@sbf/design-system";

export const QuitacaoDeDivida = () => (
  <div style={{ maxWidth: 420 }}>
    <ProgressBar value={0.68} size="md" label="68% quitado" hint="18 de 48 parcelas" />
  </div>
);

export const LimiteDeCartao = () => (
  <div style={{ maxWidth: 420 }}>
    <ProgressBar
      value={0.269}
      fill="var(--secondary)"
      labelColor="var(--secondary-ink)"
      label="Limite livre R$ 5.850"
      hint="de R$ 8.000"
    />
  </div>
);

export const SemRotulos = () => (
  <div style={{ maxWidth: 420, display: "flex", flexDirection: "column", gap: 14 }}>
    <ProgressBar value={0.15} />
    <ProgressBar value={0.5} />
    <ProgressBar value={0.92} />
  </div>
);

export const MetaBatida = () => (
  <div style={{ maxWidth: 420 }}>
    <ProgressBar value={1} size="md" label="100% quitado" hint="48 de 48 parcelas" />
  </div>
);
