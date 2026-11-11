import { TopBar } from "@sbf/design-system";

export const CabecalhoDeHoje = () => (
  <div style={{ minWidth: 820 }}>
    <TopBar
      title="Hoje"
      subtitle="sábado, 22 de agosto de 2026 · ciclo 20/08–19/09"
      period="20 ago – 19 set"
    />
  </div>
);

export const OutraTela = () => (
  <div style={{ minWidth: 820 }}>
    <TopBar
      title="Projetos & Centros"
      subtitle="4 centros ativos · ciclo 20/08–19/09"
      period="20 ago – 19 set"
      actionLabel="Novo centro"
    />
  </div>
);

export const SemPeriodo = () => (
  <div style={{ minWidth: 820 }}>
    <TopBar title="Configurações" subtitle="Preferências, temas e privacidade" />
  </div>
);
