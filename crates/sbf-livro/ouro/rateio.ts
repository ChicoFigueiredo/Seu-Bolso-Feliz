// Gera a fixture de ouro da direção do rateio: `allocationDirection` e
// `canAllocateTransactionType` de packages/domain/src/allocation/transaction-direction.ts
// para os 8 tipos de `transactions.type` e um tipo desconhecido.
import {
  allocationDirection,
  canAllocateTransactionType,
} from "../../../packages/domain/src/allocation/transaction-direction.ts";

const TIPOS = [
  "income",
  "expense",
  "refund",
  "adjustment",
  "interest_charge",
  "fee",
  "statement_payment",
  "liability_payment",
  "tipo_que_nao_existe",
];

console.log(
  JSON.stringify({
    fonte: "packages/domain/src/allocation/transaction-direction.ts",
    casos: TIPOS.map((tipo) => ({
      tipo,
      direcao: allocationDirection(tipo),
      aceitaRateio: canAllocateTransactionType(tipo),
    })),
  }),
);
