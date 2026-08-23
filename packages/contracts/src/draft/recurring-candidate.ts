/**
 * Contrato de CANDIDATO a recorrência.
 *
 * O §8 do plano mestre é explícito: a saída deve ser um candidato, nunca um
 * template criado silenciosamente. Uma conta com competência mensal não prova
 * recorrência — prova que existe uma conta.
 *
 * Por isso `RecurringCandidatePostableSchema` exige `evidence_count >= 2`:
 * é estruturalmente impossível materializar uma recorrência a partir de um
 * único documento, que era exatamente o que `classifyDraftTypes` fazia ao
 * disparar com `hasSupplier && hasCompetence` e cravar `recurrence: "monthly"`.
 *
 * O valor gravado em `draft_records.draft_type` continua sendo
 * `"recurring_template"` porque a CHECK da tabela só aceita os quatro valores
 * originais. O que muda é a semântica do payload e a regra de lançamento.
 */
import { z } from "zod";
import {
  DraftBaseSchema,
  FrequencySchema,
  IsoDateSchema,
  MoneyCentsSchema,
  PrioritySchema,
  RecurringTypeSchema,
  UuidSchema,
  centsToNumeric,
} from "./common";

/** Uma ocorrência observada que sustenta o candidato. */
export const RecurrenceOccurrenceSchema = z.object({
  event_date: IsoDateSchema,
  amount_cents: MoneyCentsSchema.nullable().default(null),
  source_document_id: UuidSchema.nullable().default(null),
});

export const RecurringCandidateV1Schema = DraftBaseSchema.extend({
  draft_type: z.literal("recurring_template"),

  name: z.string().nullable().default(null),
  recurring_type: RecurringTypeSchema.nullable().default(null),

  /** Nulo até que o motor de recorrência infira a periodicidade do histórico. */
  frequency: FrequencySchema.nullable().default(null),
  day_of_month: z.number().int().min(1).max(31).nullable().default(null),

  amount_cents: MoneyCentsSchema.nullable().default(null),
  is_variable_amount: z.boolean().default(false),

  starts_at: IsoDateSchema.nullable().default(null),
  ends_at: IsoDateSchema.nullable().default(null),

  supplier_name_raw: z.string().nullable().default(null),
  supplier_id: UuidSchema.nullable().default(null),
  financial_product_id: UuidSchema.nullable().default(null),
  category_suggestion: z.string().nullable().default(null),
  category_id: UuidSchema.nullable().default(null),
  priority: PrioritySchema.nullable().default(null),
  notes: z.string().nullable().default(null),

  /** Quantas ocorrências distintas sustentam este candidato. */
  evidence_count: z.number().int().nonnegative().default(0),
  occurrences: z.array(RecurrenceOccurrenceSchema).default([]),
  /** Por que o motor acredita nisto — exigência de explicabilidade do §8. */
  explanation: z.array(z.string()).default([]),
  confidence: z.number().min(0).max(1).nullable().default(null),
});
export type RecurringCandidateV1 = z.infer<typeof RecurringCandidateV1Schema>;

export const RecurringCandidatePostableSchema = RecurringCandidateV1Schema.superRefine((v, ctx) => {
  if (v.evidence_count < 2) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["evidence_count"],
      message:
        "Recorrência exige ao menos 2 ocorrências observadas; um documento isolado não prova periodicidade",
    });
  }
  if (!v.name) {
    ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["name"], message: "Nome é obrigatório" });
  }
  if (!v.recurring_type) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["recurring_type"],
      message: "Tipo da recorrência é obrigatório",
    });
  }
  if (!v.frequency) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["frequency"],
      message: "Periodicidade é obrigatória e deve vir do histórico, não de um padrão fixo",
    });
  }
});

export interface RecurringTemplateInsert {
  financial_product_id: string | null;
  name: string;
  type: string;
  amount: number | null;
  is_variable_amount: boolean;
  frequency: string;
  day_of_month: number | null;
  category_id: string | null;
  priority: string | null;
  starts_at: string | null;
  ends_at: string | null;
  notes: string | null;
}

export function toRecurringTemplateInsert(draft: RecurringCandidateV1): RecurringTemplateInsert {
  return {
    financial_product_id: draft.financial_product_id,
    name: draft.name!,
    type: draft.recurring_type!,
    amount: draft.amount_cents === null ? null : centsToNumeric(draft.amount_cents),
    is_variable_amount: draft.is_variable_amount,
    frequency: draft.frequency!,
    day_of_month: draft.day_of_month,
    category_id: draft.category_id,
    priority: draft.priority,
    starts_at: draft.starts_at,
    ends_at: draft.ends_at,
    notes: draft.notes,
  };
}
