import { z } from "zod";

// ══════════════════════════════════════════════════════════════
// Rateios
//
// Rateio é a divisão do valor de uma movimentação entre um ou mais centros.
// Guarda **valor**, nunca percentual — o percentual é derivado na exibição
// (`percentageOf` em @sbf/domain). A soma dos rateios pode ser menor que a
// movimentação (o restante é "não alocado"), nunca maior — quem confere isso
// é `validateAllocations`, porque a regra depende do valor da movimentação,
// que não está neste payload.
//
// Ao contrário dos outros schemas deste pacote, os campos aqui estão em
// camelCase: este não é o formato da linha de `transaction_allocations`
// (`user_id` e `transaction_id` vêm da sessão e do parâmetro da action, não
// do cliente), e sim o mesmo `AllocationInput` de @sbf/domain. Assim a saída
// de `splitEvenly()` entra direto em `allocateTransaction()` sem tradução no
// meio.
// ══════════════════════════════════════════════════════════════

const uuidSchema = z.string().uuid();

/**
 * Teto defensivo para o tamanho do conjunto: uma movimentação rateada entre
 * mais de 100 centros é erro de cliente, não caso de uso.
 */
const MAX_ALLOCATIONS_PER_TRANSACTION = 100;

export const allocationInputSchema = z.object({
  financialCenterId: uuidSchema,
  /** Sempre positivo: o sinal vem da movimentação, não do rateio. */
  amount: z.number().finite().positive(),
  note: z.string().max(500).nullish(),
});

export const allocationSetSchema = z
  .array(allocationInputSchema)
  .max(MAX_ALLOCATIONS_PER_TRANSACTION);

export const allocateTransactionSchema = z.object({
  transactionId: uuidSchema,
  /** Lista vazia é válida: significa "apagar todos os rateios". */
  allocations: allocationSetSchema,
});

export type AllocationInputPayload = z.infer<typeof allocationInputSchema>;
export type AllocateTransactionInput = z.infer<typeof allocateTransactionSchema>;
