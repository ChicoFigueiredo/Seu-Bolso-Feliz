/**
 * Contrato de draft de métrica de consumo (kWh, m³, litros).
 *
 * Alvo: tabela `consumption_metrics` — NÃO `transactions`.
 *
 * O materializador antigo roteava `consumption_metric` para
 * `materializeTransaction`. Como `classifyDraftTypes` emite *tanto*
 * `transaction` quanto `consumption_metric` para uma conta de luz, aprovar
 * uma única conta da CEMIG lançaria o mesmo valor duas vezes em
 * `transactions`. Rotear para a tabela certa é o que elimina a contagem dupla.
 */
import { z } from "zod";
import {
  DraftBaseSchema,
  IsoDateSchema,
  MoneyCentsSchema,
  UuidSchema,
  centsToNumeric,
} from "./common";

export const ConsumptionMetricDraftV1Schema = DraftBaseSchema.extend({
  draft_type: z.literal("consumption_metric"),

  supplier_name_raw: z.string().nullable().default(null),
  supplier_id: UuidSchema.nullable().default(null),
  supplier_contract_id: UuidSchema.nullable().default(null),
  contract_identifier: z.string().nullable().default(null),

  /** `consumption_metrics` exige os dois como NOT NULL. */
  reference_period_start: IsoDateSchema.nullable().default(null),
  reference_period_end: IsoDateSchema.nullable().default(null),

  /** Ex.: "energia_eletrica" / "kWh". A CHECK chk_metric_or_attribute exige o trio. */
  metric_name: z.string().nullable().default(null),
  metric_unit: z.string().nullable().default(null),
  quantity: z.number().nullable().default(null),

  unit_price: z.number().nullable().default(null),
  subtotal_cents: MoneyCentsSchema.nullable().default(null),
});
export type ConsumptionMetricDraftV1 = z.infer<typeof ConsumptionMetricDraftV1Schema>;

/**
 * Exige o que a tabela exige, incluindo a CHECK `chk_metric_or_attribute`:
 * quantity + metric_name + metric_unit precisam existir juntos.
 */
export const ConsumptionMetricPostableSchema = ConsumptionMetricDraftV1Schema.superRefine(
  (v, ctx) => {
    if (!v.supplier_id) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["supplier_id"],
        message: "Fornecedor canônico é obrigatório",
      });
    }
    if (!v.reference_period_start) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["reference_period_start"],
        message: "Início do período de referência é obrigatório",
      });
    }
    if (!v.reference_period_end) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["reference_period_end"],
        message: "Fim do período de referência é obrigatório",
      });
    }
    const trio = [v.quantity, v.metric_name, v.metric_unit];
    if (trio.some((x) => x !== null) && trio.some((x) => x === null)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["quantity"],
        message: "quantity, metric_name e metric_unit precisam ser informados juntos",
      });
    }
  },
);

export interface ConsumptionMetricInsert {
  supplier_id: string;
  supplier_contract_id: string | null;
  reference_period_start: string;
  reference_period_end: string;
  metric_name: string | null;
  metric_unit: string | null;
  quantity: number | null;
  unit_price: number | null;
  subtotal: number | null;
  metadata: Record<string, unknown>;
}

export function toConsumptionMetricInsert(
  draft: ConsumptionMetricDraftV1,
): ConsumptionMetricInsert {
  return {
    supplier_id: draft.supplier_id!,
    supplier_contract_id: draft.supplier_contract_id,
    reference_period_start: draft.reference_period_start!,
    reference_period_end: draft.reference_period_end!,
    metric_name: draft.metric_name,
    metric_unit: draft.metric_unit,
    quantity: draft.quantity,
    unit_price: draft.unit_price,
    subtotal: draft.subtotal_cents === null ? null : centsToNumeric(draft.subtotal_cents),
    metadata: {
      // Satisfaz o ramo "atributo" da CHECK quando não há métrica quantificada.
      type: draft.quantity === null ? "attribute" : "metric",
      contract_identifier: draft.contract_identifier,
      supplier_name_raw: draft.supplier_name_raw,
      obligation_id: draft.obligation_id,
      provenance: draft.provenance,
    },
  };
}
