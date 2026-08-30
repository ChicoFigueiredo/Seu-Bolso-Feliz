/**
 * Schema Drizzle do Neon — gerado por `drizzle-kit introspect`
 * (dentro de packages/db: `bunx drizzle-kit introspect`, ou o script
 * `db:introspect` do package.json) a partir do banco real, MAS COM EDIÇÕES
 * MANUAIS. Não é 100% auto-gerado, apesar de estar na lista de ignorados do
 * eslint.config.ts junto com os arquivos que são.
 *
 * `drizzle.config.ts` usa `introspect.casing: "preserve"`: os nomes de coluna
 * saem snake_case (iguais ao Postgres), sem alias camelCase. Os identificadores
 * de tabela (`export const ...`) TAMBÉM saem snake_case (ex.: `financial_periods`,
 * `recurring_templates`, `user_secrets`) — não camelCase.
 *
 * ⚠️ Rodar a introspecção de novo SOBRESCREVE este arquivo e apaga as edições
 * abaixo em silêncio. Se você reintrospectar, reaplique-as:
 *
 *   1. `transactions.recurring_instance_id` — a FK para `recurring_instances`
 *      é declarada inline na coluna, via
 *      `.references((): AnyPgColumn => recurring_instances.id, { onDelete: "set null" })`,
 *      em vez do `foreignKey({...})` builder na lista de constraints da
 *      tabela (que é o que a introspecção gera). Motivo: `transactions` e
 *      `recurring_instances` se referenciam mutuamente e o builder produz um
 *      ciclo de inferência de tipos — `tsc` falha com TS7022/TS7024. O
 *      callback anotado com `AnyPgColumn` quebra o ciclo (mesmo padrão que o
 *      próprio drizzle-kit usa para auto-referências, ex. `categories.parent_id`).
 *      Efeito colateral aceito: o nome do constraint passa a ser o default do
 *      Drizzle em vez de `transactions_recurring_instance_id_fkey` — irrelevante
 *      aqui, porque este schema NUNCA é usado para `drizzle-kit generate`/`push`
 *      (a fonte da verdade do DDL é packages/db/schema.sql, aplicado no Neon).
 *
 *   2. `user_secrets` — o parâmetro do callback de constraints é `_table`, não
 *      `table`. O índice único `uq_user_secrets_scope` usa `sql\`\`` cru
 *      (COALESCE não é representável encadeando `table.coluna`), então o
 *      parâmetro fica sem uso e o `noUnusedParameters` do tsc reclama (TS6133).
 *
 * ⚠️ Passo manual de local: `drizzle.config.ts` tem `out: "./drizzle"` (o
 * drizzle-kit escreve schema.ts + relations.ts + snapshot SQL + meta/ todos
 * nessa mesma pasta; apontar `out` para `./src` sujaria src/ com artefatos de
 * migration). Depois de rodar a introspecção, mova à mão:
 *
 *     packages/db/drizzle/schema.ts     -> packages/db/src/schema.ts
 *     packages/db/drizzle/relations.ts  -> packages/db/src/relations.ts
 *
 * e descarte o resto (packages/db/drizzle/ é gitignorado justamente por isso).
 *
 * ⚠️ Toda coluna `numeric(...)` precisa de `mode: "number"` no objeto de
 * opções (a introspecção não adiciona isso sozinha) — sem ele o Drizzle
 * devolve `string`, não `number`. Reaplicar em toda ocorrência depois de
 * reintrospectar.
 *
 * ⚠️ Toda coluna `jsonb()` precisa de `.$type<Json>()` encadeado logo depois
 * (import `Json` de `@sbf/shared-types`) — sem isso o Drizzle infere
 * `unknown`, que quebra o typecheck de qualquer componente/página que ainda
 * espera o tipo `Json` antigo (gerado pelo Supabase CLI) vindo das Server
 * Actions em `apps/web/src/app/actions/`. Achado na Fase 4a (Task 8,
 * ingestion.ts): sem essa anotação, `bun run typecheck` falha em vários
 * componentes de `apps/web/src/app/dashboard/` e `apps/web/src/components/`
 * que nunca são tocados pelas Server Actions em si — o tipo `unknown` se
 * propaga pelos tipos de retorno inferidos (`typeof schema.<tabela>.$inferSelect`)
 * até esses consumidores. Reaplicar em toda ocorrência de `jsonb()` depois de
 * reintrospectar.
 */
import {
  pgTable,
  uniqueIndex,
  check,
  uuid,
  text,
  timestamp,
  integer,
  index,
  foreignKey,
  unique,
  jsonb,
  numeric,
  boolean,
  inet,
  date,
  bigint,
  smallint,
  type AnyPgColumn,
  primaryKey,
  pgView,
  pgMaterializedView,
} from "drizzle-orm/pg-core";
import { sql } from "drizzle-orm";
import type { Json } from "@sbf/shared-types";

export const user_secrets = pgTable(
  "user_secrets",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    secret_type: text().notNull(),
    entity_type: text(),
    entity_id: uuid(),
    encrypted_value: text().notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    encryption_version: integer().default(1).notNull(),
    contract_identifier: text(),
    label: text(),
    last_used_at: timestamp({ withTimezone: true, mode: "string" }),
    success_count: integer().default(0).notNull(),
    // `table` não é usado aqui: o índice único abaixo precisou cair para sql`` cru
    // (COALESCE não é representável via table.coluna encadeado), então o parâmetro
    // fica sem uso — prefixado com "_" para o `noUnusedParameters` do tsc.
    // (Edição manual nº 2 — ver cabeçalho deste arquivo.)
  },
  (_table) => [
    uniqueIndex("uq_user_secrets_scope").using(
      "btree",
      sql`user_id`,
      sql`secret_type`,
      sql`COALESCE(entity_type, '-'::text)`,
      sql`COALESCE(entity_id, '00000000-0000-0000-0000-000000000000'::uui`,
      sql`COALESCE(contract_identifier, '-'::text)`,
    ),
    check("chk_user_secrets_encrypted", sql`encryption_version >= 1`),
    check(
      "chk_user_secrets_entity_type",
      sql`(entity_type IS NULL) OR (entity_type = ANY (ARRAY['supplier'::text, 'financial_product'::text, 'card'::text, 'supplier_contract'::text, 'institution'::text]))`,
    ),
  ],
);

export const document_patterns = pgTable(
  "document_patterns",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    supplier_id: uuid(),
    document_type: text().notNull(),
    institution_id: uuid(),
    extraction_rules: jsonb().$type<Json>().default({}).notNull(),
    field_mappings: jsonb().$type<Json>().default({}).notNull(),
    sample_fingerprints: text().array().default([""]).notNull(),
    confidence_threshold: numeric({ precision: 3, scale: 2, mode: "number" })
      .default(0.8)
      .notNull(),
    version: integer().default(1).notNull(),
    is_active: boolean().default(true).notNull(),
    feedback_count: integer().default(0).notNull(),
    success_count: integer().default(0).notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_document_patterns_institution")
      .using("btree", table.institution_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(is_active = true)`),
    index("idx_document_patterns_supplier_type")
      .using(
        "btree",
        table.supplier_id.asc().nullsLast().op("text_ops"),
        table.document_type.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(is_active = true)`),
    index("idx_document_patterns_user_active").using(
      "btree",
      table.user_id.asc().nullsLast().op("bool_ops"),
      table.is_active.asc().nullsLast().op("bool_ops"),
    ),
    foreignKey({
      columns: [table.institution_id],
      foreignColumns: [institutions.id],
      name: "document_patterns_institution_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "document_patterns_supplier_id_fkey",
    }).onDelete("set null"),
    unique("document_patterns_user_id_name_version_key").on(
      table.name,
      table.user_id,
      table.version,
    ),
    check(
      "document_patterns_confidence_threshold_check",
      sql`(confidence_threshold >= 0.00) AND (confidence_threshold <= 1.00)`,
    ),
    check("document_patterns_feedback_count_check", sql`feedback_count >= 0`),
    check("document_patterns_success_count_check", sql`success_count >= 0`),
  ],
);

