/**
 * Blocos comuns a todos os contratos de draft.
 *
 * A causa raiz do cisma entre gerador e materializador era confundir
 * "o que o worker honestamente sabe" com "o que o banco exige". Por isso
 * cada tipo de draft tem DOIS schemas:
 *
 *   - `...DraftV1Schema`     camada honesta. Aceita nulos onde a extração
 *                            legitimamente não sabe (conta, categoria, data
 *                            de evento de uma conta a vencer).
 *   - `...PostableSchema`    camada de lançamento. Refina a honesta exigindo
 *                            tudo que a tabela alvo exige como NOT NULL.
 *
 * Um draft recém-gerado deve passar na primeira e reprovar na segunda. Isso
 * não é um defeito: é a codificação de "falta o humano escolher a conta".
 */
import { z } from "zod";

/** Versão corrente do contrato de draft. Espelhada em draft_records.draft_schema_version. */
export const DRAFT_SCHEMA_VERSION = 1 as const;

/**
 * Direção do dinheiro.
 *
 * Substitui o cisma `"despesa"` (gerador) vs `"expense"` (materializador) por um
 * termo que nenhum dos dois lados possuía. `transaction_type` continua existindo
 * para o enum exato do banco, mas é anulável no momento do draft.
 */
export const DirectionSchema = z.enum(["outflow", "inflow"]);
export type Direction = z.infer<typeof DirectionSchema>;

/** Enum de `transactions.type`. */
export const TransactionTypeSchema = z.enum([
  "income",
  "expense",
  "refund",
  "adjustment",
  "interest_charge",
  "fee",
  "statement_payment",
  "liability_payment",
]);
export type TransactionType = z.infer<typeof TransactionTypeSchema>;

/** Enum de `recurring_templates.type`. */
export const RecurringTypeSchema = z.enum([
  "income",
  "expense",
  "liability_payment",
  "statement_payment",
]);

/** Enum de `recurring_templates.frequency`. */
export const FrequencySchema = z.enum([
  "monthly",
  "weekly",
  "biweekly",
  "quarterly",
  "annual",
  "custom",
]);
export type Frequency = z.infer<typeof FrequencySchema>;

/** Enum de prioridade, compartilhado por transactions e recurring_templates. */
export const PrioritySchema = z.enum(["essential", "high", "medium", "low", "optional"]);

/** Enum de `liabilities.type`. */
export const LiabilityTypeSchema = z.enum([
  "personal_loan",
  "mortgage",
  "overdraft",
  "installment_plan",
  "other",
]);

/**
 * Dinheiro em centavos inteiros.
 *
 * Ponto flutuante não representa 0,1 exatamente, e o gerador antigo repassava
 * `total_amount` cru — que o Supabase devolve como *string* para `numeric`.
 * Centavos inteiros eliminam os dois problemas e casam com a unidade que a
 * identity key já usa.
 */
export const MoneyCentsSchema = z.number().int();

/** Data ISO `YYYY-MM-DD`. Rejeita timestamp completo para não gravar fuso em coluna `date`. */
export const IsoDateSchema = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/, "Data deve estar em formato YYYY-MM-DD");

export const UuidSchema = z.string().uuid();

/**
 * De onde cada dado veio. Sem isso, um valor errado numa revisão é
 * irrastreável até o parser que o produziu.
 */
export const ProvenanceSchema = z.object({
  source_document_id: UuidSchema.nullable().default(null),
  extraction_result_id: UuidSchema.nullable().default(null),
  parsed_version_id: UuidSchema.nullable().default(null),
  parser_type: z.string().nullable().default(null),
  /** Mapa campo → origem, ex.: `{ total_amount: "boleto_parser" }`. */
  field_sources: z.record(z.string()).default({}),
});
export type Provenance = z.infer<typeof ProvenanceSchema>;

/** Campos presentes em todo draft, qualquer que seja o tipo. */
export const DraftBaseSchema = z.object({
  schema_version: z.literal(DRAFT_SCHEMA_VERSION),
  provenance: ProvenanceSchema,
  /** Preenchido por P0-7 quando a evidência converge para uma obrigação canônica. */
  obligation_id: UuidSchema.nullable().default(null),
});

/** Tipos de draft aceitos por `draft_records.draft_type`. */
export const DRAFT_TYPES = [
  "transaction",
  "recurring_template",
  "consumption_metric",
  "liability",
] as const;
export type DraftType = (typeof DRAFT_TYPES)[number];

/**
 * Converte centavos inteiros para o `numeric(15,2)` que as tabelas usam.
 * Centralizado para que a divisão por 100 exista em um lugar só.
 */
export function centsToNumeric(cents: number): number {
  return Math.round(cents) / 100;
}

/**
 * Converte um valor monetário vindo da extração para centavos inteiros.
 *
 * Aceita number e string porque o driver do Supabase devolve `numeric` como
 * string — a origem exata do `amount` inválido que reprovava todo draft.
 */
export function toCents(value: unknown): number | null {
  if (value === null || value === undefined || value === "") return null;
  const n = typeof value === "string" ? Number(value) : value;
  if (typeof n !== "number" || !Number.isFinite(n)) return null;
  return Math.round(n * 100);
}

/** Normaliza para `YYYY-MM-DD`, devolvendo null quando não há data utilizável. */
export function toIsoDate(value: unknown): string | null {
  if (typeof value !== "string" || value.length === 0) return null;
  const match = /^(\d{4}-\d{2}-\d{2})/.exec(value);
  return match ? match[1]! : null;
}
