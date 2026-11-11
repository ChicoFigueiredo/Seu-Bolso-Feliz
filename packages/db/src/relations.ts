import { relations } from "drizzle-orm/relations";
import {
  institutions,
  document_patterns,
  suppliers,
  pattern_feedback,
  source_documents,
  financial_products,
  cards,
  categories,
  documents_legacy,
  consumption_metrics,
  supplier_contracts,
  transactions,
  document_splits,
  draft_batches,
  draft_records,
  extraction_results,
  financial_obligations,
  recurring_templates,
  ai_chat_sessions,
  ai_chat_messages,
  document_fingerprints,
  document_transactions,
  ingestion_runs,
  external_account_mappings,
  provider_connections,
  financial_obligation_identity_keys,
  ingestion_jobs,
  ingestion_logs,
  parsed_document_versions,
  liabilities,
  financial_obligation_evidences,
  supplier_aliases,
  statement_cycles,
  statement_items,
  liability_installments,
  financial_periods,
  recurring_instances,
  supplier_tags,
  tags,
  transfers,
  liability_tags,
  recurring_template_tags,
  transaction_tags,
} from "./schema";

export const document_patternsRelations = relations(document_patterns, ({ one, many }) => ({
  institution: one(institutions, {
    fields: [document_patterns.institution_id],
    references: [institutions.id],
  }),
  supplier: one(suppliers, {
    fields: [document_patterns.supplier_id],
    references: [suppliers.id],
  }),
  pattern_feedbacks: many(pattern_feedback),
}));

export const institutionsRelations = relations(institutions, ({ many }) => ({
  document_patterns: many(document_patterns),
  financial_products: many(financial_products),
  suppliers: many(suppliers),
}));

export const suppliersRelations = relations(suppliers, ({ one, many }) => ({
  document_patterns: many(document_patterns),
  consumption_metrics: many(consumption_metrics),
  documents_legacies: many(documents_legacy),
  extraction_results: many(extraction_results),
  financial_obligations: many(financial_obligations),
  liabilities: many(liabilities),
  supplier_aliases: many(supplier_aliases),
  source_documents: many(source_documents),
  recurring_templates: many(recurring_templates),
  statement_items: many(statement_items),
  supplier_contracts: many(supplier_contracts),
  institution: one(institutions, {
    fields: [suppliers.institution_id],
    references: [institutions.id],
  }),
  transactions: many(transactions),
  supplier_tags: many(supplier_tags),
}));

export const pattern_feedbackRelations = relations(pattern_feedback, ({ one }) => ({
  document_pattern: one(document_patterns, {
    fields: [pattern_feedback.pattern_id],
    references: [document_patterns.id],
  }),
  source_document: one(source_documents, {
    fields: [pattern_feedback.source_document_id],
    references: [source_documents.id],
  }),
}));

export const source_documentsRelations = relations(source_documents, ({ one, many }) => ({
  pattern_feedbacks: many(pattern_feedback),
  document_splits: many(document_splits),
  draft_records: many(draft_records),
  document_fingerprints: many(document_fingerprints),
  document_transactions: many(document_transactions),
  draft_batches: many(draft_batches),
  ingestion_jobs: many(ingestion_jobs),
  financial_obligation_evidences: many(financial_obligation_evidences),
  supplier: one(suppliers, {
    fields: [source_documents.supplier_id],
    references: [suppliers.id],
  }),
  parsed_document_versions: many(parsed_document_versions),
  transactions: many(transactions),
}));

export const cardsRelations = relations(cards, ({ one, many }) => ({
  financial_product: one(financial_products, {
    fields: [cards.financial_product_id],
    references: [financial_products.id],
  }),
  statement_cycles: many(statement_cycles),
}));

export const financial_productsRelations = relations(financial_products, ({ one, many }) => ({
  cards: many(cards),
  external_account_mappings: many(external_account_mappings),
  institution: one(institutions, {
    fields: [financial_products.institution_id],
    references: [institutions.id],
  }),
  financial_obligations: many(financial_obligations),
  liabilities: many(liabilities),
  recurring_templates: many(recurring_templates),
  transactions: many(transactions),
  transfers_source_product_id: many(transfers, {
    relationName: "transfers_source_product_id_financial_products_id",
  }),
  transfers_target_product_id: many(transfers, {
    relationName: "transfers_target_product_id_financial_products_id",
  }),
}));

export const categoriesRelations = relations(categories, ({ one, many }) => ({
  category: one(categories, {
    fields: [categories.parent_id],
    references: [categories.id],
    relationName: "categories_parent_id_categories_id",
  }),
  categories: many(categories, {
    relationName: "categories_parent_id_categories_id",
  }),
  document_splits: many(document_splits),
  recurring_templates: many(recurring_templates),
  transactions: many(transactions),
}));