export const pattern_feedback = pgTable(
  "pattern_feedback",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    pattern_id: uuid().notNull(),
    source_document_id: uuid(),
    feedback_type: text().notNull(),
    corrections: jsonb().$type<Json>().default({}).notNull(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_pattern_feedback_document")
      .using("btree", table.source_document_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("idx_pattern_feedback_pattern").using(
      "btree",
      table.pattern_id.asc().nullsLast().op("timestamptz_ops"),
      table.created_at.desc().nullsFirst().op("timestamptz_ops"),
    ),
    index("idx_pattern_feedback_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
      table.created_at.desc().nullsFirst().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.pattern_id],
      foreignColumns: [document_patterns.id],
      name: "pattern_feedback_pattern_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "pattern_feedback_source_document_id_fkey",
    }).onDelete("set null"),
    check(
      "pattern_feedback_feedback_type_check",
      sql`feedback_type = ANY (ARRAY['correct'::text, 'incorrect'::text, 'partial'::text, 'improved'::text])`,
    ),
  ],
);

export const audit_logs = pgTable(
  "audit_logs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    action: text().notNull(),
    entity_type: text(),
    entity_id: uuid(),
    old_values: jsonb().$type<Json>(),
    new_values: jsonb().$type<Json>(),
    ip_address: inet(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_audit_logs_entity").using(
      "btree",
      table.entity_type.asc().nullsLast().op("text_ops"),
      table.entity_id.asc().nullsLast().op("text_ops"),
    ),
    index("idx_audit_logs_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
  ],
);

export const cards = pgTable(
  "cards",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    financial_product_id: uuid().notNull(),
    last_four_digits: text(),
    card_brand: text(),
    is_primary: boolean().default(true).notNull(),
    holder_name: text(),
    credit_limit: numeric({ precision: 15, scale: 2, mode: "number" }),
    closing_day: integer(),
    due_day: integer(),
    is_active: boolean().default(true).notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_cards_financial_product_id").using(
      "btree",
      table.financial_product_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_cards_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.financial_product_id],
      foreignColumns: [financial_products.id],
      name: "cards_financial_product_id_fkey",
    }).onDelete("cascade"),
    check("cards_closing_day_check", sql`(closing_day >= 1) AND (closing_day <= 31)`),
    check("cards_due_day_check", sql`(due_day >= 1) AND (due_day <= 31)`),
  ],
);

export const categories = pgTable(
  "categories",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    parent_id: uuid(),
    icon: text(),
    color: text(),
    display_order: integer(),
    is_active: boolean().default(true).notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_categories_parent_id").using(
      "btree",
      table.parent_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_categories_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.parent_id],
      foreignColumns: [table.id],
      name: "categories_parent_id_fkey",
    }).onDelete("set null"),
  ],
);

export const consumption_metrics = pgTable(
  "consumption_metrics",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    supplier_id: uuid().notNull(),
    supplier_contract_id: uuid(),
    transaction_id: uuid(),
    document_id: uuid(),
    reference_period_start: date().notNull(),
    reference_period_end: date().notNull(),
    metric_name: text(),
    metric_unit: text(),
    quantity: numeric({ precision: 15, scale: 4, mode: "number" }),
    unit_price: numeric({ precision: 15, scale: 6, mode: "number" }),
    subtotal: numeric({ precision: 15, scale: 2, mode: "number" }),
    metadata: jsonb().$type<Json>(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_consumption_metrics_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_consumption_metrics_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.document_id],
      foreignColumns: [documents_legacy.id],
      name: "consumption_metrics_document_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplier_contract_id],
      foreignColumns: [supplier_contracts.id],
      name: "consumption_metrics_supplier_contract_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "consumption_metrics_supplier_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transaction_id],
      foreignColumns: [transactions.id],
      name: "consumption_metrics_transaction_id_fkey",
    }).onDelete("set null"),
    check(
      "chk_metric_or_attribute",
      sql`((quantity IS NOT NULL) AND (metric_name IS NOT NULL) AND (metric_unit IS NOT NULL)) OR ((quantity IS NULL) AND (metadata IS NOT NULL) AND ((metadata ->> 'type'::text) = 'attribute'::text))`,
    ),
    check("chk_reference_period", sql`reference_period_end >= reference_period_start`),
  ],
);

export const document_splits = pgTable(
  "document_splits",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    source_document_id: uuid().notNull(),
    user_id: uuid().notNull(),
    category_id: uuid(),
    tags: uuid().array().default([""]).notNull(),
    amount: numeric({ precision: 14, scale: 2, mode: "number" }).notNull(),
    description: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_document_splits_category").using(
      "btree",
      table.category_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_splits_source_document").using(
      "btree",
      table.source_document_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_splits_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.category_id],
      foreignColumns: [categories.id],
      name: "document_splits_category_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "document_splits_source_document_id_fkey",
    }).onDelete("cascade"),
    check("document_splits_amount_positive", sql`amount > (0)::numeric`),
  ],
);

export const documents_legacy = pgTable(
  "documents_legacy",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    description: text(),
    file_path: text().notNull(),
    file_type: text(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    file_size: bigint({ mode: "number" }),
    document_type: text(),
    entity_type: text(),
    entity_id: uuid(),
    version: integer().default(1).notNull(),
    is_password_protected: boolean().default(false).notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    supplier_id: uuid(),
  },
  (table) => [
    index("idx_documents_entity").using(
      "btree",
      table.entity_type.asc().nullsLast().op("text_ops"),
      table.entity_id.asc().nullsLast().op("text_ops"),
    ),
    index("idx_documents_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_documents_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "documents_supplier_id_fkey",
    }).onDelete("set null"),
    check(
      "documents_document_type_check",
      sql`(document_type IS NULL) OR (document_type = ANY (ARRAY['receipt'::text, 'invoice'::text, 'statement'::text, 'contract'::text, 'proof'::text, 'other'::text]))`,
    ),
  ],
);

