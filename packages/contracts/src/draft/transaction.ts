/**
 * Contrato de draft de transação.
 *
 * Alvo: tabela `transactions`, cujos NOT NULL são
 * `user_id, financial_product_id, type, amount, event_date`.
 */
import { z } from "zod";
import {
  DRAFT_SCHEMA_VERSION,
  DirectionSchema,
  DraftBaseSchema,
  IsoDateSchema,
  MoneyCentsSchema,
  PrioritySchema,
  TransactionTypeSchema,
  UuidSchema,
  centsToNumeric,
} from "./common";

/**
 * Camada honesta: o que o worker consegue afirmar a partir de um documento.
 *
 * `event_date` é anulável de propósito. Uma conta a vencer não tem data de
 * evento — ela tem vencimento. Exigir `event_date` no draft (como o schema
 * antigo fazia com `.min(1)`) é o que reprovava 100% dos drafts gerados.
 */
export const TransactionDraftV1Schema = DraftBaseSchema.extend({
  draft_type: z.literal("transaction"),

  direction: DirectionSchema,
  /** Enum exato do banco. Nulo enquanto a classificação não é conclusiva. */
  transaction_type: TransactionTypeSchema.nullable().default(null),

  amount_cents: MoneyCentsSchema.nullable().default(null),
  currency: z.string().default("BRL"),

  event_date: IsoDateSchema.nullable().default(null),
  due_date: IsoDateSchema.nullable().default(null),
  competence_date: IsoDateSchema.nullable().default(null),

  description: z.string().nullable().default(null),

  supplier_name_raw: z.string().nullable().default(null),
  supplier_id: UuidSchema.nullable().default(null),

  /**
   * Categoria como texto sugerido E como id canônico — dois campos distintos,
   * nunca um só campo polimórfico. O gerador antigo produzia `category` textual
   * e o materializador esperava `category_id` uuid; escrever ambos torna a
   * lacuna explícita em vez de silenciosa.
   */
  category_suggestion: z.string().nullable().default(null),
  category_id: UuidSchema.nullable().default(null),

  tags: z.array(z.string()).default([]),
  priority: PrioritySchema.nullable().default(null),

  document_number: z.string().nullable().default(null),
  contract_identifier: z.string().nullable().default(null),
  barcode_digitable_line: z.string().nullable().default(null),

  /** Conta/cartão. O pipeline não consegue inferir: exige escolha humana. */
  financial_product_id: UuidSchema.nullable().default(null),

  notes: z.string().nullable().default(null),
});
export type TransactionDraftV1 = z.infer<typeof TransactionDraftV1Schema>;

/**
 * Camada de lançamento: exige tudo que `transactions` exige como NOT NULL.
 *
 * Usa superRefine em vez de um schema separado para que os campos não possam
 * divergir do contrato honesto com o tempo.
 */
export const TransactionPostableSchema = TransactionDraftV1Schema.superRefine((v, ctx) => {
  if (!v.financial_product_id) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["financial_product_id"],
      message: "Selecione a conta ou cartão em que este lançamento entra",
    });
  }
  if (!v.transaction_type) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["transaction_type"],
      message: "Tipo do lançamento é obrigatório",
    });
  }
  if (v.amount_cents === null || v.amount_cents <= 0) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["amount_cents"],
      message: "Valor deve ser maior que zero",
    });
  }
  if (!v.event_date) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["event_date"],
      message: "Data do evento é obrigatória para lançar",
    });
  }
});

/** Payload de INSERT em `transactions`. Espelha exatamente as colunas da tabela. */
export interface TransactionInsert {
  financial_product_id: string;
  type: string;
  amount: number;
  description: string | null;
  event_date: string;
  competence_date: string | null;
  category_id: string | null;
  priority: string | null;
  notes: string | null;
  origin_type: "import";
  is_confirmed: boolean;
  metadata: Record<string, unknown>;
}

/** Conversão explícita draft → linha de banco. Sem spread: nada vaza por acidente. */
export function toTransactionInsert(draft: TransactionDraftV1): TransactionInsert {
  return {
    financial_product_id: draft.financial_product_id!,
    type: draft.transaction_type!,
    amount: centsToNumeric(draft.amount_cents!),
    description: draft.description ?? draft.supplier_name_raw,
    event_date: draft.event_date!,
    competence_date: draft.competence_date,
    category_id: draft.category_id,
    priority: draft.priority,
    notes: draft.notes,
    origin_type: "import",
    is_confirmed: true,
    metadata: {
      schema_version: DRAFT_SCHEMA_VERSION,
      supplier_id: draft.supplier_id,
      supplier_name_raw: draft.supplier_name_raw,
      document_number: draft.document_number,
      contract_identifier: draft.contract_identifier,
      due_date: draft.due_date,
      tags: draft.tags,
      obligation_id: draft.obligation_id,
      provenance: draft.provenance,
    },
  };
}
