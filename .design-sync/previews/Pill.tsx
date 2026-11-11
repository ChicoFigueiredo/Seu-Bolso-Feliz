import { Pill } from "@sbf/design-system";

export const Tons = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
    <Pill tone="neutral">9 lançamentos</Pill>
    <Pill tone="primary">caindo</Pill>
    <Pill tone="secondary">estimado</Pill>
    <Pill tone="accent">meta batida</Pill>
    <Pill tone="warn">vence amanhã</Pill>
    <Pill tone="danger">vencida há 3 dias</Pill>
  </div>
);

export const ComIcone = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
    <Pill tone="primary" icon="check_circle">
      Pago
    </Pill>
    <Pill tone="warn" icon="rate_review">
      Precisa revisar
    </Pill>
    <Pill tone="secondary" icon="lock">
      Protegido por senha
    </Pill>
    <Pill tone="warn" icon="content_copy">
      Possível duplicidade
    </Pill>
  </div>
);

export const Contorno = () => (
  <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
    <Pill tone="primary" variant="outline">
      Edulingo
    </Pill>
    <Pill tone="secondary" variant="outline">
      Imob Max
    </Pill>
    <Pill tone="neutral" variant="outline">
      Casa
    </Pill>
  </div>
);