export const draft_records = pgTable(
  "draft_records",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    batch_id: uuid(),
    user_id: uuid().notNull(),
    source_document_id: uuid(),
    extraction_result_id: uuid(),
    draft_type: text().notNull(),
    status: text().default("pending_review").notNull(),
    draft_data: jsonb().$type<Json>().notNull(),
    corrections: jsonb().$type<Json>(),
    confidence_score: numeric({ precision: 3, scale: 2, mode: "number" }),
    approved_at: timestamp({ withTimezone: true, mode: "string" }),
    approved_by: uuid(),
    posted_record_id: uuid(),
    posted_record_type: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    reconciliation_status: text().default("not_checked").notNull(),
    reconciled_transaction_id: uuid(),
    reconciled_template_id: uuid(),
    reconciliation_candidates: jsonb().$type<Json>().default([]),
    reconciled_at: timestamp({ withTimezone: true, mode: "string" }),
    draft_schema_version: smallint().default(0).notNull(),
    draft_data_legacy: jsonb().$type<Json>(),
    materialization_key: text().notNull(),
    posted_at: timestamp({ withTimezone: true, mode: "string" }),
    materialization_error: jsonb().$type<Json>(),
    obligation_id: uuid(),
    external_ref: text(),
  },
  (table) => [
    index("idx_draft_records_batch")
      .using("btree", table.batch_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(batch_id IS NOT NULL)`),
    uniqueIndex("idx_draft_records_external_ref")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.external_ref.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(external_ref IS NOT NULL)`),
    index("idx_draft_records_reconciled_transaction")
      .using("btree", table.reconciled_transaction_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(reconciled_transaction_id IS NOT NULL)`),
    index("idx_draft_records_reconciliation_status")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.reconciliation_status.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(reconciliation_status <> ALL (ARRAY['not_checked'::text, 'no_match'::text]))`),
    index("idx_draft_records_review_queue")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.status.asc().nullsLast().op("text_ops"),
        table.created_at.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(status = 'pending_review'::text)`),
    index("idx_draft_records_source_doc")
      .using("btree", table.source_document_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("ix_draft_records_obligation")
      .using("btree", table.obligation_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(obligation_id IS NOT NULL)`),
    index("ix_draft_records_schema_version")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("int2_ops"),
        table.draft_schema_version.asc().nullsLast().op("int2_ops"),
      )
      .where(sql`(draft_schema_version = 0)`),
    uniqueIndex("uq_draft_records_materialization_key").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.materialization_key.asc().nullsLast().op("text_ops"),
    ),
    uniqueIndex("uq_draft_records_posted_record")
      .using(
        "btree",
        table.posted_record_type.asc().nullsLast().op("text_ops"),
        table.posted_record_id.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(posted_record_id IS NOT NULL)`),
    foreignKey({
      columns: [table.batch_id],
      foreignColumns: [draft_batches.id],
      name: "draft_records_batch_id_fkey",
    }),
    foreignKey({
      columns: [table.extraction_result_id],
      foreignColumns: [extraction_results.id],
      name: "draft_records_extraction_result_id_fkey",
    }),
    foreignKey({
      columns: [table.obligation_id],
      foreignColumns: [financial_obligations.id],
      name: "draft_records_obligation_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.reconciled_template_id],
      foreignColumns: [recurring_templates.id],
      name: "draft_records_reconciled_template_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.reconciled_transaction_id],
      foreignColumns: [transactions.id],
      name: "draft_records_reconciled_transaction_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "draft_records_source_document_id_fkey",
    }),
    check(
      "draft_records_draft_type_check",
      sql`draft_type = ANY (ARRAY['transaction'::text, 'recurring_template'::text, 'liability'::text, 'consumption_metric'::text])`,
    ),
    check(
      "draft_records_reconciliation_status_check",
      sql`reconciliation_status = ANY (ARRAY['not_checked'::text, 'no_match'::text, 'match_exact'::text, 'match_fuzzy'::text, 'match_duplicate'::text, 'match_recurrence'::text, 'confirmed_new'::text, 'confirmed_duplicate'::text])`,
    ),
    check(
      "draft_records_status_check",
      sql`status = ANY (ARRAY['pending_review'::text, 'approved'::text, 'posted'::text, 'rejected'::text, 'corrected'::text, 'archived'::text])`,
    ),
  ],
);

export const ai_chat_sessions = pgTable(
  "ai_chat_sessions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    title: text(),
    context_type: text(),
    context_id: uuid(),
    model: text().default("gpt-4o").notNull(),
    total_tokens_used: integer().default(0),
    message_count: integer().default(0),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ai_chat_sessions_updated").using(
      "btree",
      table.updated_at.desc().nullsFirst().op("timestamptz_ops"),
    ),
    index("idx_ai_chat_sessions_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
  ],
);

export const ai_chat_messages = pgTable(
  "ai_chat_messages",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    session_id: uuid().notNull(),
    user_id: uuid().notNull(),
    role: text().notNull(),
    content: text(),
    tool_calls: jsonb().$type<Json>(),
    tool_call_id: text(),
    tool_name: text(),
    tokens_used: integer(),
    latency_ms: integer(),
    model: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ai_chat_messages_session").using(
      "btree",
      table.session_id.asc().nullsLast().op("timestamptz_ops"),
      table.created_at.asc().nullsLast().op("timestamptz_ops"),
    ),
    index("idx_ai_chat_messages_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.session_id],
      foreignColumns: [ai_chat_sessions.id],
      name: "ai_chat_messages_session_id_fkey",
    }).onDelete("cascade"),
    check(
      "ai_chat_messages_role_check",
      sql`role = ANY (ARRAY['user'::text, 'assistant'::text, 'system'::text, 'tool'::text])`,
    ),
  ],
);

export const document_fingerprints = pgTable(
  "document_fingerprints",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    source_document_id: uuid().notNull(),
    user_id: uuid().notNull(),
    content_hash: text().notNull(),
    canonical_fingerprint: text(),
    hash_algorithm: text().default("sha256"),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_document_fingerprints_canonical")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.canonical_fingerprint.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(canonical_fingerprint IS NOT NULL)`),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "document_fingerprints_source_document_id_fkey",
    }),
    unique("document_fingerprints_user_id_content_hash_key").on(table.content_hash, table.user_id),
  ],
);

export const document_transactions = pgTable(
  "document_transactions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    source_document_id: uuid().notNull(),
    transaction_id: uuid().notNull(),
    user_id: uuid().notNull(),
    link_type: text().notNull(),
    confidence: numeric({ precision: 4, scale: 3, mode: "number" }),
    created_by: text().default("user").notNull(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_document_transactions_source").using(
      "btree",
      table.source_document_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_transactions_tx").using(
      "btree",
      table.transaction_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_transactions_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "document_transactions_source_document_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transaction_id],
      foreignColumns: [transactions.id],
      name: "document_transactions_transaction_id_fkey",
    }).onDelete("cascade"),
    unique("document_transactions_source_document_id_transaction_id_lin_key").on(
      table.link_type,
      table.source_document_id,
      table.transaction_id,
    ),
    check(
      "document_transactions_confidence_range",
      sql`(confidence IS NULL) OR ((confidence >= (0)::numeric) AND (confidence <= (1)::numeric))`,
    ),
    check(
      "document_transactions_created_by_check",
      sql`created_by = ANY (ARRAY['user'::text, 'ai'::text, 'pattern'::text])`,
    ),
    check(
      "document_transactions_link_type_check",
      sql`link_type = ANY (ARRAY['payment'::text, 'refund'::text, 'installment'::text, 'support'::text])`,
    ),
  ],
);

export const draft_batches = pgTable(
  "draft_batches",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    run_id: uuid(),
    name: text(),
    status: text().default("open").notNull(),
    total_drafts: integer().default(0),
    approved_count: integer().default(0),
    rejected_count: integer().default(0),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    source_document_id: uuid(),
  },
  (table) => [
    index("idx_draft_batches_source_document")
      .using("btree", table.source_document_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("idx_draft_batches_user_status").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    foreignKey({
      columns: [table.run_id],
      foreignColumns: [ingestion_runs.id],
      name: "draft_batches_run_id_fkey",
    }),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "draft_batches_source_document_id_fkey",
    }).onDelete("set null"),
    check(
      "draft_batches_status_check",
      sql`status = ANY (ARRAY['open'::text, 'reviewing'::text, 'approved'::text, 'partial'::text, 'rejected'::text])`,
    ),
  ],
);

export const external_account_mappings = pgTable(
  "external_account_mappings",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    provider_connection_id: uuid().notNull(),
    external_account_id: text().notNull(),
    external_account_name: text(),
    external_account_type: text(),
    financial_product_id: uuid(),
    status: text().default("pending_mapping").notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_external_account_mappings_connection").using(
      "btree",
      table.provider_connection_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_external_account_mappings_product").using(
      "btree",
      table.financial_product_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_external_account_mappings_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.financial_product_id],
      foreignColumns: [financial_products.id],
      name: "external_account_mappings_financial_product_id_fkey",
    }),
    foreignKey({
      columns: [table.provider_connection_id],
      foreignColumns: [provider_connections.id],
      name: "external_account_mappings_provider_connection_id_fkey",
    }).onDelete("cascade"),
    unique("external_account_mappings_provider_connection_id_external_a_key").on(
      table.external_account_id,
      table.provider_connection_id,
    ),
    check(
      "external_account_mappings_status_check",
      sql`status = ANY (ARRAY['pending_mapping'::text, 'mapped'::text, 'ignored'::text])`,
    ),
  ],
);

export const institutions = pgTable(
  "institutions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    type: text().default("bank").notNull(),
    icon_url: text(),
    color: text(),
    is_active: boolean().default(true).notNull(),
    display_order: integer(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_institutions_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    check(
      "institutions_type_check",
      sql`type = ANY (ARRAY['bank'::text, 'fintech'::text, 'broker'::text, 'other'::text])`,
    ),
  ],
);

export const financial_obligation_identity_keys = pgTable(
  "financial_obligation_identity_keys",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    obligation_id: uuid().notNull(),
    key: text().notNull(),
    key_kind: text().notNull(),
    strength: text().notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("ix_fo_identity_keys_obligation").using(
      "btree",
      table.obligation_id.asc().nullsLast().op("uuid_ops"),
    ),
    uniqueIndex("uq_fo_identity_keys").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.key.asc().nullsLast().op("text_ops"),
    ),
    foreignKey({
      columns: [table.obligation_id],
      foreignColumns: [financial_obligations.id],
      name: "financial_obligation_identity_keys_obligation_id_fkey",
    }).onDelete("cascade"),
    check(
      "financial_obligation_identity_keys_key_kind_check",
      sql`key_kind = ANY (ARRAY['barcode'::text, 'docnum'::text, 'card_cycle'::text, 'supplier_period_amount'::text, 'supplier_period'::text])`,
    ),
    check(
      "financial_obligation_identity_keys_strength_check",
      sql`strength = ANY (ARRAY['strong'::text, 'medium'::text, 'weak'::text])`,
    ),
  ],
);

