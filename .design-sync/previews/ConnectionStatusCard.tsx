import { ConnectionStatusCard } from "@sbf/design-system";

export const GmailConectado = () => (
  <div style={{ maxWidth: 360 }}>
    <ConnectionStatusCard
      name="Gmail (financeiro@)"
      icon="mail"
      status="conectado"
      lastSync="hoje, 08:12"
      found={4}
    />
  </div>
);

export const WorkerLocal = () => (
  <div style={{ maxWidth: 360 }}>
    <ConnectionStatusCard
      name="MacBook do Chico"
      icon="computer"
      status="conectado"
      lastSync="hoje, 08:12"
      found={0}
    />
  </div>
);

export const ComFalha = () => (
  <div style={{ maxWidth: 360 }}>
    <ConnectionStatusCard
      name="Pasta local ~/Documentos/Contas"
      icon="folder"
      status="falha"
      lastSync="ontem, 22:40"
      error="A pasta não foi encontrada. Ela pode ter sido movida ou renomeada."
    />
  </div>
);

export const CentralDeConexoes = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 360 }}>
    <ConnectionStatusCard
      name="Gmail (financeiro@)"
      icon="mail"
      status="conectado"
      lastSync="hoje, 08:12"
      found={4}
    />
    <ConnectionStatusCard name="Google Drive" icon="cloud" status="desconectado" />
    <ConnectionStatusCard
      name="Pasta ~/Documentos/Contas"
      icon="folder"
      status="sincronizando"
      lastSync="agora"
    />
  </div>
);
