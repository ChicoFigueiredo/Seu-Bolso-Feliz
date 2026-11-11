import { GoalProgress, formatBRL } from "@sbf/design-system";

export const EmAndamento = () => (
  <div style={{ maxWidth: 400 }}>
    <GoalProgress
      name="Quitar o empréstimo Caixa"
      progress={0.68}
      remaining={formatBRL(9840)}
      forecast="No ritmo atual, quitação em set/2027."
    />
  </div>
);

export const NoComeco = () => (
  <div style={{ maxWidth: 400 }}>
    <GoalProgress
      name="Formar reserva de 3 meses"
      progress={0.22}
      remaining={formatBRL(14000)}
      forecast="No ritmo atual, meta atingida em mai/2028."
    />
  </div>
);

export const Concluida = () => (
  <div style={{ maxWidth: 400 }}>
    <GoalProgress
      name="Quitar o parcelamento do notebook"
      progress={1}
      achieved
      achievement="Você reduziu R$ 18.400 desta dívida em 11 meses."
    />
  </div>
);