export const ingestion_jobs = pgTable(
  "ingestion_jobs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    run_id: uuid().notNull(),
    user_id: uuid().notNull(),
    source_document_id: uuid(),
    status: text().default("discovered").notNull(),
    error_message: text(),
    error_details: jsonb().$type<Json>(),
    retry_count: integer().default(0),
    max_retries: integer().default(3),
    metadata: jsonb().$type<Json>().default({}),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    needs_full_ai_review: boolean().default(false),
    // Fila (migration 0001): a Vercel grava, os workers puxam com
    // FOR UPDATE SKIP LOCKED e acordam por NOTIFY. Nada é exposto na rede.
    locked_by: text(),
    locked_at: timestamp({ withTimezone: true, mode: "string" }),
    // Capacidades que o job exige do worker: 'ocr', 'pasta-local', 'senha-pdf'.
    // Um worker só reivindica job cujo `requires` cabe no que ele oferece.
    requires: text().array().default([]).notNull(),
    // Backoff: o job só volta a ser elegível a partir daqui.
    available_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_ingestion_jobs_needs_full_ai_review")
      .using(
        "btree",
        table.needs_full_ai_review.asc().nullsLast().op("bool_ops"),
        table.user_id.asc().nullsLast().op("bool_ops"),
      )
      .where(sql`(needs_full_ai_review = true)`),
    index("idx_ingestion_jobs_queue")
      .using(
        "btree",
        table.status.asc().nullsLast().op("timestamptz_ops"),
        table.created_at.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(status = ANY (ARRAY['queued'::text, 'discovered'::text]))`),
    index("idx_ingestion_jobs_run").using(
      "btree",
      table.run_id.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    index("idx_ingestion_jobs_user_status").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.run_id],
      foreignColumns: [ingestion_runs.id],
      name: "ingestion_jobs_run_id_fkey",
    }),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "ingestion_jobs_source_document_id_fkey",
    }),
    check(
      "ingestion_jobs_status_check",
      sql`status = ANY (ARRAY['discovered'::text, 'downloaded'::text, 'hashed'::text, 'queued'::text, 'parsing'::text, 'parsed'::text, 'classified'::text, 'reconciled'::text, 'drafted'::text, 'pending_review'::text, 'approved'::text, 'posted'::text, 'failed'::text])`,
    ),
  ],
);

export const financial_periods = pgTable(
  "financial_periods",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    start_date: date().notNull(),
    end_date: date().notNull(),
    label: text(),
    is_current: boolean().default(false).notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_financial_periods_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    check("chk_period_dates", sql`end_date >= start_date`),
  ],
);

export const financial_products = pgTable(
  "financial_products",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    institution_id: uuid().notNull(),
    name: text().notNull(),
    type: text().notNull(),
    current_balance: numeric({ precision: 15, scale: 2, mode: "number" }).default(0),
    credit_limit: numeric({ precision: 15, scale: 2, mode: "number" }),
    is_active: boolean().default(true).notNull(),
    display_order: integer(),
    metadata: jsonb().$type<Json>(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_financial_products_institution_id").using(
      "btree",
      table.institution_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_financial_products_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.institution_id],
      foreignColumns: [institutions.id],
      name: "financial_products_institution_id_fkey",
    }).onDelete("cascade"),
    check(
      "financial_products_type_check",
      sql`type = ANY (ARRAY['checking_account'::text, 'savings_account'::text, 'credit_card'::text, 'overdraft'::text, 'personal_loan'::text, 'mortgage'::text, 'investment'::text, 'other'::text])`,
    ),
  ],
);

export const import_jobs = pgTable(
  "import_jobs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    source_type: text().notNull(),
    file_path: text(),
    status: text().default("pending").notNull(),
    total_rows: integer(),
    imported_rows: integer().default(0).notNull(),
    skipped_rows: integer().default(0).notNull(),
    error_rows: integer().default(0).notNull(),
    error_details: jsonb().$type<Json>(),
    started_at: timestamp({ withTimezone: true, mode: "string" }),
    completed_at: timestamp({ withTimezone: true, mode: "string" }),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_import_jobs_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    check(
      "import_jobs_source_type_check",
      sql`source_type = ANY (ARRAY['csv'::text, 'xlsx'::text, 'manual'::text, 'api'::text])`,
    ),
    check(
      "import_jobs_status_check",
      sql`status = ANY (ARRAY['pending'::text, 'processing'::text, 'completed'::text, 'failed'::text, 'partial'::text])`,
    ),
  ],
);

export const ingestion_logs = pgTable(
  "ingestion_logs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    run_id: uuid(),
    job_id: uuid(),
    level: text().default("info").notNull(),
    message: text().notNull(),
    details: jsonb().$type<Json>().default({}),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ingestion_logs_job")
      .using(
        "btree",
        table.job_id.asc().nullsLast().op("timestamptz_ops"),
        table.created_at.asc().nullsLast().op("timestamptz_ops"),
      )
      .where(sql`(job_id IS NOT NULL)`),
    index("idx_ingestion_logs_run").using(
      "btree",
      table.run_id.asc().nullsLast().op("uuid_ops"),
      table.created_at.asc().nullsLast().op("timestamptz_ops"),
    ),
    index("idx_ingestion_logs_user_level")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("timestamptz_ops"),
        table.level.asc().nullsLast().op("timestamptz_ops"),
        table.created_at.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(level = ANY (ARRAY['warn'::text, 'error'::text]))`),
    foreignKey({
      columns: [table.job_id],
      foreignColumns: [ingestion_jobs.id],
      name: "ingestion_logs_job_id_fkey",
    }),
    foreignKey({
      columns: [table.run_id],
      foreignColumns: [ingestion_runs.id],
      name: "ingestion_logs_run_id_fkey",
    }),
    check(
      "ingestion_logs_level_check",
      sql`level = ANY (ARRAY['debug'::text, 'info'::text, 'warn'::text, 'error'::text])`,
    ),
  ],
);

export const ingestion_checkpoints = pgTable(
  "ingestion_checkpoints",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    source_type: text().notNull(),
    scope_key: text().notNull(),
    cursor_kind: text().notNull(),
    cursor_value: text(),
    last_message_id: text(),
    messages_seen: integer().default(0).notNull(),
    documents_created: integer().default(0).notNull(),
    duplicates_skipped: integer().default(0).notNull(),
    errors: integer().default(0).notNull(),
    window_start: date(),
    window_end: date(),
    status: text().default("running").notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("ix_ingestion_checkpoints_status").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    unique("ingestion_checkpoints_user_id_source_type_scope_key_key").on(
      table.scope_key,
      table.source_type,
      table.user_id,
    ),
    check(
      "ingestion_checkpoints_cursor_kind_check",
      sql`cursor_kind = ANY (ARRAY['page_token'::text, 'internal_date'::text, 'mtime'::text, 'page_number'::text])`,
    ),
    check(
      "ingestion_checkpoints_source_type_check",
      sql`source_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'pluggy'::text])`,
    ),
    check(
      "ingestion_checkpoints_status_check",
      sql`status = ANY (ARRAY['running'::text, 'paused'::text, 'completed'::text, 'failed'::text])`,
    ),
  ],
);

