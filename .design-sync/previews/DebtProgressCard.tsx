import { DebtProgressCard, formatBRL } from "@sbf/design-system";

export const EmprestimoEmAndamento = () => (
  <div style={{ maxWidth: 460 }}>
    <DebtProgressCard
      name="Empréstimo Caixa"
      meta="Contratado em fev/2024 · 48 parcelas"
      balance={formatBRL(9840)}
      paid={0.68}
      installments="18 de 48 pagas"
      facts={["Próxima parcela 05/09", "Taxa 1,79% a.m.", "Termina em set/2027"]}
    />
  </div>
);

export const QuaseQuitada = () => (
  <div style={{ maxWidth: 460 }}>
    <DebtProgressCard
      name="Parcelamento notebook"
      meta="Cartão C6 ·8891 · 12 parcelas"
      balance={formatBRL(620)}
      paid={0.92}
      installments="11 de 12 pagas"
      facts={["Próxima parcela 12/09", "Sem juros"]}
    />
  </div>
);

export const NoComeco = () => (
  <div style={{ maxWidth: 460 }}>
    <DebtProgressCard
      name="Financiamento do carro"
      meta="Banco Inter · 60 parcelas"
      balance={formatBRL(41200)}
      paid={0.08}
      installments="5 de 60 pagas"
      facts={["Próxima parcela 20/09", "Taxa 1,29% a.m.", "Termina em jul/2031"]}
    />
  </div>
);
