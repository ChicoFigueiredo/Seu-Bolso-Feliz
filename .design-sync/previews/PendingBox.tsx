import { PendingBox } from "@sbf/design-system";

const pendencias = [
  { label: "Vencidas", hint: "IPTU e mais uma conta", count: 2, tone: "danger" as const },
  {
    label: "Sem comprovante",
    hint: "lançamentos que você não consegue explicar depois",
    count: 4,
    tone: "warn" as const,
  },
  {
    label: "Sem centro",
    hint: "não entram no resultado de nenhum projeto",
    count: 3,
    tone: "secondary" as const,
  },
  {
    label: "Possível duplicidade",
    hint: "duas cobranças iguais no mesmo dia",
    count: 1,
    tone: "warn" as const,
  },
  {
    label: "Precisa de revisão da IA",
    hint: "sugestões esperando sua confirmação",
    count: 1,
    tone: "secondary" as const,
  },
];

export const Recolhida = () => (
  <div style={{ maxWidth: 420 }}>
    <PendingBox items={pendencias} />
  </div>
);

export const Aberta = () => (
  <div style={{ maxWidth: 420 }}>
    <PendingBox items={pendencias} open />
  </div>
);

export const PoucasPendencias = () => (
  <div style={{ maxWidth: 420 }}>
    <PendingBox
      open
      items={[
        {
          label: "Sem comprovante",
          hint: "um lançamento de agosto",
          count: 1,
          tone: "warn" as const,
        },
      ]}
    />
  </div>
);