export const extraction_results = pgTable(
  "extraction_results",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    parsed_version_id: uuid().notNull(),
    user_id: uuid().notNull(),
    supplier_name_raw: text(),
    supplier_id: uuid(),
    supplier_confidence: numeric({ precision: 3, scale: 2, mode: "number" }),
    competence_date: date(),
    due_date: date(),
    total_amount: numeric({ precision: 12, scale: 2, mode: "number" }),
    currency: text().default("BRL"),
    breakdown: jsonb().$type<Json>(),
    document_number: text(),
    contract_identifier: text(),
    consumption_data: jsonb().$type<Json>(),
    category_suggestion: text(),
    tags_suggestion: text().array(),
    priority_suggestion: text(),
    financial_period_suggestion: jsonb().$type<Json>(),
    metadata: jsonb().$type<Json>().default({}),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    ai_enrichment_type: text(),
    ai_enrichment_at: timestamp({ withTimezone: true, mode: "string" }),
    confidence_per_field: jsonb().$type<Json>(),
    reasoning: text(),
    financial_intent: text(),
  },
  (table) => [
    index("idx_extraction_results_ai_enrichment_type")
      .using("btree", table.ai_enrichment_type.asc().nullsLast().op("text_ops"))
      .where(sql`(ai_enrichment_type IS NOT NULL)`),
    index("idx_extraction_results_supplier")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("uuid_ops"),
        table.supplier_id.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(supplier_id IS NOT NULL)`),
    index("idx_extraction_results_version").using(
      "btree",
      table.parsed_version_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.parsed_version_id],
      foreignColumns: [parsed_document_versions.id],
      name: "extraction_results_parsed_version_id_fkey",
    }),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "extraction_results_supplier_id_fkey",
    }),
    check(
      "extraction_results_ai_enrichment_type_check",
      sql`(ai_enrichment_type IS NULL) OR (ai_enrichment_type = ANY (ARRAY['lite'::text, 'full'::text]))`,
    ),
    check(
      "extraction_results_financial_intent_check",
      sql`(financial_intent IS NULL) OR (financial_intent = ANY (ARRAY['transaction'::text, 'recurring_expense'::text, 'metric'::text, 'liability_payment'::text, 'unknown'::text]))`,
    ),
  ],
);

export const financial_obligations = pgTable(
  "financial_obligations",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    obligation_type: text().notNull(),
    status: text().default("open").notNull(),
    supplier_id: uuid(),
    supplier_name_raw: text(),
    financial_product_id: uuid(),
    amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    due_date: date(),
    competence_date: date(),
    cycle_start_date: date(),
    cycle_end_date: date(),
    document_number: text(),
    barcode_digitable_line: text(),
    financial_identity_key: text(),
    confidence_score: numeric({ precision: 3, scale: 2, mode: "number" }),
    metadata: jsonb().$type<Json>().default({}),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    uniqueIndex("idx_financial_obligations_identity")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.financial_identity_key.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(financial_identity_key IS NOT NULL)`),
    index("idx_financial_obligations_user_status").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    foreignKey({
      columns: [table.financial_product_id],
      foreignColumns: [financial_products.id],
      name: "financial_obligations_financial_product_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "financial_obligations_supplier_id_fkey",
    }).onDelete("set null"),
    check(
      "financial_obligations_obligation_type_check",
      sql`obligation_type = ANY (ARRAY['bill_to_pay'::text, 'bill_reminder'::text, 'invoice_statement'::text, 'recurring_charge'::text, 'liability_installment'::text, 'unknown'::text])`,
    ),
    check(
      "financial_obligations_status_check",
      sql`status = ANY (ARRAY['open'::text, 'pending_review'::text, 'approved'::text, 'paid'::text, 'cancelled'::text, 'duplicate'::text, 'rejected'::text])`,
    ),
  ],
);

export const liabilities = pgTable(
  "liabilities",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    financial_product_id: uuid().notNull(),
    name: text().notNull(),
    type: text().notNull(),
    original_amount: numeric({ precision: 15, scale: 2, mode: "number" }).notNull(),
    outstanding_balance: numeric({ precision: 15, scale: 2, mode: "number" }).notNull(),
    interest_rate: numeric({ precision: 8, scale: 6, mode: "number" }),
    rate_type: text(),
    amortization_system: text(),
    total_installments: integer(),
    paid_installments: integer().default(0).notNull(),
    start_date: date(),
    end_date: date(),
    status: text().default("active").notNull(),
    metadata: jsonb().$type<Json>(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    supplier_id: uuid(),
  },
  (table) => [
    index("idx_liabilities_financial_product_id").using(
      "btree",
      table.financial_product_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_liabilities_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_liabilities_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.financial_product_id],
      foreignColumns: [financial_products.id],
      name: "liabilities_financial_product_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "liabilities_supplier_id_fkey",
    }).onDelete("set null"),
    check(
      "liabilities_amortization_system_check",
      sql`(amortization_system IS NULL) OR (amortization_system = ANY (ARRAY['sac'::text, 'price'::text, 'mixed'::text, 'other'::text, 'none'::text]))`,
    ),
    check(
      "liabilities_rate_type_check",
      sql`(rate_type IS NULL) OR (rate_type = ANY (ARRAY['monthly'::text, 'annual'::text]))`,
    ),
    check(
      "liabilities_status_check",
      sql`status = ANY (ARRAY['active'::text, 'paid_off'::text, 'renegotiated'::text, 'defaulted'::text])`,
    ),
    check(
      "liabilities_type_check",
      sql`type = ANY (ARRAY['personal_loan'::text, 'mortgage'::text, 'overdraft'::text, 'installment_plan'::text, 'other'::text])`,
    ),
  ],
);

export const ingestion_runs = pgTable(
  "ingestion_runs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    source_type: text().notNull(),
    status: text().default("running").notNull(),
    started_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    completed_at: timestamp({ withTimezone: true, mode: "string" }),
    metadata: jsonb().$type<Json>().default({}),
    stats: jsonb().$type<Json>().default({}),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ingestion_runs_user_source").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.source_type.asc().nullsLast().op("text_ops"),
    ),
    index("idx_ingestion_runs_user_status").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
      table.status.asc().nullsLast().op("uuid_ops"),
    ),
    check(
      "ingestion_runs_source_type_check",
      sql`source_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'manual_upload'::text])`,
    ),
    check(
      "ingestion_runs_status_check",
      sql`status = ANY (ARRAY['running'::text, 'completed'::text, 'failed'::text, 'cancelled'::text])`,
    ),
  ],
);

export const financial_obligation_evidences = pgTable(
  "financial_obligation_evidences",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    obligation_id: uuid().notNull(),
    source_document_id: uuid().notNull(),
    evidence_role: text().default("supporting").notNull(),
    confidence_score: numeric({ precision: 3, scale: 2, mode: "number" }),
    reasons: jsonb().$type<Json>().default([]),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_financial_obligation_evidences_document").using(
      "btree",
      table.source_document_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_financial_obligation_evidences_obligation").using(
      "btree",
      table.obligation_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.obligation_id],
      foreignColumns: [financial_obligations.id],
      name: "financial_obligation_evidences_obligation_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "financial_obligation_evidences_source_document_id_fkey",
    }).onDelete("cascade"),
    unique("financial_obligation_evidence_user_id_obligation_id_source__key").on(
      table.obligation_id,
      table.source_document_id,
      table.user_id,
    ),
    check(
      "financial_obligation_evidences_evidence_role_check",
      sql`evidence_role = ANY (ARRAY['primary'::text, 'supporting'::text, 'duplicate'::text, 'conflicting'::text])`,
    ),
  ],
);

