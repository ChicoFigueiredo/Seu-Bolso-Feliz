import { RecurringItem, formatBRL } from "@sbf/design-system";

export const AssinaturaEstavel = () => (
  <div style={{ maxWidth: 460 }}>
    <RecurringItem
      name="Internet Vivo Fibra"
      average={formatBRL(129)}
      next={formatBRL(129)}
      cadence="todo dia 10"
      source="Cartão Nubank ·4567"
    />
  </div>
);

export const AumentoAtipico = () => (
  <div style={{ maxWidth: 460 }}>
    <RecurringItem
      name="Hetzner Online"
      average={formatBRL(745)}
      next={formatBRL(1000)}
      cadence="entre os dias 24 e 26"
      source="Cartão C6 ·8891"
      variation={0.34}
    />
  </div>
);

export const Barateou = () => (
  <div style={{ maxWidth: 460 }}>
    <RecurringItem
      name="Plano de celular"
      average={formatBRL(89)}
      next={formatBRL(69)}
      cadence="todo dia 5"
      source="Conta Corrente Caixa"
      variation={-0.22}
    />
  </div>
);

export const DetectadaPelaIA = () => (
  <div style={{ maxWidth: 460 }}>
    <RecurringItem
      name="Adobe Creative Cloud"
      average={formatBRL(119)}
      next={formatBRL(119)}
      cadence="todo dia 12"
      source="Cartão Nubank ·4567"
      suggested
    />
  </div>
);
