import { MobileHeader } from "@sbf/design-system";

export const BomDia = () => (
  <div style={{ width: 420, background: "var(--bg)" }}>
    <MobileHeader title="Hoje" greeting="Bom dia, Chico" userName="Chico Figueiredo" />
  </div>
);

export const OutraTela = () => (
  <div style={{ width: 420, background: "var(--bg)" }}>
    <MobileHeader title="Movimentações" userName="Chico Figueiredo" />
  </div>
);