export const supplier_aliases = pgTable(
  "supplier_aliases",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    supplier_id: uuid().notNull(),
    alias_name: text().notNull(),
    alias_type: text().default("other").notNull(),
    is_active: boolean().default(true).notNull(),
    valid_from: date(),
    valid_until: date(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_supplier_aliases_resolve")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("date_ops"),
        table.alias_name.asc().nullsLast().op("uuid_ops"),
        table.valid_from.asc().nullsLast().op("text_ops"),
        table.valid_until.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(is_active = true)`),
    index("idx_supplier_aliases_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_supplier_aliases_trgm").using(
      "gin",
      table.alias_name.asc().nullsLast().op("gin_trgm_ops"),
    ),
    index("idx_supplier_aliases_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "supplier_aliases_supplier_id_fkey",
    }).onDelete("cascade"),
    check(
      "supplier_aliases_alias_type_check",
      sql`alias_type = ANY (ARRAY['former_name'::text, 'abbreviation'::text, 'trade_name'::text, 'billing_name'::text, 'other'::text])`,
    ),
  ],
);

export const source_documents = pgTable(
  "source_documents",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    origin_type: text().notNull(),
    origin_key: text().notNull(),
    gmail_message_id: text(),
    gmail_thread_id: text(),
    gmail_attachment_id: text(),
    gmail_label: text(),
    gmail_date: timestamp({ withTimezone: true, mode: "string" }),
    gmail_from: text(),
    gmail_subject: text(),
    local_filepath: text(),
    local_mtime: timestamp({ withTimezone: true, mode: "string" }),
    filename: text().notNull(),
    mime_type: text(),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    file_size_bytes: bigint({ mode: "number" }),
    storage_path: text(),
    status: text().default("new").notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    content_hash: text(),
    metadata: jsonb().$type<Json>(),
    document_type: text(),
    supplier_id: uuid(),
    supplier_name_raw: text(),
  },
  (table) => [
    index("idx_source_documents_content_hash")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("uuid_ops"),
        table.content_hash.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(content_hash IS NOT NULL)`),
    index("idx_source_documents_document_type")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("uuid_ops"),
        table.document_type.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(document_type IS NOT NULL)`),
    index("idx_source_documents_gmail_msg")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.gmail_message_id.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(gmail_message_id IS NOT NULL)`),
    uniqueIndex("idx_source_documents_gmail_msg_file_dedup")
      .using(
        "btree",
        table.user_id.asc().nullsLast().op("text_ops"),
        table.gmail_message_id.asc().nullsLast().op("text_ops"),
        table.filename.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`((origin_type = 'gmail'::text) AND (gmail_message_id IS NOT NULL))`),
    index("idx_source_documents_status").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    index("idx_source_documents_supplier_id")
      .using("btree", table.supplier_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(supplier_id IS NOT NULL)`),
    index("idx_source_documents_user_origin").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.origin_type.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "source_documents_supplier_id_fkey",
    }).onDelete("set null"),
    unique("source_documents_user_id_origin_type_origin_key_key").on(
      table.origin_key,
      table.origin_type,
      table.user_id,
    ),
    check(
      "source_documents_origin_type_check",
      sql`origin_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'manual_upload'::text])`,
    ),
  ],
);

export const provider_connections = pgTable(
  "provider_connections",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    provider: text().notNull(),
    external_item_id: text().notNull(),
    institution_name: text(),
    status: text().default("active").notNull(),
    last_synced_at: timestamp({ withTimezone: true, mode: "string" }),
    metadata: jsonb().$type<Json>().default({}).notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_provider_connections_user").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    unique("provider_connections_user_id_provider_external_item_id_key").on(
      table.external_item_id,
      table.provider,
      table.user_id,
    ),
    check("provider_connections_provider_check", sql`provider = 'pluggy'::text`),
    check(
      "provider_connections_status_check",
      sql`status = ANY (ARRAY['active'::text, 'error'::text, 'revoked'::text, 'expired'::text])`,
    ),
  ],
);

export const parsed_document_versions = pgTable(
  "parsed_document_versions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    source_document_id: uuid().notNull(),
    user_id: uuid().notNull(),
    version_number: integer().default(1).notNull(),
    parser_type: text().notNull(),
    parser_version: text(),
    raw_text: text(),
    structured_data: jsonb().$type<Json>(),
    confidence_score: numeric({ precision: 3, scale: 2, mode: "number" }),
    metadata: jsonb().$type<Json>().default({}),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_parsed_versions_doc").using(
      "btree",
      table.source_document_id.asc().nullsLast().op("int4_ops"),
      table.version_number.desc().nullsFirst().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "parsed_document_versions_source_document_id_fkey",
    }),
    unique("parsed_document_versions_source_document_id_version_number_key").on(
      table.source_document_id,
      table.version_number,
    ),
    check(
      "parsed_document_versions_parser_type_check",
      sql`parser_type = ANY (ARRAY['local_text'::text, 'local_regex'::text, 'openai_vision'::text, 'openai_text'::text])`,
    ),
  ],
);

export const recurring_templates = pgTable(
  "recurring_templates",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    financial_product_id: uuid(),
    name: text().notNull(),
    type: text().notNull(),
    amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    is_variable_amount: boolean().default(false).notNull(),
    frequency: text().notNull(),
    day_of_month: integer(),
    custom_interval_days: integer(),
    category_id: uuid(),
    priority: text(),
    starts_at: date(),
    ends_at: date(),
    is_active: boolean().default(true).notNull(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    supplier_id: uuid(),
  },
  (table) => [
    index("idx_recurring_templates_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_recurring_templates_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.category_id],
      foreignColumns: [categories.id],
      name: "recurring_templates_category_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.financial_product_id],
      foreignColumns: [financial_products.id],
      name: "recurring_templates_financial_product_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "recurring_templates_supplier_id_fkey",
    }).onDelete("set null"),
    check(
      "recurring_templates_day_of_month_check",
      sql`(day_of_month IS NULL) OR ((day_of_month >= 1) AND (day_of_month <= 31))`,
    ),
    check(
      "recurring_templates_frequency_check",
      sql`frequency = ANY (ARRAY['monthly'::text, 'weekly'::text, 'biweekly'::text, 'quarterly'::text, 'annual'::text, 'custom'::text])`,
    ),
    check(
      "recurring_templates_priority_check",
      sql`(priority IS NULL) OR (priority = ANY (ARRAY['essential'::text, 'high'::text, 'medium'::text, 'low'::text, 'optional'::text]))`,
    ),
    check(
      "recurring_templates_type_check",
      sql`type = ANY (ARRAY['income'::text, 'expense'::text, 'liability_payment'::text, 'statement_payment'::text])`,
    ),
  ],
);

export const statement_cycles = pgTable(
  "statement_cycles",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    card_id: uuid().notNull(),
    reference_month: date().notNull(),
    cycle_start_date: date().notNull(),
    cycle_end_date: date().notNull(),
    due_date: date().notNull(),
    total_amount: numeric({ precision: 15, scale: 2, mode: "number" }).default(0),
    paid_amount: numeric({ precision: 15, scale: 2, mode: "number" }).default(0),
    status: text().default("open").notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_statement_cycles_card_id").using(
      "btree",
      table.card_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_statement_cycles_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.card_id],
      foreignColumns: [cards.id],
      name: "statement_cycles_card_id_fkey",
    }).onDelete("cascade"),
    check(
      "statement_cycles_status_check",
      sql`status = ANY (ARRAY['open'::text, 'closed'::text, 'paid'::text, 'partial'::text, 'overdue'::text])`,
    ),
  ],
);

export const statement_items = pgTable(
  "statement_items",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    statement_cycle_id: uuid().notNull(),
    transaction_id: uuid(),
    description: text(),
    amount: numeric({ precision: 15, scale: 2, mode: "number" }).notNull(),
    transaction_date: date(),
    installment_number: integer(),
    total_installments: integer(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    supplier_id: uuid(),
  },
  (table) => [
    index("idx_statement_items_statement_cycle_id").using(
      "btree",
      table.statement_cycle_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_statement_items_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    uniqueIndex("idx_statement_items_transaction_id")
      .using("btree", table.transaction_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(transaction_id IS NOT NULL)`),
    index("idx_statement_items_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.statement_cycle_id],
      foreignColumns: [statement_cycles.id],
      name: "statement_items_statement_cycle_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "statement_items_supplier_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.transaction_id],
      foreignColumns: [transactions.id],
      name: "statement_items_transaction_id_fkey",
    }).onDelete("set null"),
  ],
);

export const supplier_contracts = pgTable(
  "supplier_contracts",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    supplier_id: uuid().notNull(),
    contract_type: text().notNull(),
    identifier: text(),
    label: text(),
    is_active: boolean().default(true).notNull(),
    start_date: date(),
    end_date: date(),
    metadata: jsonb().$type<Json>(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_supplier_contracts_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_supplier_contracts_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "supplier_contracts_supplier_id_fkey",
    }).onDelete("cascade"),
    check(
      "supplier_contracts_contract_type_check",
      sql`contract_type = ANY (ARRAY['service'::text, 'subscription'::text, 'utility'::text, 'loan'::text, 'insurance'::text, 'maintenance'::text, 'other'::text])`,
    ),
  ],
);

export const liability_installments = pgTable(
  "liability_installments",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    liability_id: uuid().notNull(),
    installment_number: integer().notNull(),
    due_date: date().notNull(),
    total_amount: numeric({ precision: 15, scale: 2, mode: "number" }).notNull(),
    principal_amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    interest_amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    insurance_amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    fee_amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    paid_amount: numeric({ precision: 15, scale: 2, mode: "number" }).default(0).notNull(),
    paid_date: date(),
    status: text().default("pending").notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_liability_installments_liability_id").using(
      "btree",
      table.liability_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_liability_installments_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.liability_id],
      foreignColumns: [liabilities.id],
      name: "liability_installments_liability_id_fkey",
    }).onDelete("cascade"),
    check(
      "liability_installments_status_check",
      sql`status = ANY (ARRAY['pending'::text, 'paid'::text, 'partial'::text, 'overdue'::text, 'waived'::text])`,
    ),
  ],
);

