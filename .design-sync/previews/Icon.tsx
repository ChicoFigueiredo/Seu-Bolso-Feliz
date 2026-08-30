import { Icon } from "@sbf/design-system";

const Cell = ({ name }: { name: string }) => (
  <div
    style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6, width: 82 }}
  >
    <Icon name={name} size={24} style={{ color: "var(--text-2)" }} />
    <span style={{ fontSize: 10.5, color: "var(--text-3)", fontFamily: "var(--sbf-font-sans)" }}>
      {name}
    </span>
  </div>
);

export const IconesDoProduto = () => (
  <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
    {[
      "savings",
      "account_balance",
      "credit_card",
      "trending_down",
      "receipt_long",
      "auto_awesome",
      "rule",
      "sync_alt",
    ].map((n) => (
      <Cell key={n} name={n} />
    ))}
  </div>
);

export const IconesDeEstado = () => (
  <div style={{ display: "flex", gap: 12, flexWrap: "wrap" }}>
    {[
      "check_circle",
      "info",
      "warning",
      "lock",
      "content_copy",
      "hourglass_top",
      "chevron_right",
      "expand_more",
    ].map((n) => (
      <Cell key={n} name={n} />
    ))}
  </div>
);

export const Tamanhos = () => (
  <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
    <Icon name="savings" size={16} style={{ color: "var(--text-3)" }} />
    <Icon name="savings" size={20} style={{ color: "var(--text-2)" }} />
    <Icon name="savings" size={28} style={{ color: "var(--primary)" }} />
    <Icon name="savings" size={40} style={{ color: "var(--primary)" }} />
  </div>
);

export const Preenchido = () => (
  <div style={{ display: "flex", gap: 16, alignItems: "center" }}>
    <Icon name="home" size={28} style={{ color: "var(--text-3)" }} />
    <Icon name="home" size={28} filled style={{ color: "var(--primary)" }} />
  </div>
);
