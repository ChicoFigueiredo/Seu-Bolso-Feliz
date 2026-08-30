import { BottomNav } from "@sbf/design-system";

const abas = [
  { id: "hoje", label: "Hoje", icon: "today" },
  { id: "mov", label: "Movimentos", icon: "receipt_long" },
  { id: "centros", label: "Projetos", icon: "workspaces" },
  { id: "mais", label: "Mais", icon: "more_horiz" },
];

export const ComAcaoCentral = () => (
  <div style={{ width: 420, paddingTop: 24 }}>
    <BottomNav tabs={abas} active="hoje" />
  </div>
);

export const OutraAbaAtiva = () => (
  <div style={{ width: 420, paddingTop: 24 }}>
    <BottomNav tabs={abas} active="centros" />
  </div>
);

export const SemAcaoCentral = () => (
  <div style={{ width: 420, paddingTop: 24 }}>
    <BottomNav tabs={abas} active="mais" fab={false} />
  </div>
);