export const suppliers = pgTable(
  "suppliers",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    trade_name: text(),
    legal_name: text(),
    document_number: text(),
    type: text().default("company").notNull(),
    website: text(),
    contact_info: jsonb().$type<Json>(),
    notes: text(),
    institution_id: uuid(),
    is_active: boolean().default(true).notNull(),
    display_order: integer(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_suppliers_name_trgm").using("gin", table.name.asc().nullsLast().op("gin_trgm_ops")),
    index("idx_suppliers_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.institution_id],
      foreignColumns: [institutions.id],
      name: "suppliers_institution_id_fkey",
    }).onDelete("set null"),
    check(
      "suppliers_type_check",
      sql`type = ANY (ARRAY['company'::text, 'individual'::text, 'government'::text, 'utility'::text, 'telecom'::text, 'saas'::text, 'platform'::text, 'other'::text])`,
    ),
  ],
);

export const transactions = pgTable(
  "transactions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    financial_product_id: uuid().notNull(),
    type: text().notNull(),
    amount: numeric({ precision: 15, scale: 2, mode: "number" }).notNull(),
    description: text(),
    event_date: date().notNull(),
    competence_date: date(),
    financial_period_id: uuid(),
    statement_cycle_id: uuid(),
    liability_installment_id: uuid(),
    category_id: uuid(),
    priority: text(),
    is_confirmed: boolean().default(false).notNull(),
    notes: text(),
    metadata: jsonb().$type<Json>(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    origin_type: text().default("manual").notNull(),
    // Referência cruzada para recurring_instances (declarada mais abaixo neste
    // arquivo): usa .references() com callback tipado (AnyPgColumn) em vez do
    // foreignKey() builder na lista de constraints para quebrar o ciclo de
    // inferência de tipos entre transactions <-> recurring_instances
    // (TS7022/TS7024 — ver edição manual nº 1 no cabeçalho deste arquivo). O
    // nome do constraint gerado pelo Drizzle passa a ser o default automático
    // em vez de "transactions_recurring_instance_id_fkey" (mesma semântica de
    // onDelete "set null"); isso não afeta queries, só teria efeito se este
    // schema fosse usado para `drizzle-kit generate`/push, o que não é o caso
    // neste projeto (schema.sql/Neon são a fonte da verdade).
    recurring_instance_id: uuid().references((): AnyPgColumn => recurring_instances.id, {
      onDelete: "set null",
    }),
    supplier_id: uuid(),
    source_document_id: uuid(),
  },
  (table) => [
    index("idx_transactions_category_id").using(
      "btree",
      table.category_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_event_date").using(
      "btree",
      table.user_id.asc().nullsLast().op("date_ops"),
      table.event_date.asc().nullsLast().op("date_ops"),
    ),
    index("idx_transactions_financial_period_id").using(
      "btree",
      table.financial_period_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_financial_product_id").using(
      "btree",
      table.financial_product_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_recurring_instance_id").using(
      "btree",
      table.recurring_instance_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_source_document_id")
      .using("btree", table.source_document_id.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("idx_transactions_statement_cycle_id").using(
      "btree",
      table.statement_cycle_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_type").using(
      "btree",
      table.user_id.asc().nullsLast().op("text_ops"),
      table.type.asc().nullsLast().op("text_ops"),
    ),
    index("idx_transactions_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.category_id],
      foreignColumns: [categories.id],
      name: "transactions_category_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.financial_period_id],
      foreignColumns: [financial_periods.id],
      name: "transactions_financial_period_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.financial_product_id],
      foreignColumns: [financial_products.id],
      name: "transactions_financial_product_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.liability_installment_id],
      foreignColumns: [liability_installments.id],
      name: "transactions_liability_installment_id_fkey",
    }).onDelete("set null"),
    // FK para recurring_instances agora definida inline na coluna acima
    // (recurring_instance_id.references(...)) para quebrar o ciclo de tipos —
    // ver comentário na definição da coluna.
    foreignKey({
      columns: [table.source_document_id],
      foreignColumns: [source_documents.id],
      name: "transactions_source_document_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.statement_cycle_id],
      foreignColumns: [statement_cycles.id],
      name: "transactions_statement_cycle_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "transactions_supplier_id_fkey",
    }).onDelete("set null"),
    check(
      "transactions_origin_type_check",
      sql`origin_type = ANY (ARRAY['manual'::text, 'import'::text, 'recurring'::text, 'statement_link'::text])`,
    ),
    check(
      "transactions_priority_check",
      sql`(priority IS NULL) OR (priority = ANY (ARRAY['essential'::text, 'high'::text, 'medium'::text, 'low'::text, 'optional'::text]))`,
    ),
    check(
      "transactions_type_check",
      sql`type = ANY (ARRAY['income'::text, 'expense'::text, 'refund'::text, 'adjustment'::text, 'interest_charge'::text, 'fee'::text, 'statement_payment'::text, 'liability_payment'::text])`,
    ),
  ],
);

export const recurring_instances = pgTable(
  "recurring_instances",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    recurring_template_id: uuid().notNull(),
    expected_date: date().notNull(),
    expected_amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    actual_amount: numeric({ precision: 15, scale: 2, mode: "number" }),
    status: text().default("pending").notNull(),
    paid_date: date(),
    transaction_id: uuid(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_recurring_instances_template_id").using(
      "btree",
      table.recurring_template_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_recurring_instances_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.recurring_template_id],
      foreignColumns: [recurring_templates.id],
      name: "recurring_instances_recurring_template_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transaction_id],
      foreignColumns: [transactions.id],
      name: "recurring_instances_transaction_id_fkey",
    }).onDelete("set null"),
    check(
      "recurring_instances_status_check",
      sql`status = ANY (ARRAY['pending'::text, 'paid'::text, 'partial'::text, 'skipped'::text, 'overdue'::text, 'cancelled'::text])`,
    ),
  ],
);

export const supplier_tags = pgTable(
  "supplier_tags",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    supplier_id: uuid().notNull(),
    tag_id: uuid().notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_supplier_tags_supplier_id").using(
      "btree",
      table.supplier_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplier_id],
      foreignColumns: [suppliers.id],
      name: "supplier_tags_supplier_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.tag_id],
      foreignColumns: [tags.id],
      name: "supplier_tags_tag_id_fkey",
    }).onDelete("cascade"),
    unique("supplier_tags_supplier_id_tag_id_key").on(table.supplier_id, table.tag_id),
  ],
);

export const tags = pgTable(
  "tags",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    color: text(),
    influences_priority: boolean().default(false).notNull(),
    suggested_priority: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_tags_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    check(
      "tags_suggested_priority_check",
      sql`(suggested_priority IS NULL) OR (suggested_priority = ANY (ARRAY['essential'::text, 'high'::text, 'medium'::text, 'low'::text, 'optional'::text]))`,
    ),
  ],
);

export const transfers = pgTable(
  "transfers",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    source_product_id: uuid().notNull(),
    target_product_id: uuid().notNull(),
    amount: numeric({ precision: 15, scale: 2, mode: "number" }).notNull(),
    description: text(),
    event_date: date().notNull(),
    competence_date: date(),
    financial_period_id: uuid(),
    is_confirmed: boolean().default(false).notNull(),
    notes: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_transfers_source_product_id").using(
      "btree",
      table.source_product_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transfers_target_product_id").using(
      "btree",
      table.target_product_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transfers_user_id").using("btree", table.user_id.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.financial_period_id],
      foreignColumns: [financial_periods.id],
      name: "transfers_financial_period_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.source_product_id],
      foreignColumns: [financial_products.id],
      name: "transfers_source_product_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.target_product_id],
      foreignColumns: [financial_products.id],
      name: "transfers_target_product_id_fkey",
    }).onDelete("cascade"),
  ],
);

export const user_financial_preferences = pgTable(
  "user_financial_preferences",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    financial_cycle_start_day: integer(),
    financial_cycle_anchor_date: date(),
    default_currency: text().default("BRL").notNull(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("idx_user_financial_preferences_user_id").using(
      "btree",
      table.user_id.asc().nullsLast().op("uuid_ops"),
    ),
    unique("user_financial_preferences_user_id_key").on(table.user_id),
    check(
      "user_financial_preferences_financial_cycle_start_day_check",
      sql`(financial_cycle_start_day >= 1) AND (financial_cycle_start_day <= 31)`,
    ),
  ],
);

