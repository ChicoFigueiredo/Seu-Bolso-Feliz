/** União dos contratos de draft e tabelas de despacho por tipo. */
import { z } from "zod";
import type { DraftType } from "./common";
import {
  ConsumptionMetricDraftV1Schema,
  ConsumptionMetricPostableSchema,
  toConsumptionMetricInsert,
} from "./consumption-metric";
import { LiabilityDraftV1Schema, LiabilityPostableSchema, toLiabilityInsert } from "./liability";
import {
  RecurringCandidatePostableSchema,
  RecurringCandidateV1Schema,
  toRecurringTemplateInsert,
} from "./recurring-candidate";
import {
  TransactionDraftV1Schema,
  TransactionPostableSchema,
  toTransactionInsert,
} from "./transaction";

export * from "./common";
export * from "./transaction";
export * from "./consumption-metric";
export * from "./liability";
export * from "./recurring-candidate";

export const DraftPayloadV1Schema = z.discriminatedUnion("draft_type", [
  TransactionDraftV1Schema,
  ConsumptionMetricDraftV1Schema,
  LiabilityDraftV1Schema,
  RecurringCandidateV1Schema,
]);
export type DraftPayloadV1 = z.infer<typeof DraftPayloadV1Schema>;

/** Schema honesto por tipo — o que o worker precisa produzir. */
export const DRAFT_SCHEMAS = {
  transaction: TransactionDraftV1Schema,
  consumption_metric: ConsumptionMetricDraftV1Schema,
  liability: LiabilityDraftV1Schema,
  recurring_template: RecurringCandidateV1Schema,
} as const;

/** Schema de lançamento por tipo — o que a tabela alvo exige. */
export const POSTABLE_SCHEMAS = {
  transaction: TransactionPostableSchema,
  consumption_metric: ConsumptionMetricPostableSchema,
  liability: LiabilityPostableSchema,
  recurring_template: RecurringCandidatePostableSchema,
} as const;

/**
 * Tabela alvo de cada tipo de draft.
 *
 * `consumption_metric` aponta para `consumption_metrics`, não `transactions`.
 * O roteamento antigo para `transactions` fazia uma conta de luz ser lançada
 * duas vezes.
 */
export const TARGET_TABLES: Record<DraftType, string> = {
  transaction: "transactions",
  consumption_metric: "consumption_metrics",
  liability: "liabilities",
  recurring_template: "recurring_templates",
};

/** Conversores draft → payload de INSERT, por tipo. */
export const TO_INSERT = {
  transaction: toTransactionInsert,
  consumption_metric: toConsumptionMetricInsert,
  liability: toLiabilityInsert,
  recurring_template: toRecurringTemplateInsert,
} as const;
