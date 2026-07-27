/**
 * Contrato de draft de dívida.
 *
 * Alvo: tabela `liabilities`.
 *
 * O gerador antigo mapeava `DRAFT_BUILDERS.liability = buildTransactionDraft`,
 * um "fallback para MVP" que produzia campos de transação e era então validado
 * contra o schema de dívida — falha garantida. Aqui existe um builder real.
 */
import { z } from "zod";
import {
  DraftBaseSchema,
  IsoDateSchema,
  LiabilityTypeSchema,
  MoneyCentsSchema,
  UuidSchema,
  centsToNumeric,
} from "./common";

export const LiabilityDraftV1Schema = DraftBaseSchema.extend({
  draft_type: z.literal("liability"),

  name: z.string().nullable().default(null),
  liability_type: LiabilityTypeSchema.nullable().default(null),

  original_amount_cents: MoneyCentsSchema.nullable().default(null),
  outstanding_balance_cents: MoneyCentsSchema.nullable().default(null),

  total_installments: z.number().int().positive().nullable().default(null),
  interest_rate: z.number().nullable().default(null),
  rate_type: z.enum(["monthly", "annual"]).nullable().default(null),
  amortization_system: z.enum(["sac", "price", "mixed", "other", "none"]).nullable().default(null),

  start_date: IsoDateSchema.nullable().default(null),
  end_date: IsoDateSchema.nullable().default(null),

  financial_product_id: UuidSchema.nullable().default(null),
  supplier_name_raw: z.string().nullable().default(null),
  contract_identifier: z.string().nullable().default(null),
});
export type LiabilityDraftV1 = z.infer<typeof LiabilityDraftV1Schema>;

export const LiabilityPostableSchema = LiabilityDraftV1Schema.superRefine((v, ctx) => {
  if (!v.financial_product_id) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["financial_product_id"],
      message: "Selecione o produto financeiro desta dívida",
    });
  }
  if (!v.name) {
    ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["name"], message: "Nome é obrigatório" });
  }
  if (!v.liability_type) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["liability_type"],
      message: "Tipo da dívida é obrigatório",
    });
  }
  if (v.original_amount_cents === null || v.original_amount_cents <= 0) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["original_amount_cents"],
      message: "Valor original deve ser maior que zero",
    });
  }
  if (v.outstanding_balance_cents === null || v.outstanding_balance_cents < 0) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["outstanding_balance_cents"],
      message: "Saldo devedor é obrigatório e não pode ser negativo",
    });
  }
});

export interface LiabilityInsert {
  financial_product_id: string;
  name: string;
  type: string;
  original_amount: number;
  outstanding_balance: number;
  total_installments: number | null;
  interest_rate: number | null;
  rate_type: string | null;
  amortization_system: string | null;
  start_date: string | null;
  end_date: string | null;
}

export function toLiabilityInsert(draft: LiabilityDraftV1): LiabilityInsert {
  return {
    financial_product_id: draft.financial_product_id!,
    name: draft.name!,
    type: draft.liability_type!,
    original_amount: centsToNumeric(draft.original_amount_cents!),
    outstanding_balance: centsToNumeric(draft.outstanding_balance_cents!),
    total_installments: draft.total_installments,
    interest_rate: draft.interest_rate,
    rate_type: draft.rate_type,
    amortization_system: draft.amortization_system,
    start_date: draft.start_date,
    end_date: draft.end_date,
  };
}
