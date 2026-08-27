/**
 * Schema Drizzle do Neon — gerado por `drizzle-kit introspect`
 * (dentro de packages/db: `bunx drizzle-kit introspect`, ou o script
 * `db:introspect` do package.json) a partir do banco real, MAS COM EDIÇÕES
 * MANUAIS. Não é 100% auto-gerado, apesar de estar na lista de ignorados do
 * eslint.config.ts junto com os arquivos que são.
 *
 * ⚠️ Rodar a introspecção de novo SOBRESCREVE este arquivo e apaga as edições
 * abaixo em silêncio. Se você reintrospectar, reaplique-as:
 *
 *   1. `transactions.recurringInstanceId` — a FK para `recurring_instances`
 *      é declarada inline na coluna, via
 *      `.references((): AnyPgColumn => recurringInstances.id, { onDelete: "set null" })`,
 *      em vez do `foreignKey({...})` builder na lista de constraints da
 *      tabela (que é o que a introspecção gera). Motivo: `transactions` e
 *      `recurring_instances` se referenciam mutuamente e o builder produz um
 *      ciclo de inferência de tipos — `tsc` falha com TS7022/TS7024. O
 *      callback anotado com `AnyPgColumn` quebra o ciclo (mesmo padrão que o
 *      próprio drizzle-kit usa para auto-referências, ex. `categories.parentId`).
 *      Efeito colateral aceito: o nome do constraint passa a ser o default do
 *      Drizzle em vez de `transactions_recurring_instance_id_fkey` — irrelevante
 *      aqui, porque este schema NUNCA é usado para `drizzle-kit generate`/`push`
 *      (a fonte da verdade do DDL é packages/db/schema.sql, aplicado no Neon).
 *
 *   2. `userSecrets` — o parâmetro do callback de constraints é `_table`, não
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

export const userSecrets = pgTable(
  "user_secrets",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    secretType: text("secret_type").notNull(),
    entityType: text("entity_type"),
    entityId: uuid("entity_id"),
    encryptedValue: text("encrypted_value").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    encryptionVersion: integer("encryption_version").default(1).notNull(),
    contractIdentifier: text("contract_identifier"),
    label: text(),
    lastUsedAt: timestamp("last_used_at", { withTimezone: true, mode: "string" }),
    successCount: integer("success_count").default(0).notNull(),
  },
  // `table` não é usado aqui: o índice único abaixo precisou cair para sql`` cru
  // (COALESCE não é representável via table.coluna encadeado), então o parâmetro
  // fica sem uso — prefixado com "_" para o `noUnusedParameters` do tsc.
  // (Edição manual nº 2 — ver cabeçalho deste arquivo.)
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

export const documentPatterns = pgTable(
  "document_patterns",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    name: text().notNull(),
    supplierId: uuid("supplier_id"),
    documentType: text("document_type").notNull(),
    institutionId: uuid("institution_id"),
    extractionRules: jsonb("extraction_rules").default({}).notNull(),
    fieldMappings: jsonb("field_mappings").default({}).notNull(),
    sampleFingerprints: text("sample_fingerprints").array().default([""]).notNull(),
    confidenceThreshold: numeric("confidence_threshold", { precision: 3, scale: 2 })
      .default("0.80")
      .notNull(),
    version: integer().default(1).notNull(),
    isActive: boolean("is_active").default(true).notNull(),
    feedbackCount: integer("feedback_count").default(0).notNull(),
    successCount: integer("success_count").default(0).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_document_patterns_institution")
      .using("btree", table.institutionId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(is_active = true)`),
    index("idx_document_patterns_supplier_type")
      .using(
        "btree",
        table.supplierId.asc().nullsLast().op("text_ops"),
        table.documentType.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(is_active = true)`),
    index("idx_document_patterns_user_active").using(
      "btree",
      table.userId.asc().nullsLast().op("bool_ops"),
      table.isActive.asc().nullsLast().op("bool_ops"),
    ),
    foreignKey({
      columns: [table.institutionId],
      foreignColumns: [institutions.id],
      name: "document_patterns_institution_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "document_patterns_supplier_id_fkey",
    }).onDelete("set null"),
    unique("document_patterns_user_id_name_version_key").on(
      table.name,
      table.userId,
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

export const patternFeedback = pgTable(
  "pattern_feedback",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    patternId: uuid("pattern_id").notNull(),
    sourceDocumentId: uuid("source_document_id"),
    feedbackType: text("feedback_type").notNull(),
    corrections: jsonb().default({}).notNull(),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_pattern_feedback_document")
      .using("btree", table.sourceDocumentId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("idx_pattern_feedback_pattern").using(
      "btree",
      table.patternId.asc().nullsLast().op("timestamptz_ops"),
      table.createdAt.desc().nullsFirst().op("timestamptz_ops"),
    ),
    index("idx_pattern_feedback_user").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
      table.createdAt.desc().nullsFirst().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.patternId],
      foreignColumns: [documentPatterns.id],
      name: "pattern_feedback_pattern_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "pattern_feedback_source_document_id_fkey",
    }).onDelete("set null"),
    check(
      "pattern_feedback_feedback_type_check",
      sql`feedback_type = ANY (ARRAY['correct'::text, 'incorrect'::text, 'partial'::text, 'improved'::text])`,
    ),
  ],
);

export const auditLogs = pgTable(
  "audit_logs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    action: text().notNull(),
    entityType: text("entity_type"),
    entityId: uuid("entity_id"),
    oldValues: jsonb("old_values"),
    newValues: jsonb("new_values"),
    ipAddress: inet("ip_address"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_audit_logs_entity").using(
      "btree",
      table.entityType.asc().nullsLast().op("text_ops"),
      table.entityId.asc().nullsLast().op("text_ops"),
    ),
    index("idx_audit_logs_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
  ],
);

export const cards = pgTable(
  "cards",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    financialProductId: uuid("financial_product_id").notNull(),
    lastFourDigits: text("last_four_digits"),
    cardBrand: text("card_brand"),
    isPrimary: boolean("is_primary").default(true).notNull(),
    holderName: text("holder_name"),
    creditLimit: numeric("credit_limit", { precision: 15, scale: 2 }),
    closingDay: integer("closing_day"),
    dueDay: integer("due_day"),
    isActive: boolean("is_active").default(true).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_cards_financial_product_id").using(
      "btree",
      table.financialProductId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_cards_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.financialProductId],
      foreignColumns: [financialProducts.id],
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
    userId: uuid("user_id").notNull(),
    name: text().notNull(),
    parentId: uuid("parent_id"),
    icon: text(),
    color: text(),
    displayOrder: integer("display_order"),
    isActive: boolean("is_active").default(true).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_categories_parent_id").using(
      "btree",
      table.parentId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_categories_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.parentId],
      foreignColumns: [table.id],
      name: "categories_parent_id_fkey",
    }).onDelete("set null"),
  ],
);

export const consumptionMetrics = pgTable(
  "consumption_metrics",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    supplierId: uuid("supplier_id").notNull(),
    supplierContractId: uuid("supplier_contract_id"),
    transactionId: uuid("transaction_id"),
    documentId: uuid("document_id"),
    referencePeriodStart: date("reference_period_start").notNull(),
    referencePeriodEnd: date("reference_period_end").notNull(),
    metricName: text("metric_name"),
    metricUnit: text("metric_unit"),
    quantity: numeric({ precision: 15, scale: 4 }),
    unitPrice: numeric("unit_price", { precision: 15, scale: 6 }),
    subtotal: numeric({ precision: 15, scale: 2 }),
    metadata: jsonb(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_consumption_metrics_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_consumption_metrics_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.documentId],
      foreignColumns: [documentsLegacy.id],
      name: "consumption_metrics_document_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplierContractId],
      foreignColumns: [supplierContracts.id],
      name: "consumption_metrics_supplier_contract_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "consumption_metrics_supplier_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transactionId],
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

export const documentSplits = pgTable(
  "document_splits",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    sourceDocumentId: uuid("source_document_id").notNull(),
    userId: uuid("user_id").notNull(),
    categoryId: uuid("category_id"),
    tags: uuid().array().default([""]).notNull(),
    amount: numeric({ precision: 14, scale: 2 }).notNull(),
    description: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_document_splits_category").using(
      "btree",
      table.categoryId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_splits_source_document").using(
      "btree",
      table.sourceDocumentId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_splits_user").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.categoryId],
      foreignColumns: [categories.id],
      name: "document_splits_category_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "document_splits_source_document_id_fkey",
    }).onDelete("cascade"),
    check("document_splits_amount_positive", sql`amount > (0)::numeric`),
  ],
);

export const documentsLegacy = pgTable(
  "documents_legacy",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    name: text().notNull(),
    description: text(),
    filePath: text("file_path").notNull(),
    fileType: text("file_type"),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    fileSize: bigint("file_size", { mode: "number" }),
    documentType: text("document_type"),
    entityType: text("entity_type"),
    entityId: uuid("entity_id"),
    version: integer().default(1).notNull(),
    isPasswordProtected: boolean("is_password_protected").default(false).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    supplierId: uuid("supplier_id"),
  },
  (table) => [
    index("idx_documents_entity").using(
      "btree",
      table.entityType.asc().nullsLast().op("text_ops"),
      table.entityId.asc().nullsLast().op("text_ops"),
    ),
    index("idx_documents_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_documents_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "documents_supplier_id_fkey",
    }).onDelete("set null"),
    check(
      "documents_document_type_check",
      sql`(document_type IS NULL) OR (document_type = ANY (ARRAY['receipt'::text, 'invoice'::text, 'statement'::text, 'contract'::text, 'proof'::text, 'other'::text]))`,
    ),
  ],
);

export const draftRecords = pgTable(
  "draft_records",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    batchId: uuid("batch_id"),
    userId: uuid("user_id").notNull(),
    sourceDocumentId: uuid("source_document_id"),
    extractionResultId: uuid("extraction_result_id"),
    draftType: text("draft_type").notNull(),
    status: text().default("pending_review").notNull(),
    draftData: jsonb("draft_data").notNull(),
    corrections: jsonb(),
    confidenceScore: numeric("confidence_score", { precision: 3, scale: 2 }),
    approvedAt: timestamp("approved_at", { withTimezone: true, mode: "string" }),
    approvedBy: uuid("approved_by"),
    postedRecordId: uuid("posted_record_id"),
    postedRecordType: text("posted_record_type"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
    reconciliationStatus: text("reconciliation_status").default("not_checked").notNull(),
    reconciledTransactionId: uuid("reconciled_transaction_id"),
    reconciledTemplateId: uuid("reconciled_template_id"),
    reconciliationCandidates: jsonb("reconciliation_candidates").default([]),
    reconciledAt: timestamp("reconciled_at", { withTimezone: true, mode: "string" }),
    draftSchemaVersion: smallint("draft_schema_version").default(0).notNull(),
    draftDataLegacy: jsonb("draft_data_legacy"),
    materializationKey: text("materialization_key").notNull(),
    postedAt: timestamp("posted_at", { withTimezone: true, mode: "string" }),
    materializationError: jsonb("materialization_error"),
    obligationId: uuid("obligation_id"),
    externalRef: text("external_ref"),
  },
  (table) => [
    index("idx_draft_records_batch")
      .using("btree", table.batchId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(batch_id IS NOT NULL)`),
    uniqueIndex("idx_draft_records_external_ref")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.externalRef.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(external_ref IS NOT NULL)`),
    index("idx_draft_records_reconciled_transaction")
      .using("btree", table.reconciledTransactionId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(reconciled_transaction_id IS NOT NULL)`),
    index("idx_draft_records_reconciliation_status")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.reconciliationStatus.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(reconciliation_status <> ALL (ARRAY['not_checked'::text, 'no_match'::text]))`),
    index("idx_draft_records_review_queue")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.status.asc().nullsLast().op("text_ops"),
        table.createdAt.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(status = 'pending_review'::text)`),
    index("idx_draft_records_source_doc")
      .using("btree", table.sourceDocumentId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("ix_draft_records_obligation")
      .using("btree", table.obligationId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(obligation_id IS NOT NULL)`),
    index("ix_draft_records_schema_version")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("int2_ops"),
        table.draftSchemaVersion.asc().nullsLast().op("int2_ops"),
      )
      .where(sql`(draft_schema_version = 0)`),
    uniqueIndex("uq_draft_records_materialization_key").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.materializationKey.asc().nullsLast().op("text_ops"),
    ),
    uniqueIndex("uq_draft_records_posted_record")
      .using(
        "btree",
        table.postedRecordType.asc().nullsLast().op("text_ops"),
        table.postedRecordId.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(posted_record_id IS NOT NULL)`),
    foreignKey({
      columns: [table.batchId],
      foreignColumns: [draftBatches.id],
      name: "draft_records_batch_id_fkey",
    }),
    foreignKey({
      columns: [table.extractionResultId],
      foreignColumns: [extractionResults.id],
      name: "draft_records_extraction_result_id_fkey",
    }),
    foreignKey({
      columns: [table.obligationId],
      foreignColumns: [financialObligations.id],
      name: "draft_records_obligation_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.reconciledTemplateId],
      foreignColumns: [recurringTemplates.id],
      name: "draft_records_reconciled_template_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.reconciledTransactionId],
      foreignColumns: [transactions.id],
      name: "draft_records_reconciled_transaction_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
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

export const aiChatSessions = pgTable(
  "ai_chat_sessions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    title: text(),
    contextType: text("context_type"),
    contextId: uuid("context_id"),
    model: text().default("gpt-4o").notNull(),
    totalTokensUsed: integer("total_tokens_used").default(0),
    messageCount: integer("message_count").default(0),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ai_chat_sessions_updated").using(
      "btree",
      table.updatedAt.desc().nullsFirst().op("timestamptz_ops"),
    ),
    index("idx_ai_chat_sessions_user").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
  ],
);

export const aiChatMessages = pgTable(
  "ai_chat_messages",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    sessionId: uuid("session_id").notNull(),
    userId: uuid("user_id").notNull(),
    role: text().notNull(),
    content: text(),
    toolCalls: jsonb("tool_calls"),
    toolCallId: text("tool_call_id"),
    toolName: text("tool_name"),
    tokensUsed: integer("tokens_used"),
    latencyMs: integer("latency_ms"),
    model: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ai_chat_messages_session").using(
      "btree",
      table.sessionId.asc().nullsLast().op("timestamptz_ops"),
      table.createdAt.asc().nullsLast().op("timestamptz_ops"),
    ),
    index("idx_ai_chat_messages_user").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.sessionId],
      foreignColumns: [aiChatSessions.id],
      name: "ai_chat_messages_session_id_fkey",
    }).onDelete("cascade"),
    check(
      "ai_chat_messages_role_check",
      sql`role = ANY (ARRAY['user'::text, 'assistant'::text, 'system'::text, 'tool'::text])`,
    ),
  ],
);

export const documentFingerprints = pgTable(
  "document_fingerprints",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    sourceDocumentId: uuid("source_document_id").notNull(),
    userId: uuid("user_id").notNull(),
    contentHash: text("content_hash").notNull(),
    canonicalFingerprint: text("canonical_fingerprint"),
    hashAlgorithm: text("hash_algorithm").default("sha256"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_document_fingerprints_canonical")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.canonicalFingerprint.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(canonical_fingerprint IS NOT NULL)`),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "document_fingerprints_source_document_id_fkey",
    }),
    unique("document_fingerprints_user_id_content_hash_key").on(table.contentHash, table.userId),
  ],
);

export const documentTransactions = pgTable(
  "document_transactions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    sourceDocumentId: uuid("source_document_id").notNull(),
    transactionId: uuid("transaction_id").notNull(),
    userId: uuid("user_id").notNull(),
    linkType: text("link_type").notNull(),
    confidence: numeric({ precision: 4, scale: 3 }),
    createdBy: text("created_by").default("user").notNull(),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_document_transactions_source").using(
      "btree",
      table.sourceDocumentId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_transactions_tx").using(
      "btree",
      table.transactionId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_document_transactions_user").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "document_transactions_source_document_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transactionId],
      foreignColumns: [transactions.id],
      name: "document_transactions_transaction_id_fkey",
    }).onDelete("cascade"),
    unique("document_transactions_source_document_id_transaction_id_lin_key").on(
      table.linkType,
      table.sourceDocumentId,
      table.transactionId,
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

export const draftBatches = pgTable(
  "draft_batches",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    runId: uuid("run_id"),
    name: text(),
    status: text().default("open").notNull(),
    totalDrafts: integer("total_drafts").default(0),
    approvedCount: integer("approved_count").default(0),
    rejectedCount: integer("rejected_count").default(0),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
    sourceDocumentId: uuid("source_document_id"),
  },
  (table) => [
    index("idx_draft_batches_source_document")
      .using("btree", table.sourceDocumentId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("idx_draft_batches_user_status").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    foreignKey({
      columns: [table.runId],
      foreignColumns: [ingestionRuns.id],
      name: "draft_batches_run_id_fkey",
    }),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "draft_batches_source_document_id_fkey",
    }).onDelete("set null"),
    check(
      "draft_batches_status_check",
      sql`status = ANY (ARRAY['open'::text, 'reviewing'::text, 'approved'::text, 'partial'::text, 'rejected'::text])`,
    ),
  ],
);

export const externalAccountMappings = pgTable(
  "external_account_mappings",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    providerConnectionId: uuid("provider_connection_id").notNull(),
    externalAccountId: text("external_account_id").notNull(),
    externalAccountName: text("external_account_name"),
    externalAccountType: text("external_account_type"),
    financialProductId: uuid("financial_product_id"),
    status: text().default("pending_mapping").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_external_account_mappings_connection").using(
      "btree",
      table.providerConnectionId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_external_account_mappings_product").using(
      "btree",
      table.financialProductId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_external_account_mappings_user").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.financialProductId],
      foreignColumns: [financialProducts.id],
      name: "external_account_mappings_financial_product_id_fkey",
    }),
    foreignKey({
      columns: [table.providerConnectionId],
      foreignColumns: [providerConnections.id],
      name: "external_account_mappings_provider_connection_id_fkey",
    }).onDelete("cascade"),
    unique("external_account_mappings_provider_connection_id_external_a_key").on(
      table.externalAccountId,
      table.providerConnectionId,
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
    userId: uuid("user_id").notNull(),
    name: text().notNull(),
    type: text().default("bank").notNull(),
    iconUrl: text("icon_url"),
    color: text(),
    isActive: boolean("is_active").default(true).notNull(),
    displayOrder: integer("display_order"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_institutions_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    check(
      "institutions_type_check",
      sql`type = ANY (ARRAY['bank'::text, 'fintech'::text, 'broker'::text, 'other'::text])`,
    ),
  ],
);

export const financialObligationIdentityKeys = pgTable(
  "financial_obligation_identity_keys",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    obligationId: uuid("obligation_id").notNull(),
    key: text().notNull(),
    keyKind: text("key_kind").notNull(),
    strength: text().notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("ix_fo_identity_keys_obligation").using(
      "btree",
      table.obligationId.asc().nullsLast().op("uuid_ops"),
    ),
    uniqueIndex("uq_fo_identity_keys").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.key.asc().nullsLast().op("text_ops"),
    ),
    foreignKey({
      columns: [table.obligationId],
      foreignColumns: [financialObligations.id],
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

export const ingestionJobs = pgTable(
  "ingestion_jobs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    runId: uuid("run_id").notNull(),
    userId: uuid("user_id").notNull(),
    sourceDocumentId: uuid("source_document_id"),
    status: text().default("discovered").notNull(),
    errorMessage: text("error_message"),
    errorDetails: jsonb("error_details"),
    retryCount: integer("retry_count").default(0),
    maxRetries: integer("max_retries").default(3),
    metadata: jsonb().default({}),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
    needsFullAiReview: boolean("needs_full_ai_review").default(false),
  },
  (table) => [
    index("idx_ingestion_jobs_needs_full_ai_review")
      .using(
        "btree",
        table.needsFullAiReview.asc().nullsLast().op("bool_ops"),
        table.userId.asc().nullsLast().op("bool_ops"),
      )
      .where(sql`(needs_full_ai_review = true)`),
    index("idx_ingestion_jobs_queue")
      .using(
        "btree",
        table.status.asc().nullsLast().op("timestamptz_ops"),
        table.createdAt.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(status = ANY (ARRAY['queued'::text, 'discovered'::text]))`),
    index("idx_ingestion_jobs_run").using(
      "btree",
      table.runId.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    index("idx_ingestion_jobs_user_status").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.runId],
      foreignColumns: [ingestionRuns.id],
      name: "ingestion_jobs_run_id_fkey",
    }),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "ingestion_jobs_source_document_id_fkey",
    }),
    check(
      "ingestion_jobs_status_check",
      sql`status = ANY (ARRAY['discovered'::text, 'downloaded'::text, 'hashed'::text, 'queued'::text, 'parsing'::text, 'parsed'::text, 'classified'::text, 'reconciled'::text, 'drafted'::text, 'pending_review'::text, 'approved'::text, 'posted'::text, 'failed'::text])`,
    ),
  ],
);

export const financialPeriods = pgTable(
  "financial_periods",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    startDate: date("start_date").notNull(),
    endDate: date("end_date").notNull(),
    label: text(),
    isCurrent: boolean("is_current").default(false).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_financial_periods_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    check("chk_period_dates", sql`end_date >= start_date`),
  ],
);

export const financialProducts = pgTable(
  "financial_products",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    institutionId: uuid("institution_id").notNull(),
    name: text().notNull(),
    type: text().notNull(),
    currentBalance: numeric("current_balance", { precision: 15, scale: 2 }).default("0"),
    creditLimit: numeric("credit_limit", { precision: 15, scale: 2 }),
    isActive: boolean("is_active").default(true).notNull(),
    displayOrder: integer("display_order"),
    metadata: jsonb(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_financial_products_institution_id").using(
      "btree",
      table.institutionId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_financial_products_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.institutionId],
      foreignColumns: [institutions.id],
      name: "financial_products_institution_id_fkey",
    }).onDelete("cascade"),
    check(
      "financial_products_type_check",
      sql`type = ANY (ARRAY['checking_account'::text, 'savings_account'::text, 'credit_card'::text, 'overdraft'::text, 'personal_loan'::text, 'mortgage'::text, 'investment'::text, 'other'::text])`,
    ),
  ],
);

export const importJobs = pgTable(
  "import_jobs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    sourceType: text("source_type").notNull(),
    filePath: text("file_path"),
    status: text().default("pending").notNull(),
    totalRows: integer("total_rows"),
    importedRows: integer("imported_rows").default(0).notNull(),
    skippedRows: integer("skipped_rows").default(0).notNull(),
    errorRows: integer("error_rows").default(0).notNull(),
    errorDetails: jsonb("error_details"),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "string" }),
    completedAt: timestamp("completed_at", { withTimezone: true, mode: "string" }),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_import_jobs_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
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

export const ingestionLogs = pgTable(
  "ingestion_logs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    runId: uuid("run_id"),
    jobId: uuid("job_id"),
    level: text().default("info").notNull(),
    message: text().notNull(),
    details: jsonb().default({}),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ingestion_logs_job")
      .using(
        "btree",
        table.jobId.asc().nullsLast().op("timestamptz_ops"),
        table.createdAt.asc().nullsLast().op("timestamptz_ops"),
      )
      .where(sql`(job_id IS NOT NULL)`),
    index("idx_ingestion_logs_run").using(
      "btree",
      table.runId.asc().nullsLast().op("uuid_ops"),
      table.createdAt.asc().nullsLast().op("timestamptz_ops"),
    ),
    index("idx_ingestion_logs_user_level")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("timestamptz_ops"),
        table.level.asc().nullsLast().op("timestamptz_ops"),
        table.createdAt.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(level = ANY (ARRAY['warn'::text, 'error'::text]))`),
    foreignKey({
      columns: [table.jobId],
      foreignColumns: [ingestionJobs.id],
      name: "ingestion_logs_job_id_fkey",
    }),
    foreignKey({
      columns: [table.runId],
      foreignColumns: [ingestionRuns.id],
      name: "ingestion_logs_run_id_fkey",
    }),
    check(
      "ingestion_logs_level_check",
      sql`level = ANY (ARRAY['debug'::text, 'info'::text, 'warn'::text, 'error'::text])`,
    ),
  ],
);

export const ingestionCheckpoints = pgTable(
  "ingestion_checkpoints",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    sourceType: text("source_type").notNull(),
    scopeKey: text("scope_key").notNull(),
    cursorKind: text("cursor_kind").notNull(),
    cursorValue: text("cursor_value"),
    lastMessageId: text("last_message_id"),
    messagesSeen: integer("messages_seen").default(0).notNull(),
    documentsCreated: integer("documents_created").default(0).notNull(),
    duplicatesSkipped: integer("duplicates_skipped").default(0).notNull(),
    errors: integer().default(0).notNull(),
    windowStart: date("window_start"),
    windowEnd: date("window_end"),
    status: text().default("running").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("ix_ingestion_checkpoints_status").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    unique("ingestion_checkpoints_user_id_source_type_scope_key_key").on(
      table.scopeKey,
      table.sourceType,
      table.userId,
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

export const extractionResults = pgTable(
  "extraction_results",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    parsedVersionId: uuid("parsed_version_id").notNull(),
    userId: uuid("user_id").notNull(),
    supplierNameRaw: text("supplier_name_raw"),
    supplierId: uuid("supplier_id"),
    supplierConfidence: numeric("supplier_confidence", { precision: 3, scale: 2 }),
    competenceDate: date("competence_date"),
    dueDate: date("due_date"),
    totalAmount: numeric("total_amount", { precision: 12, scale: 2 }),
    currency: text().default("BRL"),
    breakdown: jsonb(),
    documentNumber: text("document_number"),
    contractIdentifier: text("contract_identifier"),
    consumptionData: jsonb("consumption_data"),
    categorySuggestion: text("category_suggestion"),
    tagsSuggestion: text("tags_suggestion").array(),
    prioritySuggestion: text("priority_suggestion"),
    financialPeriodSuggestion: jsonb("financial_period_suggestion"),
    metadata: jsonb().default({}),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    aiEnrichmentType: text("ai_enrichment_type"),
    aiEnrichmentAt: timestamp("ai_enrichment_at", { withTimezone: true, mode: "string" }),
    confidencePerField: jsonb("confidence_per_field"),
    reasoning: text(),
    financialIntent: text("financial_intent"),
  },
  (table) => [
    index("idx_extraction_results_ai_enrichment_type")
      .using("btree", table.aiEnrichmentType.asc().nullsLast().op("text_ops"))
      .where(sql`(ai_enrichment_type IS NOT NULL)`),
    index("idx_extraction_results_supplier")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("uuid_ops"),
        table.supplierId.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(supplier_id IS NOT NULL)`),
    index("idx_extraction_results_version").using(
      "btree",
      table.parsedVersionId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.parsedVersionId],
      foreignColumns: [parsedDocumentVersions.id],
      name: "extraction_results_parsed_version_id_fkey",
    }),
    foreignKey({
      columns: [table.supplierId],
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

export const financialObligations = pgTable(
  "financial_obligations",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    obligationType: text("obligation_type").notNull(),
    status: text().default("open").notNull(),
    supplierId: uuid("supplier_id"),
    supplierNameRaw: text("supplier_name_raw"),
    financialProductId: uuid("financial_product_id"),
    amount: numeric({ precision: 15, scale: 2 }),
    dueDate: date("due_date"),
    competenceDate: date("competence_date"),
    cycleStartDate: date("cycle_start_date"),
    cycleEndDate: date("cycle_end_date"),
    documentNumber: text("document_number"),
    barcodeDigitableLine: text("barcode_digitable_line"),
    financialIdentityKey: text("financial_identity_key"),
    confidenceScore: numeric("confidence_score", { precision: 3, scale: 2 }),
    metadata: jsonb().default({}),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    uniqueIndex("idx_financial_obligations_identity")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.financialIdentityKey.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(financial_identity_key IS NOT NULL)`),
    index("idx_financial_obligations_user_status").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    foreignKey({
      columns: [table.financialProductId],
      foreignColumns: [financialProducts.id],
      name: "financial_obligations_financial_product_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplierId],
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
    userId: uuid("user_id").notNull(),
    financialProductId: uuid("financial_product_id").notNull(),
    name: text().notNull(),
    type: text().notNull(),
    originalAmount: numeric("original_amount", { precision: 15, scale: 2 }).notNull(),
    outstandingBalance: numeric("outstanding_balance", { precision: 15, scale: 2 }).notNull(),
    interestRate: numeric("interest_rate", { precision: 8, scale: 6 }),
    rateType: text("rate_type"),
    amortizationSystem: text("amortization_system"),
    totalInstallments: integer("total_installments"),
    paidInstallments: integer("paid_installments").default(0).notNull(),
    startDate: date("start_date"),
    endDate: date("end_date"),
    status: text().default("active").notNull(),
    metadata: jsonb(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    supplierId: uuid("supplier_id"),
  },
  (table) => [
    index("idx_liabilities_financial_product_id").using(
      "btree",
      table.financialProductId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_liabilities_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_liabilities_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.financialProductId],
      foreignColumns: [financialProducts.id],
      name: "liabilities_financial_product_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.supplierId],
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

export const ingestionRuns = pgTable(
  "ingestion_runs",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    sourceType: text("source_type").notNull(),
    status: text().default("running").notNull(),
    startedAt: timestamp("started_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    completedAt: timestamp("completed_at", { withTimezone: true, mode: "string" }),
    metadata: jsonb().default({}),
    stats: jsonb().default({}),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_ingestion_runs_user_source").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.sourceType.asc().nullsLast().op("text_ops"),
    ),
    index("idx_ingestion_runs_user_status").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
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

export const financialObligationEvidences = pgTable(
  "financial_obligation_evidences",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    obligationId: uuid("obligation_id").notNull(),
    sourceDocumentId: uuid("source_document_id").notNull(),
    evidenceRole: text("evidence_role").default("supporting").notNull(),
    confidenceScore: numeric("confidence_score", { precision: 3, scale: 2 }),
    reasons: jsonb().default([]),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_financial_obligation_evidences_document").using(
      "btree",
      table.sourceDocumentId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_financial_obligation_evidences_obligation").using(
      "btree",
      table.obligationId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.obligationId],
      foreignColumns: [financialObligations.id],
      name: "financial_obligation_evidences_obligation_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "financial_obligation_evidences_source_document_id_fkey",
    }).onDelete("cascade"),
    unique("financial_obligation_evidence_user_id_obligation_id_source__key").on(
      table.obligationId,
      table.sourceDocumentId,
      table.userId,
    ),
    check(
      "financial_obligation_evidences_evidence_role_check",
      sql`evidence_role = ANY (ARRAY['primary'::text, 'supporting'::text, 'duplicate'::text, 'conflicting'::text])`,
    ),
  ],
);

export const supplierAliases = pgTable(
  "supplier_aliases",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    supplierId: uuid("supplier_id").notNull(),
    aliasName: text("alias_name").notNull(),
    aliasType: text("alias_type").default("other").notNull(),
    isActive: boolean("is_active").default(true).notNull(),
    validFrom: date("valid_from"),
    validUntil: date("valid_until"),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_supplier_aliases_resolve")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("date_ops"),
        table.aliasName.asc().nullsLast().op("uuid_ops"),
        table.validFrom.asc().nullsLast().op("text_ops"),
        table.validUntil.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(is_active = true)`),
    index("idx_supplier_aliases_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_supplier_aliases_trgm").using(
      "gin",
      table.aliasName.asc().nullsLast().op("gin_trgm_ops"),
    ),
    index("idx_supplier_aliases_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "supplier_aliases_supplier_id_fkey",
    }).onDelete("cascade"),
    check(
      "supplier_aliases_alias_type_check",
      sql`alias_type = ANY (ARRAY['former_name'::text, 'abbreviation'::text, 'trade_name'::text, 'billing_name'::text, 'other'::text])`,
    ),
  ],
);

export const sourceDocuments = pgTable(
  "source_documents",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    originType: text("origin_type").notNull(),
    originKey: text("origin_key").notNull(),
    gmailMessageId: text("gmail_message_id"),
    gmailThreadId: text("gmail_thread_id"),
    gmailAttachmentId: text("gmail_attachment_id"),
    gmailLabel: text("gmail_label"),
    gmailDate: timestamp("gmail_date", { withTimezone: true, mode: "string" }),
    gmailFrom: text("gmail_from"),
    gmailSubject: text("gmail_subject"),
    localFilepath: text("local_filepath"),
    localMtime: timestamp("local_mtime", { withTimezone: true, mode: "string" }),
    filename: text().notNull(),
    mimeType: text("mime_type"),
    // You can use { mode: "bigint" } if numbers are exceeding js number limitations
    fileSizeBytes: bigint("file_size_bytes", { mode: "number" }),
    storagePath: text("storage_path"),
    status: text().default("new").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
    contentHash: text("content_hash"),
    metadata: jsonb(),
    documentType: text("document_type"),
    supplierId: uuid("supplier_id"),
    supplierNameRaw: text("supplier_name_raw"),
  },
  (table) => [
    index("idx_source_documents_content_hash")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("uuid_ops"),
        table.contentHash.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(content_hash IS NOT NULL)`),
    index("idx_source_documents_document_type")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("uuid_ops"),
        table.documentType.asc().nullsLast().op("text_ops"),
      )
      .where(sql`(document_type IS NOT NULL)`),
    index("idx_source_documents_gmail_msg")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.gmailMessageId.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`(gmail_message_id IS NOT NULL)`),
    uniqueIndex("idx_source_documents_gmail_msg_file_dedup")
      .using(
        "btree",
        table.userId.asc().nullsLast().op("text_ops"),
        table.gmailMessageId.asc().nullsLast().op("text_ops"),
        table.filename.asc().nullsLast().op("uuid_ops"),
      )
      .where(sql`((origin_type = 'gmail'::text) AND (gmail_message_id IS NOT NULL))`),
    index("idx_source_documents_status").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.status.asc().nullsLast().op("text_ops"),
    ),
    index("idx_source_documents_supplier_id")
      .using("btree", table.supplierId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(supplier_id IS NOT NULL)`),
    index("idx_source_documents_user_origin").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.originType.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "source_documents_supplier_id_fkey",
    }).onDelete("set null"),
    unique("source_documents_user_id_origin_type_origin_key_key").on(
      table.originKey,
      table.originType,
      table.userId,
    ),
    check(
      "source_documents_origin_type_check",
      sql`origin_type = ANY (ARRAY['gmail'::text, 'local_file'::text, 'manual_upload'::text])`,
    ),
  ],
);

export const providerConnections = pgTable(
  "provider_connections",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    provider: text().notNull(),
    externalItemId: text("external_item_id").notNull(),
    institutionName: text("institution_name"),
    status: text().default("active").notNull(),
    lastSyncedAt: timestamp("last_synced_at", { withTimezone: true, mode: "string" }),
    metadata: jsonb().default({}).notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_provider_connections_user").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    unique("provider_connections_user_id_provider_external_item_id_key").on(
      table.externalItemId,
      table.provider,
      table.userId,
    ),
    check("provider_connections_provider_check", sql`provider = 'pluggy'::text`),
    check(
      "provider_connections_status_check",
      sql`status = ANY (ARRAY['active'::text, 'error'::text, 'revoked'::text, 'expired'::text])`,
    ),
  ],
);

export const parsedDocumentVersions = pgTable(
  "parsed_document_versions",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    sourceDocumentId: uuid("source_document_id").notNull(),
    userId: uuid("user_id").notNull(),
    versionNumber: integer("version_number").default(1).notNull(),
    parserType: text("parser_type").notNull(),
    parserVersion: text("parser_version"),
    rawText: text("raw_text"),
    structuredData: jsonb("structured_data"),
    confidenceScore: numeric("confidence_score", { precision: 3, scale: 2 }),
    metadata: jsonb().default({}),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" }).defaultNow(),
  },
  (table) => [
    index("idx_parsed_versions_doc").using(
      "btree",
      table.sourceDocumentId.asc().nullsLast().op("int4_ops"),
      table.versionNumber.desc().nullsFirst().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "parsed_document_versions_source_document_id_fkey",
    }),
    unique("parsed_document_versions_source_document_id_version_number_key").on(
      table.sourceDocumentId,
      table.versionNumber,
    ),
    check(
      "parsed_document_versions_parser_type_check",
      sql`parser_type = ANY (ARRAY['local_text'::text, 'local_regex'::text, 'openai_vision'::text, 'openai_text'::text])`,
    ),
  ],
);

export const recurringTemplates = pgTable(
  "recurring_templates",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    financialProductId: uuid("financial_product_id"),
    name: text().notNull(),
    type: text().notNull(),
    amount: numeric({ precision: 15, scale: 2 }),
    isVariableAmount: boolean("is_variable_amount").default(false).notNull(),
    frequency: text().notNull(),
    dayOfMonth: integer("day_of_month"),
    customIntervalDays: integer("custom_interval_days"),
    categoryId: uuid("category_id"),
    priority: text(),
    startsAt: date("starts_at"),
    endsAt: date("ends_at"),
    isActive: boolean("is_active").default(true).notNull(),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    supplierId: uuid("supplier_id"),
  },
  (table) => [
    index("idx_recurring_templates_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_recurring_templates_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.categoryId],
      foreignColumns: [categories.id],
      name: "recurring_templates_category_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.financialProductId],
      foreignColumns: [financialProducts.id],
      name: "recurring_templates_financial_product_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplierId],
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

export const statementCycles = pgTable(
  "statement_cycles",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    cardId: uuid("card_id").notNull(),
    referenceMonth: date("reference_month").notNull(),
    cycleStartDate: date("cycle_start_date").notNull(),
    cycleEndDate: date("cycle_end_date").notNull(),
    dueDate: date("due_date").notNull(),
    totalAmount: numeric("total_amount", { precision: 15, scale: 2 }).default("0"),
    paidAmount: numeric("paid_amount", { precision: 15, scale: 2 }).default("0"),
    status: text().default("open").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_statement_cycles_card_id").using(
      "btree",
      table.cardId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_statement_cycles_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.cardId],
      foreignColumns: [cards.id],
      name: "statement_cycles_card_id_fkey",
    }).onDelete("cascade"),
    check(
      "statement_cycles_status_check",
      sql`status = ANY (ARRAY['open'::text, 'closed'::text, 'paid'::text, 'partial'::text, 'overdue'::text])`,
    ),
  ],
);

export const statementItems = pgTable(
  "statement_items",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    statementCycleId: uuid("statement_cycle_id").notNull(),
    transactionId: uuid("transaction_id"),
    description: text(),
    amount: numeric({ precision: 15, scale: 2 }).notNull(),
    transactionDate: date("transaction_date"),
    installmentNumber: integer("installment_number"),
    totalInstallments: integer("total_installments"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    supplierId: uuid("supplier_id"),
  },
  (table) => [
    index("idx_statement_items_statement_cycle_id").using(
      "btree",
      table.statementCycleId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_statement_items_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    uniqueIndex("idx_statement_items_transaction_id")
      .using("btree", table.transactionId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(transaction_id IS NOT NULL)`),
    index("idx_statement_items_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.statementCycleId],
      foreignColumns: [statementCycles.id],
      name: "statement_items_statement_cycle_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "statement_items_supplier_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.transactionId],
      foreignColumns: [transactions.id],
      name: "statement_items_transaction_id_fkey",
    }).onDelete("set null"),
  ],
);

export const supplierContracts = pgTable(
  "supplier_contracts",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    supplierId: uuid("supplier_id").notNull(),
    contractType: text("contract_type").notNull(),
    identifier: text(),
    label: text(),
    isActive: boolean("is_active").default(true).notNull(),
    startDate: date("start_date"),
    endDate: date("end_date"),
    metadata: jsonb(),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_supplier_contracts_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_supplier_contracts_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "supplier_contracts_supplier_id_fkey",
    }).onDelete("cascade"),
    check(
      "supplier_contracts_contract_type_check",
      sql`contract_type = ANY (ARRAY['service'::text, 'subscription'::text, 'utility'::text, 'loan'::text, 'insurance'::text, 'maintenance'::text, 'other'::text])`,
    ),
  ],
);

export const liabilityInstallments = pgTable(
  "liability_installments",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    liabilityId: uuid("liability_id").notNull(),
    installmentNumber: integer("installment_number").notNull(),
    dueDate: date("due_date").notNull(),
    totalAmount: numeric("total_amount", { precision: 15, scale: 2 }).notNull(),
    principalAmount: numeric("principal_amount", { precision: 15, scale: 2 }),
    interestAmount: numeric("interest_amount", { precision: 15, scale: 2 }),
    insuranceAmount: numeric("insurance_amount", { precision: 15, scale: 2 }),
    feeAmount: numeric("fee_amount", { precision: 15, scale: 2 }),
    paidAmount: numeric("paid_amount", { precision: 15, scale: 2 }).default("0").notNull(),
    paidDate: date("paid_date"),
    status: text().default("pending").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_liability_installments_liability_id").using(
      "btree",
      table.liabilityId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_liability_installments_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.liabilityId],
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
    userId: uuid("user_id").notNull(),
    name: text().notNull(),
    tradeName: text("trade_name"),
    legalName: text("legal_name"),
    documentNumber: text("document_number"),
    type: text().default("company").notNull(),
    website: text(),
    contactInfo: jsonb("contact_info"),
    notes: text(),
    institutionId: uuid("institution_id"),
    isActive: boolean("is_active").default(true).notNull(),
    displayOrder: integer("display_order"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_suppliers_name_trgm").using("gin", table.name.asc().nullsLast().op("gin_trgm_ops")),
    index("idx_suppliers_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.institutionId],
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
    userId: uuid("user_id").notNull(),
    financialProductId: uuid("financial_product_id").notNull(),
    type: text().notNull(),
    amount: numeric({ precision: 15, scale: 2 }).notNull(),
    description: text(),
    eventDate: date("event_date").notNull(),
    competenceDate: date("competence_date"),
    financialPeriodId: uuid("financial_period_id"),
    statementCycleId: uuid("statement_cycle_id"),
    liabilityInstallmentId: uuid("liability_installment_id"),
    categoryId: uuid("category_id"),
    priority: text(),
    isConfirmed: boolean("is_confirmed").default(false).notNull(),
    notes: text(),
    metadata: jsonb(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    originType: text("origin_type").default("manual").notNull(),
    // Referência cruzada para recurring_instances (declarada mais abaixo neste
    // arquivo): usa .references() com callback tipado (AnyPgColumn) em vez do
    // foreignKey() builder para quebrar o ciclo de inferência de tipos entre
    // transactions <-> recurring_instances (TS7022/TS7024 — ver edição manual
    // nº 1 no cabeçalho deste arquivo). O nome do constraint gerado pelo
    // Drizzle passa a ser o
    // default automático em vez de "transactions_recurring_instance_id_fkey"
    // (mesma semântica de onDelete "set null"); isso não afeta queries, só
    // teria efeito se este schema fosse usado para `drizzle-kit generate`/push,
    // o que não é o caso neste projeto (schema.sql/Neon são a fonte da verdade).
    recurringInstanceId: uuid("recurring_instance_id").references(
      (): AnyPgColumn => recurringInstances.id,
      { onDelete: "set null" },
    ),
    supplierId: uuid("supplier_id"),
    sourceDocumentId: uuid("source_document_id"),
  },
  (table) => [
    index("idx_transactions_category_id").using(
      "btree",
      table.categoryId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_event_date").using(
      "btree",
      table.userId.asc().nullsLast().op("date_ops"),
      table.eventDate.asc().nullsLast().op("date_ops"),
    ),
    index("idx_transactions_financial_period_id").using(
      "btree",
      table.financialPeriodId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_financial_product_id").using(
      "btree",
      table.financialProductId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_recurring_instance_id").using(
      "btree",
      table.recurringInstanceId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_source_document_id")
      .using("btree", table.sourceDocumentId.asc().nullsLast().op("uuid_ops"))
      .where(sql`(source_document_id IS NOT NULL)`),
    index("idx_transactions_statement_cycle_id").using(
      "btree",
      table.statementCycleId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transactions_type").using(
      "btree",
      table.userId.asc().nullsLast().op("text_ops"),
      table.type.asc().nullsLast().op("text_ops"),
    ),
    index("idx_transactions_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.categoryId],
      foreignColumns: [categories.id],
      name: "transactions_category_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.financialPeriodId],
      foreignColumns: [financialPeriods.id],
      name: "transactions_financial_period_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.financialProductId],
      foreignColumns: [financialProducts.id],
      name: "transactions_financial_product_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.liabilityInstallmentId],
      foreignColumns: [liabilityInstallments.id],
      name: "transactions_liability_installment_id_fkey",
    }).onDelete("set null"),
    // FK para recurring_instances agora definida inline na coluna acima
    // (recurringInstanceId.references(...)) para quebrar o ciclo de tipos —
    // ver comentário na definição da coluna.
    foreignKey({
      columns: [table.sourceDocumentId],
      foreignColumns: [sourceDocuments.id],
      name: "transactions_source_document_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.statementCycleId],
      foreignColumns: [statementCycles.id],
      name: "transactions_statement_cycle_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.supplierId],
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

export const recurringInstances = pgTable(
  "recurring_instances",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    recurringTemplateId: uuid("recurring_template_id").notNull(),
    expectedDate: date("expected_date").notNull(),
    expectedAmount: numeric("expected_amount", { precision: 15, scale: 2 }),
    actualAmount: numeric("actual_amount", { precision: 15, scale: 2 }),
    status: text().default("pending").notNull(),
    paidDate: date("paid_date"),
    transactionId: uuid("transaction_id"),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_recurring_instances_template_id").using(
      "btree",
      table.recurringTemplateId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_recurring_instances_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.recurringTemplateId],
      foreignColumns: [recurringTemplates.id],
      name: "recurring_instances_recurring_template_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transactionId],
      foreignColumns: [transactions.id],
      name: "recurring_instances_transaction_id_fkey",
    }).onDelete("set null"),
    check(
      "recurring_instances_status_check",
      sql`status = ANY (ARRAY['pending'::text, 'paid'::text, 'partial'::text, 'skipped'::text, 'overdue'::text, 'cancelled'::text])`,
    ),
  ],
);

export const supplierTags = pgTable(
  "supplier_tags",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    supplierId: uuid("supplier_id").notNull(),
    tagId: uuid("tag_id").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_supplier_tags_supplier_id").using(
      "btree",
      table.supplierId.asc().nullsLast().op("uuid_ops"),
    ),
    foreignKey({
      columns: [table.supplierId],
      foreignColumns: [suppliers.id],
      name: "supplier_tags_supplier_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.tagId],
      foreignColumns: [tags.id],
      name: "supplier_tags_tag_id_fkey",
    }).onDelete("cascade"),
    unique("supplier_tags_supplier_id_tag_id_key").on(table.supplierId, table.tagId),
  ],
);

export const tags = pgTable(
  "tags",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    name: text().notNull(),
    color: text(),
    influencesPriority: boolean("influences_priority").default(false).notNull(),
    suggestedPriority: text("suggested_priority"),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_tags_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
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
    userId: uuid("user_id").notNull(),
    sourceProductId: uuid("source_product_id").notNull(),
    targetProductId: uuid("target_product_id").notNull(),
    amount: numeric({ precision: 15, scale: 2 }).notNull(),
    description: text(),
    eventDate: date("event_date").notNull(),
    competenceDate: date("competence_date"),
    financialPeriodId: uuid("financial_period_id"),
    isConfirmed: boolean("is_confirmed").default(false).notNull(),
    notes: text(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_transfers_source_product_id").using(
      "btree",
      table.sourceProductId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transfers_target_product_id").using(
      "btree",
      table.targetProductId.asc().nullsLast().op("uuid_ops"),
    ),
    index("idx_transfers_user_id").using("btree", table.userId.asc().nullsLast().op("uuid_ops")),
    foreignKey({
      columns: [table.financialPeriodId],
      foreignColumns: [financialPeriods.id],
      name: "transfers_financial_period_id_fkey",
    }).onDelete("set null"),
    foreignKey({
      columns: [table.sourceProductId],
      foreignColumns: [financialProducts.id],
      name: "transfers_source_product_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.targetProductId],
      foreignColumns: [financialProducts.id],
      name: "transfers_target_product_id_fkey",
    }).onDelete("cascade"),
  ],
);

export const userFinancialPreferences = pgTable(
  "user_financial_preferences",
  {
    id: uuid().defaultRandom().primaryKey().notNull(),
    userId: uuid("user_id").notNull(),
    financialCycleStartDay: integer("financial_cycle_start_day"),
    financialCycleAnchorDate: date("financial_cycle_anchor_date"),
    defaultCurrency: text("default_currency").default("BRL").notNull(),
    createdAt: timestamp("created_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
    updatedAt: timestamp("updated_at", { withTimezone: true, mode: "string" })
      .defaultNow()
      .notNull(),
  },
  (table) => [
    index("idx_user_financial_preferences_user_id").using(
      "btree",
      table.userId.asc().nullsLast().op("uuid_ops"),
    ),
    unique("user_financial_preferences_user_id_key").on(table.userId),
    check(
      "user_financial_preferences_financial_cycle_start_day_check",
      sql`(financial_cycle_start_day >= 1) AND (financial_cycle_start_day <= 31)`,
    ),
  ],
);

export const liabilityTags = pgTable(
  "liability_tags",
  {
    liabilityId: uuid("liability_id").notNull(),
    tagId: uuid("tag_id").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.liabilityId],
      foreignColumns: [liabilities.id],
      name: "liability_tags_liability_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.tagId],
      foreignColumns: [tags.id],
      name: "liability_tags_tag_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.liabilityId, table.tagId], name: "liability_tags_pkey" }),
  ],
);

export const recurringTemplateTags = pgTable(
  "recurring_template_tags",
  {
    recurringTemplateId: uuid("recurring_template_id").notNull(),
    tagId: uuid("tag_id").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.recurringTemplateId],
      foreignColumns: [recurringTemplates.id],
      name: "recurring_template_tags_recurring_template_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.tagId],
      foreignColumns: [tags.id],
      name: "recurring_template_tags_tag_id_fkey",
    }).onDelete("cascade"),
    primaryKey({
      columns: [table.recurringTemplateId, table.tagId],
      name: "recurring_template_tags_pkey",
    }),
  ],
);

export const transactionTags = pgTable(
  "transaction_tags",
  {
    transactionId: uuid("transaction_id").notNull(),
    tagId: uuid("tag_id").notNull(),
  },
  (table) => [
    foreignKey({
      columns: [table.tagId],
      foreignColumns: [tags.id],
      name: "transaction_tags_tag_id_fkey",
    }).onDelete("cascade"),
    foreignKey({
      columns: [table.transactionId],
      foreignColumns: [transactions.id],
      name: "transaction_tags_transaction_id_fkey",
    }).onDelete("cascade"),
    primaryKey({ columns: [table.tagId, table.transactionId], name: "transaction_tags_pkey" }),
  ],
);
export const vExpensesDeduplicated = pgView("v_expenses_deduplicated", {
  canonicalId: uuid("canonical_id"),
  sourceType: text("source_type"),
  userId: uuid("user_id"),
  amount: numeric({ precision: 15, scale: 2 }),
  description: text(),
  supplierId: uuid("supplier_id"),
  categoryId: uuid("category_id"),
  priority: text(),
  eventDate: date("event_date"),
  competenceDate: date("competence_date"),
  financialPeriodId: uuid("financial_period_id"),
  statementCycleId: uuid("statement_cycle_id"),
}).as(
  sql`SELECT si.id AS canonical_id, 'statement_item'::text AS source_type, si.user_id, si.amount, si.description, si.supplier_id, NULL::uuid AS category_id, NULL::text AS priority, si.transaction_date AS event_date, NULL::date AS competence_date, NULL::uuid AS financial_period_id, si.statement_cycle_id FROM statement_items si WHERE si.transaction_id IS NULL UNION ALL SELECT t.id AS canonical_id, 'transaction'::text AS source_type, t.user_id, t.amount, t.description, t.supplier_id, t.category_id, t.priority, t.event_date, t.competence_date, t.financial_period_id, t.statement_cycle_id FROM transactions t WHERE (t.type = ANY (ARRAY['expense'::text, 'fee'::text, 'interest_charge'::text])) AND (t.type <> ALL (ARRAY['statement_payment'::text, 'refund'::text]))`,
);

export const mvSupplierSpending = pgMaterializedView("mv_supplier_spending", {
  userId: uuid("user_id"),
  supplierId: uuid("supplier_id"),
  supplierName: text("supplier_name"),
  supplierType: text("supplier_type"),
  // You can use { mode: "bigint" } if numbers are exceeding js number limitations
  transactionCount: bigint("transaction_count", { mode: "number" }),
  totalSpent: numeric("total_spent"),
  firstTransactionDate: date("first_transaction_date"),
  lastTransactionDate: date("last_transaction_date"),
  // You can use { mode: "bigint" } if numbers are exceeding js number limitations
  periodsActive: bigint("periods_active", { mode: "number" }),
}).as(
  sql`SELECT s.user_id, s.id AS supplier_id, s.name AS supplier_name, s.type AS supplier_type, count(DISTINCT t.id) AS transaction_count, COALESCE(sum(t.amount), 0::numeric) AS total_spent, min(t.event_date) AS first_transaction_date, max(t.event_date) AS last_transaction_date, count(DISTINCT t.financial_period_id) AS periods_active FROM suppliers s LEFT JOIN transactions t ON t.supplier_id = s.id AND t.user_id = s.user_id AND (t.type = ANY (ARRAY['expense'::text, 'fee'::text, 'interest_charge'::text])) WHERE s.is_active = true GROUP BY s.user_id, s.id, s.name, s.type`,
);
