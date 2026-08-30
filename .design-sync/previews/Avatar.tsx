import { Avatar } from "@sbf/design-system";

export const Tamanhos = () => (
  <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
    <Avatar name="Chico Figueiredo" size={28} />
    <Avatar name="Chico Figueiredo" size={40} />
    <Avatar name="Chico Figueiredo" size={56} />
  </div>
);

export const Iniciais = () => (
  <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
    <Avatar name="Chico Figueiredo" />
    <Avatar name="Daiane Cristina" />
    <Avatar name="Ana" />
  </div>
);