export const consumption_metricsRelations = relations(consumption_metrics, ({ one }) => ({
  documents_legacy: one(documents_legacy, {
    fields: [consumption_metrics.document_id],
    references: [documents_legacy.id],
  }),
  supplier_contract: one(supplier_contracts, {
    fields: [consumption_metrics.supplier_contract_id],
    references: [supplier_contracts.id],
  }),
  supplier: one(suppliers, {
    fields: [consumption_metrics.supplier_id],
    references: [suppliers.id],
  }),
  transaction: one(transactions, {
    fields: [consumption_metrics.transaction_id],
    references: [transactions.id],
  }),
}));

export const documents_legacyRelations = relations(documents_legacy, ({ one, many }) => ({
  consumption_metrics: many(consumption_metrics),
  supplier: one(suppliers, {
    fields: [documents_legacy.supplier_id],
    references: [suppliers.id],
  }),
}));

export const supplier_contractsRelations = relations(supplier_contracts, ({ one, many }) => ({
  consumption_metrics: many(consumption_metrics),
  supplier: one(suppliers, {
    fields: [supplier_contracts.supplier_id],
    references: [suppliers.id],
  }),
}));

export const transactionsRelations = relations(transactions, ({ one, many }) => ({
  consumption_metrics: many(consumption_metrics),
  draft_records: many(draft_records),
  document_transactions: many(document_transactions),
  statement_items: many(statement_items),
  category: one(categories, {
    fields: [transactions.category_id],
    references: [categories.id],
  }),
  financial_period: one(financial_periods, {
    fields: [transactions.financial_period_id],
    references: [financial_periods.id],
  }),
  financial_product: one(financial_products, {
    fields: [transactions.financial_product_id],
    references: [financial_products.id],
  }),
  liability_installment: one(liability_installments, {
    fields: [transactions.liability_installment_id],
    references: [liability_installments.id],
  }),
  recurring_instance: one(recurring_instances, {
    fields: [transactions.recurring_instance_id],
    references: [recurring_instances.id],
    relationName: "transactions_recurring_instance_id_recurring_instances_id",
  }),
  source_document: one(source_documents, {
    fields: [transactions.source_document_id],
    references: [source_documents.id],
  }),
  statement_cycle: one(statement_cycles, {
    fields: [transactions.statement_cycle_id],
    references: [statement_cycles.id],
  }),
  supplier: one(suppliers, {
    fields: [transactions.supplier_id],
    references: [suppliers.id],
  }),
  recurring_instances: many(recurring_instances, {
    relationName: "recurring_instances_transaction_id_transactions_id",
  }),
  transaction_tags: many(transaction_tags),
}));

export const document_splitsRelations = relations(document_splits, ({ one }) => ({
  category: one(categories, {
    fields: [document_splits.category_id],
    references: [categories.id],
  }),
  source_document: one(source_documents, {
    fields: [document_splits.source_document_id],
    references: [source_documents.id],
  }),
}));

export const draft_recordsRelations = relations(draft_records, ({ one }) => ({
  draft_batch: one(draft_batches, {
    fields: [draft_records.batch_id],
    references: [draft_batches.id],
  }),
  extraction_result: one(extraction_results, {
    fields: [draft_records.extraction_result_id],
    references: [extraction_results.id],
  }),
  financial_obligation: one(financial_obligations, {
    fields: [draft_records.obligation_id],
    references: [financial_obligations.id],
  }),
  recurring_template: one(recurring_templates, {
    fields: [draft_records.reconciled_template_id],
    references: [recurring_templates.id],
  }),
  transaction: one(transactions, {
    fields: [draft_records.reconciled_transaction_id],
    references: [transactions.id],
  }),
  source_document: one(source_documents, {
    fields: [draft_records.source_document_id],
    references: [source_documents.id],
  }),
}));

export const draft_batchesRelations = relations(draft_batches, ({ one, many }) => ({
  draft_records: many(draft_records),
  ingestion_run: one(ingestion_runs, {
    fields: [draft_batches.run_id],
    references: [ingestion_runs.id],
  }),
  source_document: one(source_documents, {
    fields: [draft_batches.source_document_id],
    references: [source_documents.id],
  }),
}));

export const extraction_resultsRelations = relations(extraction_results, ({ one, many }) => ({
  draft_records: many(draft_records),
  parsed_document_version: one(parsed_document_versions, {
    fields: [extraction_results.parsed_version_id],
    references: [parsed_document_versions.id],
  }),
  supplier: one(suppliers, {
    fields: [extraction_results.supplier_id],
    references: [suppliers.id],
  }),
}));