export const liability_tags = pgTable(
  "liability_tags",
  {
    liability_id: uuid().notNull(),
    tag_id: uuid().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.liability_id],
      foreignColumns: [liabilities.id],
      name: "liability_tags_liability_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.tag_id],
      foreignColumns: [tags.id],
      name: "liability_tags_tag_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.liability_id, table.tag_id], name: "liability_tags_pkey" }),
  ],
);

export const recurring_template_tags = pgTable(
  "recurring_template_tags",
  {
    recurring_template_id: uuid().notNull(),
    tag_id: uuid().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.recurring_template_id],
      foreignColumns: [recurring_templates.id],
      name: "recurring_template_tags_recurring_template_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.tag_id],
      foreignColumns: [tags.id],
      name: "recurring_template_tags_tag_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.recurring_template_id, table.tag_id],
      name: "recurring_template_tags_pkey",
    }),
  ],
);

export const transaction_tags = pgTable(
  "transaction_tags",
  {
    transaction_id: uuid().notNull(),
    tag_id: uuid().notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.tag_id],
      foreignColumns: [tags.id],
      name: "transaction_tags_tag_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transaction_id],
      foreignColumns: [transactions.id],
      name: "transaction_tags_transaction_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.tag_id, table.transaction_id], name: "transaction_tags_pkey" }),
  ],
);
export const v_expenses_deduplicated = pgView("v_expenses_deduplicated", {
  canonical_id: uuid(),
  source_type: text(),
  user_id: uuid(),
  amount: numeric({ precision: 15, scale: 2, mode: "number" }),
  description: text(),
  supplier_id: uuid(),
  category_id: uuid(),
  priority: text(),
  event_date: date(),
  competence_date: date(),
  financial_period_id: uuid(),
  statement_cycle_id: uuid(),
}).as(
  sql`SELECT si.id AS canonical_id, 'statement_item'::text AS source_type, si.user_id, si.amount, si.description, si.supplier_id, NULL::uuid AS category_id, NULL::text AS priority, si.transaction_date AS event_date, NULL::date AS competence_date, NULL::uuid AS financial_period_id, si.statement_cycle_id FROM statement_items si WHERE si.transaction_id IS NULL UNION ALL SELECT t.id AS canonical_id, 'transaction'::text AS source_type, t.user_id, t.amount, t.description, t.supplier_id, t.category_id, t.priority, t.event_date, t.competence_date, t.financial_period_id, t.statement_cycle_id FROM transactions t WHERE (t.type = ANY (ARRAY['expense'::text, 'fee'::text, 'interest_charge'::text])) AND (t.type <> ALL (ARRAY['statement_payment'::text, 'refund'::text]))`,
);

export const mv_supplier_spending = pgMaterializedView("mv_supplier_spending", {
  user_id: uuid(),
  supplier_id: uuid(),
  supplier_name: text(),
  supplier_type: text(),
  // You can use { mode: "bigint" } if numbers are exceeding js number limitations
  transaction_count: bigint({ mode: "number" }),
  total_spent: numeric({ mode: "number" }),
  first_transaction_date: date(),
  last_transaction_date: date(),
  // You can use { mode: "bigint" } if numbers are exceeding js number limitations
  periods_active: bigint({ mode: "number" }),
}).as(
  sql`SELECT s.user_id, s.id AS supplier_id, s.name AS supplier_name, s.type AS supplier_type, count(DISTINCT t.id) AS transaction_count, COALESCE(sum(t.amount), 0::numeric) AS total_spent, min(t.event_date) AS first_transaction_date, max(t.event_date) AS last_transaction_date, count(DISTINCT t.financial_period_id) AS periods_active FROM suppliers s LEFT JOIN transactions t ON t.supplier_id = s.id AND t.user_id = s.user_id AND (t.type = ANY (ARRAY['expense'::text, 'fee'::text, 'interest_charge'::text])) WHERE s.is_active = true GROUP BY s.user_id, s.id, s.name, s.type`,
);

// ═══════════════════════════════════════════════════════════════════════════
// Tabelas criadas fora da introspecção original (migrations 0001 e 0002).
// Escritas à mão, no mesmo estilo do resto do arquivo. Se você reintrospectar,
// elas voltam sozinhas — mas os comentários abaixo, não.
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Identidade própria, no lugar de `auth.users` do Supabase.
 *
 * `role` e `owner_id` separam duas coisas que costumam ser confundidas: **quem
 * você é** e **de quem são os dados que você opera**. O dono entra pelo Google
 * e opera sobre si mesmo (`owner_id` nulo). O administrador de emergência entra
 * por e-mail/senha e opera sobre os dados do dono (`owner_id` aponta pra ele) —
 * é chave reserva para o dia em que a conta Google falhar. Toda query de dados
 * filtra pelo **dono**, nunca pelo ator; o `audit_logs` grava o ator.
 */
export const users = pgTable(
  "users",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    email: text().notNull(),
    name: text(),
    google_sub: text(),
    password_hash: text(),
    role: text().default("owner").notNull(),
    owner_id: uuid(),
    failed_login_count: integer().default(0).notNull(),
    locked_until: timestamp({ withTimezone: true, mode: "string" }),
    last_login_at: timestamp({ withTimezone: true, mode: "string" }),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    unique("users_email_key").on(table.email),
    unique("users_google_sub_key").on(table.google_sub),
    foreignKey({
      columns: [table.owner_id],
      foreignColumns: [table.id],
      name: "users_owner_id_fkey",
    }).onDelete("restrict"),
    check("users_role_check", sql`role = ANY (ARRAY['owner'::text, 'admin'::text])`),
    check("users_admin_tem_dono", sql`(role <> 'admin'::text) OR (owner_id IS NOT NULL)`),
  ],
);

/**
 * Centro: *para quem* ou *para qual projeto* o dinheiro foi (Edulingo, Imob Max,
 * Daiane). Diferente de Categoria (a natureza do gasto) e de Tag (rótulo livre,
 * sem valor). É a única das três que aceita rateio e tem saldo — comporta-se
 * como uma conta corrente virtual. Ver CONTEXT.md.
 */
export const financial_centers = pgTable(
  "financial_centers",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    name: text().notNull(),
    type: text().default("projeto").notNull(),
    description: text(),
    status: text().default("active").notNull(),
    icon: text(),
    color: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
    updated_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    unique("financial_centers_nome_unico").on(table.user_id, table.name),
    check(
      "financial_centers_type_check",
      sql`type = ANY (ARRAY['projeto'::text, 'pessoa'::text, 'negocio'::text])`,
    ),
    check(
      "financial_centers_status_check",
      sql`status = ANY (ARRAY['active'::text, 'archived'::text])`,
    ),
  ],
);

/**
 * Rateio: a divisão do valor de uma movimentação entre Centros.
 *
 * Guarda **valor**, nunca percentual — o percentual é derivado na exibição.
 * Guardar os dois faz um divergir do outro na primeira vez que o valor da
 * movimentação mudar. A soma dos rateios **pode ser menor** que a movimentação
 * (o resto é "não alocado", um estado legítimo e visível) e **nunca maior** —
 * isso é garantido pelo trigger `transaction_allocations_valida`, que também
 * recusa rateio em pagamento de fatura, já que as compras dentro dela já foram
 * rateadas.
 */
export const transaction_allocations = pgTable(
  "transaction_allocations",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    user_id: uuid().notNull(),
    transaction_id: uuid().notNull(),
    financial_center_id: uuid().notNull(),
    amount: numeric({ precision: 14, scale: 2, mode: "number" }).notNull(),
    note: text(),
    created_at: timestamp({ withTimezone: true, mode: "string" }).defaultNow().notNull(),
  },
  (table) => [
    index("ix_transaction_allocations_centro").using(
      "btree",
      table.financial_center_id.asc().nullsLast().op("uuid_ops"),
    ),
    index("ix_transaction_allocations_transacao").using(
      "btree",
      table.transaction_id.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.transaction_id],
      foreignColumns: [transactions.id],
      name: "transaction_allocations_transaction_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.financial_center_id],
      foreignColumns: [financial_centers.id],
      name: "transaction_allocations_financial_center_id_fkey",
    }).onDelete("restrict"),
    unique("transaction_allocations_um_por_centro").on(
      table.transaction_id,
      table.financial_center_id,
    ),
    check("transaction_allocations_amount_positivo", sql`amount > (0)::numeric`),
  ],
);
