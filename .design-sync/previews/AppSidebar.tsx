import { AppSidebar } from "@sbf/design-system";

const nav = [
  { id: "hoje", label: "Hoje", icon: "today" },
  { id: "mov", label: "Movimentações", icon: "receipt_long" },
  { id: "contas", label: "Contas & Cartões", icon: "account_balance" },
  { id: "dividas", label: "Dívidas", icon: "trending_down" },
  { id: "recorrentes", label: "Recorrentes", icon: "autorenew" },
  { id: "centros", label: "Projetos & Centros", icon: "workspaces" },
  { id: "documentos", label: "Documentos", icon: "folder_open", badge: 5 },
  { id: "ia", label: "Inteligência", icon: "auto_awesome", badge: 2 },
];

const secundarios = [
  { id: "conexoes", label: "Conexões", icon: "sync_alt" },
  { id: "config", label: "Configurações", icon: "settings" },
];

export const NavegacaoCompleta = () => (
  <div style={{ height: 620, display: "flex" }}>
    <AppSidebar
      items={nav}
      active="hoje"
      secondary={secundarios}
      workerName="MacBook do Chico"
      workerStatus="sincronizado 08:12"
    />
  </div>
);

export const OutraSecaoAtiva = () => (
  <div style={{ height: 620, display: "flex" }}>
    <AppSidebar
      items={nav}
      active="documentos"
      secondary={secundarios}
      workerName="MacBook do Chico"
      workerStatus="sincronizado 08:12"
    />
  </div>
);

export const WorkerOffline = () => (
  <div style={{ height: 620, display: "flex" }}>
    <AppSidebar
      items={nav}
      active="hoje"
      secondary={secundarios}
      workerName="MacBook do Chico"
      workerStatus="offline desde ontem, 22:40"
      workerOnline={false}
    />
  </div>
);