export const financial_obligationsRelations = relations(financial_obligations, ({ one, many }) => ({
  draft_records: many(draft_records),
  financial_obligation_identity_keys: many(financial_obligation_identity_keys),
  financial_product: one(financial_products, {
    fields: [financial_obligations.financial_product_id],
    references: [financial_products.id],
  }),
  supplier: one(suppliers, {
    fields: [financial_obligations.supplier_id],
    references: [suppliers.id],
  }),
  financial_obligation_evidences: many(financial_obligation_evidences),
}));

export const recurring_templatesRelations = relations(recurring_templates, ({ one, many }) => ({
  draft_records: many(draft_records),
  category: one(categories, {
    fields: [recurring_templates.category_id],
    references: [categories.id],
  }),
  financial_product: one(financial_products, {
    fields: [recurring_templates.financial_product_id],
    references: [financial_products.id],
  }),
  supplier: one(suppliers, {
    fields: [recurring_templates.supplier_id],
    references: [suppliers.id],
  }),
  recurring_instances: many(recurring_instances),
  recurring_template_tags: many(recurring_template_tags),
}));

export const ai_chat_messagesRelations = relations(ai_chat_messages, ({ one }) => ({
  ai_chat_session: one(ai_chat_sessions, {
    fields: [ai_chat_messages.session_id],
    references: [ai_chat_sessions.id],
  }),
}));

export const ai_chat_sessionsRelations = relations(ai_chat_sessions, ({ many }) => ({
  ai_chat_messages: many(ai_chat_messages),
}));

export const document_fingerprintsRelations = relations(document_fingerprints, ({ one }) => ({
  source_document: one(source_documents, {
    fields: [document_fingerprints.source_document_id],
    references: [source_documents.id],
  }),
}));

export const document_transactionsRelations = relations(document_transactions, ({ one }) => ({
  source_document: one(source_documents, {
    fields: [document_transactions.source_document_id],
    references: [source_documents.id],
  }),
  transaction: one(transactions, {
    fields: [document_transactions.transaction_id],
    references: [transactions.id],
  }),
}));

export const ingestion_runsRelations = relations(ingestion_runs, ({ many }) => ({
  draft_batches: many(draft_batches),
  ingestion_jobs: many(ingestion_jobs),
  ingestion_logs: many(ingestion_logs),
}));

export const external_account_mappingsRelations = relations(
  external_account_mappings,
  ({ one }) => ({
    financial_product: one(financial_products, {
      fields: [external_account_mappings.financial_product_id],
      references: [financial_products.id],
    }),
    provider_connection: one(provider_connections, {
      fields: [external_account_mappings.provider_connection_id],
      references: [provider_connections.id],
    }),
  }),
);

export const provider_connectionsRelations = relations(provider_connections, ({ many }) => ({
  external_account_mappings: many(external_account_mappings),
}));

export const financial_obligation_identity_keysRelations = relations(
  financial_obligation_identity_keys,
  ({ one }) => ({
    financial_obligation: one(financial_obligations, {
      fields: [financial_obligation_identity_keys.obligation_id],
      references: [financial_obligations.id],
    }),
  }),
);

export const ingestion_jobsRelations = relations(ingestion_jobs, ({ one, many }) => ({
  ingestion_run: one(ingestion_runs, {
    fields: [ingestion_jobs.run_id],
    references: [ingestion_runs.id],
  }),
  source_document: one(source_documents, {
    fields: [ingestion_jobs.source_document_id],
    references: [source_documents.id],
  }),
  ingestion_logs: many(ingestion_logs),
}));

export const ingestion_logsRelations = relations(ingestion_logs, ({ one }) => ({
  ingestion_job: one(ingestion_jobs, {
    fields: [ingestion_logs.job_id],
    references: [ingestion_jobs.id],
  }),
  ingestion_run: one(ingestion_runs, {
    fields: [ingestion_logs.run_id],
    references: [ingestion_runs.id],
  }),
}));

export const parsed_document_versionsRelations = relations(
  parsed_document_versions,
  ({ one, many }) => ({
    extraction_results: many(extraction_results),
    source_document: one(source_documents, {
      fields: [parsed_document_versions.source_document_id],
      references: [source_documents.id],
    }),
  }),
);

export const liabilitiesRelations = relations(liabilities, ({ one, many }) => ({
  financial_product: one(financial_products, {
    fields: [liabilities.financial_product_id],
    references: [financial_products.id],
  }),
  supplier: one(suppliers, {
    fields: [liabilities.supplier_id],
    references: [suppliers.id],
  }),
  liability_installments: many(liability_installments),
  liability_tags: many(liability_tags),
}));

export const financial_obligation_evidencesRelations = relations(
  financial_obligation_evidences,
  ({ one }) => ({
    financial_obligation: one(financial_obligations, {
      fields: [financial_obligation_evidences.obligation_id],
      references: [financial_obligations.id],
    }),
    source_document: one(source_documents, {
      fields: [financial_obligation_evidences.source_document_id],
      references: [source_documents.id],
    }),
  }),
);

export const supplier_aliasesRelations = relations(supplier_aliases, ({ one }) => ({
  supplier: one(suppliers, {
    fields: [supplier_aliases.supplier_id],
    references: [suppliers.id],
  }),
}));

export const statement_cyclesRelations = relations(statement_cycles, ({ one, many }) => ({
  card: one(cards, {
    fields: [statement_cycles.card_id],
    references: [cards.id],
  }),
  statement_items: many(statement_items),
  transactions: many(transactions),
}));

export const statement_itemsRelations = relations(statement_items, ({ one }) => ({
  statement_cycle: one(statement_cycles, {
    fields: [statement_items.statement_cycle_id],
    references: [statement_cycles.id],
  }),
  supplier: one(suppliers, {
    fields: [statement_items.supplier_id],
    references: [suppliers.id],
  }),
  transaction: one(transactions, {
    fields: [statement_items.transaction_id],
    references: [transactions.id],
  }),
}));

export const liability_installmentsRelations = relations(
  liability_installments,
  ({ one, many }) => ({
    liability: one(liabilities, {
      fields: [liability_installments.liability_id],
      references: [liabilities.id],
    }),
    transactions: many(transactions),
  }),
);

export const financial_periodsRelations = relations(financial_periods, ({ many }) => ({
  transactions: many(transactions),
  transfers: many(transfers),
}));

export const recurring_instancesRelations = relations(recurring_instances, ({ one, many }) => ({
  transactions: many(transactions, {
    relationName: "transactions_recurring_instance_id_recurring_instances_id",
  }),
  recurring_template: one(recurring_templates, {
    fields: [recurring_instances.recurring_template_id],
    references: [recurring_templates.id],
  }),
  transaction: one(transactions, {
    fields: [recurring_instances.transaction_id],
    references: [transactions.id],
    relationName: "recurring_instances_transaction_id_transactions_id",
  }),
}));

export const supplier_tagsRelations = relations(supplier_tags, ({ one }) => ({
  supplier: one(suppliers, {
    fields: [supplier_tags.supplier_id],
    references: [suppliers.id],
  }),
  tag: one(tags, {
    fields: [supplier_tags.tag_id],
    references: [tags.id],
  }),
}));

export const tagsRelations = relations(tags, ({ many }) => ({
  supplier_tags: many(supplier_tags),
  liability_tags: many(liability_tags),
  recurring_template_tags: many(recurring_template_tags),
  transaction_tags: many(transaction_tags),
}));

export const transfersRelations = relations(transfers, ({ one }) => ({
  financial_period: one(financial_periods, {
    fields: [transfers.financial_period_id],
    references: [financial_periods.id],
  }),
  financial_product_source_product_id: one(financial_products, {
    fields: [transfers.source_product_id],
    references: [financial_products.id],
    relationName: "transfers_source_product_id_financial_products_id",
  }),
  financial_product_target_product_id: one(financial_products, {
    fields: [transfers.target_product_id],
    references: [financial_products.id],
    relationName: "transfers_target_product_id_financial_products_id",
  }),
}));

export const liability_tagsRelations = relations(liability_tags, ({ one }) => ({
  liability: one(liabilities, {
    fields: [liability_tags.liability_id],
    references: [liabilities.id],
  }),
  tag: one(tags, {
    fields: [liability_tags.tag_id],
    references: [tags.id],
  }),
}));

export const recurring_template_tagsRelations = relations(recurring_template_tags, ({ one }) => ({
  recurring_template: one(recurring_templates, {
    fields: [recurring_template_tags.recurring_template_id],
    references: [recurring_templates.id],
  }),
  tag: one(tags, {
    fields: [recurring_template_tags.tag_id],
    references: [tags.id],
  }),
}));

export const transaction_tagsRelations = relations(transaction_tags, ({ one }) => ({
  tag: one(tags, {
    fields: [transaction_tags.tag_id],
    references: [tags.id],
  }),
  transaction: one(transactions, {
    fields: [transaction_tags.transaction_id],
    references: [transactions.id],
  }),
}));
