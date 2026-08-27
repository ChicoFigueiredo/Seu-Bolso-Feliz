import { relations } from "drizzle-orm/relations";
import {
  institutions,
  documentPatterns,
  suppliers,
  patternFeedback,
  sourceDocuments,
  financialProducts,
  cards,
  categories,
  documentsLegacy,
  consumptionMetrics,
  supplierContracts,
  transactions,
  documentSplits,
  draftBatches,
  draftRecords,
  extractionResults,
  financialObligations,
  recurringTemplates,
  aiChatSessions,
  aiChatMessages,
  documentFingerprints,
  documentTransactions,
  ingestionRuns,
  externalAccountMappings,
  providerConnections,
  financialObligationIdentityKeys,
  ingestionJobs,
  ingestionLogs,
  parsedDocumentVersions,
  liabilities,
  financialObligationEvidences,
  supplierAliases,
  statementCycles,
  statementItems,
  liabilityInstallments,
  financialPeriods,
  recurringInstances,
  supplierTags,
  tags,
  transfers,
  liabilityTags,
  recurringTemplateTags,
  transactionTags,
} from "./schema";

export const documentPatternsRelations = relations(documentPatterns, ({ one, many }) => ({
  institution: one(institutions, {
    fields: [documentPatterns.institutionId],
    references: [institutions.id],
  }),
  supplier: one(suppliers, {
    fields: [documentPatterns.supplierId],
    references: [suppliers.id],
  }),
  patternFeedbacks: many(patternFeedback),
}));

export const institutionsRelations = relations(institutions, ({ many }) => ({
  documentPatterns: many(documentPatterns),
  financialProducts: many(financialProducts),
  suppliers: many(suppliers),
}));

export const suppliersRelations = relations(suppliers, ({ one, many }) => ({
  documentPatterns: many(documentPatterns),
  consumptionMetrics: many(consumptionMetrics),
  documentsLegacies: many(documentsLegacy),
  extractionResults: many(extractionResults),
  financialObligations: many(financialObligations),
  liabilities: many(liabilities),
  supplierAliases: many(supplierAliases),
  sourceDocuments: many(sourceDocuments),
  recurringTemplates: many(recurringTemplates),
  statementItems: many(statementItems),
  supplierContracts: many(supplierContracts),
  institution: one(institutions, {
    fields: [suppliers.institutionId],
    references: [institutions.id],
  }),
  transactions: many(transactions),
  supplierTags: many(supplierTags),
}));

export const patternFeedbackRelations = relations(patternFeedback, ({ one }) => ({
  documentPattern: one(documentPatterns, {
    fields: [patternFeedback.patternId],
    references: [documentPatterns.id],
  }),
  sourceDocument: one(sourceDocuments, {
    fields: [patternFeedback.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
}));

export const sourceDocumentsRelations = relations(sourceDocuments, ({ one, many }) => ({
  patternFeedbacks: many(patternFeedback),
  documentSplits: many(documentSplits),
  draftRecords: many(draftRecords),
  documentFingerprints: many(documentFingerprints),
  documentTransactions: many(documentTransactions),
  draftBatches: many(draftBatches),
  ingestionJobs: many(ingestionJobs),
  financialObligationEvidences: many(financialObligationEvidences),
  supplier: one(suppliers, {
    fields: [sourceDocuments.supplierId],
    references: [suppliers.id],
  }),
  parsedDocumentVersions: many(parsedDocumentVersions),
  transactions: many(transactions),
}));

export const cardsRelations = relations(cards, ({ one, many }) => ({
  financialProduct: one(financialProducts, {
    fields: [cards.financialProductId],
    references: [financialProducts.id],
  }),
  statementCycles: many(statementCycles),
}));

export const financialProductsRelations = relations(financialProducts, ({ one, many }) => ({
  cards: many(cards),
  externalAccountMappings: many(externalAccountMappings),
  institution: one(institutions, {
    fields: [financialProducts.institutionId],
    references: [institutions.id],
  }),
  financialObligations: many(financialObligations),
  liabilities: many(liabilities),
  recurringTemplates: many(recurringTemplates),
  transactions: many(transactions),
  transfers_sourceProductId: many(transfers, {
    relationName: "transfers_sourceProductId_financialProducts_id",
  }),
  transfers_targetProductId: many(transfers, {
    relationName: "transfers_targetProductId_financialProducts_id",
  }),
}));

export const categoriesRelations = relations(categories, ({ one, many }) => ({
  category: one(categories, {
    fields: [categories.parentId],
    references: [categories.id],
    relationName: "categories_parentId_categories_id",
  }),
  categories: many(categories, {
    relationName: "categories_parentId_categories_id",
  }),
  documentSplits: many(documentSplits),
  recurringTemplates: many(recurringTemplates),
  transactions: many(transactions),
}));

export const consumptionMetricsRelations = relations(consumptionMetrics, ({ one }) => ({
  documentsLegacy: one(documentsLegacy, {
    fields: [consumptionMetrics.documentId],
    references: [documentsLegacy.id],
  }),
  supplierContract: one(supplierContracts, {
    fields: [consumptionMetrics.supplierContractId],
    references: [supplierContracts.id],
  }),
  supplier: one(suppliers, {
    fields: [consumptionMetrics.supplierId],
    references: [suppliers.id],
  }),
  transaction: one(transactions, {
    fields: [consumptionMetrics.transactionId],
    references: [transactions.id],
  }),
}));

export const documentsLegacyRelations = relations(documentsLegacy, ({ one, many }) => ({
  consumptionMetrics: many(consumptionMetrics),
  supplier: one(suppliers, {
    fields: [documentsLegacy.supplierId],
    references: [suppliers.id],
  }),
}));

export const supplierContractsRelations = relations(supplierContracts, ({ one, many }) => ({
  consumptionMetrics: many(consumptionMetrics),
  supplier: one(suppliers, {
    fields: [supplierContracts.supplierId],
    references: [suppliers.id],
  }),
}));

export const transactionsRelations = relations(transactions, ({ one, many }) => ({
  consumptionMetrics: many(consumptionMetrics),
  draftRecords: many(draftRecords),
  documentTransactions: many(documentTransactions),
  statementItems: many(statementItems),
  category: one(categories, {
    fields: [transactions.categoryId],
    references: [categories.id],
  }),
  financialPeriod: one(financialPeriods, {
    fields: [transactions.financialPeriodId],
    references: [financialPeriods.id],
  }),
  financialProduct: one(financialProducts, {
    fields: [transactions.financialProductId],
    references: [financialProducts.id],
  }),
  liabilityInstallment: one(liabilityInstallments, {
    fields: [transactions.liabilityInstallmentId],
    references: [liabilityInstallments.id],
  }),
  recurringInstance: one(recurringInstances, {
    fields: [transactions.recurringInstanceId],
    references: [recurringInstances.id],
    relationName: "transactions_recurringInstanceId_recurringInstances_id",
  }),
  sourceDocument: one(sourceDocuments, {
    fields: [transactions.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
  statementCycle: one(statementCycles, {
    fields: [transactions.statementCycleId],
    references: [statementCycles.id],
  }),
  supplier: one(suppliers, {
    fields: [transactions.supplierId],
    references: [suppliers.id],
  }),
  recurringInstances: many(recurringInstances, {
    relationName: "recurringInstances_transactionId_transactions_id",
  }),
  transactionTags: many(transactionTags),
}));

export const documentSplitsRelations = relations(documentSplits, ({ one }) => ({
  category: one(categories, {
    fields: [documentSplits.categoryId],
    references: [categories.id],
  }),
  sourceDocument: one(sourceDocuments, {
    fields: [documentSplits.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
}));

export const draftRecordsRelations = relations(draftRecords, ({ one }) => ({
  draftBatch: one(draftBatches, {
    fields: [draftRecords.batchId],
    references: [draftBatches.id],
  }),
  extractionResult: one(extractionResults, {
    fields: [draftRecords.extractionResultId],
    references: [extractionResults.id],
  }),
  financialObligation: one(financialObligations, {
    fields: [draftRecords.obligationId],
    references: [financialObligations.id],
  }),
  recurringTemplate: one(recurringTemplates, {
    fields: [draftRecords.reconciledTemplateId],
    references: [recurringTemplates.id],
  }),
  transaction: one(transactions, {
    fields: [draftRecords.reconciledTransactionId],
    references: [transactions.id],
  }),
  sourceDocument: one(sourceDocuments, {
    fields: [draftRecords.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
}));

export const draftBatchesRelations = relations(draftBatches, ({ one, many }) => ({
  draftRecords: many(draftRecords),
  ingestionRun: one(ingestionRuns, {
    fields: [draftBatches.runId],
    references: [ingestionRuns.id],
  }),
  sourceDocument: one(sourceDocuments, {
    fields: [draftBatches.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
}));

export const extractionResultsRelations = relations(extractionResults, ({ one, many }) => ({
  draftRecords: many(draftRecords),
  parsedDocumentVersion: one(parsedDocumentVersions, {
    fields: [extractionResults.parsedVersionId],
    references: [parsedDocumentVersions.id],
  }),
  supplier: one(suppliers, {
    fields: [extractionResults.supplierId],
    references: [suppliers.id],
  }),
}));

export const financialObligationsRelations = relations(financialObligations, ({ one, many }) => ({
  draftRecords: many(draftRecords),
  financialObligationIdentityKeys: many(financialObligationIdentityKeys),
  financialProduct: one(financialProducts, {
    fields: [financialObligations.financialProductId],
    references: [financialProducts.id],
  }),
  supplier: one(suppliers, {
    fields: [financialObligations.supplierId],
    references: [suppliers.id],
  }),
  financialObligationEvidences: many(financialObligationEvidences),
}));

export const recurringTemplatesRelations = relations(recurringTemplates, ({ one, many }) => ({
  draftRecords: many(draftRecords),
  category: one(categories, {
    fields: [recurringTemplates.categoryId],
    references: [categories.id],
  }),
  financialProduct: one(financialProducts, {
    fields: [recurringTemplates.financialProductId],
    references: [financialProducts.id],
  }),
  supplier: one(suppliers, {
    fields: [recurringTemplates.supplierId],
    references: [suppliers.id],
  }),
  recurringInstances: many(recurringInstances),
  recurringTemplateTags: many(recurringTemplateTags),
}));

export const aiChatMessagesRelations = relations(aiChatMessages, ({ one }) => ({
  aiChatSession: one(aiChatSessions, {
    fields: [aiChatMessages.sessionId],
    references: [aiChatSessions.id],
  }),
}));

export const aiChatSessionsRelations = relations(aiChatSessions, ({ many }) => ({
  aiChatMessages: many(aiChatMessages),
}));

export const documentFingerprintsRelations = relations(documentFingerprints, ({ one }) => ({
  sourceDocument: one(sourceDocuments, {
    fields: [documentFingerprints.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
}));

export const documentTransactionsRelations = relations(documentTransactions, ({ one }) => ({
  sourceDocument: one(sourceDocuments, {
    fields: [documentTransactions.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
  transaction: one(transactions, {
    fields: [documentTransactions.transactionId],
    references: [transactions.id],
  }),
}));

export const ingestionRunsRelations = relations(ingestionRuns, ({ many }) => ({
  draftBatches: many(draftBatches),
  ingestionJobs: many(ingestionJobs),
  ingestionLogs: many(ingestionLogs),
}));

export const externalAccountMappingsRelations = relations(externalAccountMappings, ({ one }) => ({
  financialProduct: one(financialProducts, {
    fields: [externalAccountMappings.financialProductId],
    references: [financialProducts.id],
  }),
  providerConnection: one(providerConnections, {
    fields: [externalAccountMappings.providerConnectionId],
    references: [providerConnections.id],
  }),
}));

export const providerConnectionsRelations = relations(providerConnections, ({ many }) => ({
  externalAccountMappings: many(externalAccountMappings),
}));

export const financialObligationIdentityKeysRelations = relations(
  financialObligationIdentityKeys,
  ({ one }) => ({
    financialObligation: one(financialObligations, {
      fields: [financialObligationIdentityKeys.obligationId],
      references: [financialObligations.id],
    }),
  }),
);

export const ingestionJobsRelations = relations(ingestionJobs, ({ one, many }) => ({
  ingestionRun: one(ingestionRuns, {
    fields: [ingestionJobs.runId],
    references: [ingestionRuns.id],
  }),
  sourceDocument: one(sourceDocuments, {
    fields: [ingestionJobs.sourceDocumentId],
    references: [sourceDocuments.id],
  }),
  ingestionLogs: many(ingestionLogs),
}));

export const ingestionLogsRelations = relations(ingestionLogs, ({ one }) => ({
  ingestionJob: one(ingestionJobs, {
    fields: [ingestionLogs.jobId],
    references: [ingestionJobs.id],
  }),
  ingestionRun: one(ingestionRuns, {
    fields: [ingestionLogs.runId],
    references: [ingestionRuns.id],
  }),
}));

export const parsedDocumentVersionsRelations = relations(
  parsedDocumentVersions,
  ({ one, many }) => ({
    extractionResults: many(extractionResults),
    sourceDocument: one(sourceDocuments, {
      fields: [parsedDocumentVersions.sourceDocumentId],
      references: [sourceDocuments.id],
    }),
  }),
);

export const liabilitiesRelations = relations(liabilities, ({ one, many }) => ({
  financialProduct: one(financialProducts, {
    fields: [liabilities.financialProductId],
    references: [financialProducts.id],
  }),
  supplier: one(suppliers, {
    fields: [liabilities.supplierId],
    references: [suppliers.id],
  }),
  liabilityInstallments: many(liabilityInstallments),
  liabilityTags: many(liabilityTags),
}));

export const financialObligationEvidencesRelations = relations(
  financialObligationEvidences,
  ({ one }) => ({
    financialObligation: one(financialObligations, {
      fields: [financialObligationEvidences.obligationId],
      references: [financialObligations.id],
    }),
    sourceDocument: one(sourceDocuments, {
      fields: [financialObligationEvidences.sourceDocumentId],
      references: [sourceDocuments.id],
    }),
  }),
);

export const supplierAliasesRelations = relations(supplierAliases, ({ one }) => ({
  supplier: one(suppliers, {
    fields: [supplierAliases.supplierId],
    references: [suppliers.id],
  }),
}));

export const statementCyclesRelations = relations(statementCycles, ({ one, many }) => ({
  card: one(cards, {
    fields: [statementCycles.cardId],
    references: [cards.id],
  }),
  statementItems: many(statementItems),
  transactions: many(transactions),
}));

export const statementItemsRelations = relations(statementItems, ({ one }) => ({
  statementCycle: one(statementCycles, {
    fields: [statementItems.statementCycleId],
    references: [statementCycles.id],
  }),
  supplier: one(suppliers, {
    fields: [statementItems.supplierId],
    references: [suppliers.id],
  }),
  transaction: one(transactions, {
    fields: [statementItems.transactionId],
    references: [transactions.id],
  }),
}));

export const liabilityInstallmentsRelations = relations(liabilityInstallments, ({ one, many }) => ({
  liability: one(liabilities, {
    fields: [liabilityInstallments.liabilityId],
    references: [liabilities.id],
  }),
  transactions: many(transactions),
}));

export const financialPeriodsRelations = relations(financialPeriods, ({ many }) => ({
  transactions: many(transactions),
  transfers: many(transfers),
}));

export const recurringInstancesRelations = relations(recurringInstances, ({ one, many }) => ({
  transactions: many(transactions, {
    relationName: "transactions_recurringInstanceId_recurringInstances_id",
  }),
  recurringTemplate: one(recurringTemplates, {
    fields: [recurringInstances.recurringTemplateId],
    references: [recurringTemplates.id],
  }),
  transaction: one(transactions, {
    fields: [recurringInstances.transactionId],
    references: [transactions.id],
    relationName: "recurringInstances_transactionId_transactions_id",
  }),
}));

export const supplierTagsRelations = relations(supplierTags, ({ one }) => ({
  supplier: one(suppliers, {
    fields: [supplierTags.supplierId],
    references: [suppliers.id],
  }),
  tag: one(tags, {
    fields: [supplierTags.tagId],
    references: [tags.id],
  }),
}));

export const tagsRelations = relations(tags, ({ many }) => ({
  supplierTags: many(supplierTags),
  liabilityTags: many(liabilityTags),
  recurringTemplateTags: many(recurringTemplateTags),
  transactionTags: many(transactionTags),
}));

export const transfersRelations = relations(transfers, ({ one }) => ({
  financialPeriod: one(financialPeriods, {
    fields: [transfers.financialPeriodId],
    references: [financialPeriods.id],
  }),
  financialProduct_sourceProductId: one(financialProducts, {
    fields: [transfers.sourceProductId],
    references: [financialProducts.id],
    relationName: "transfers_sourceProductId_financialProducts_id",
  }),
  financialProduct_targetProductId: one(financialProducts, {
    fields: [transfers.targetProductId],
    references: [financialProducts.id],
    relationName: "transfers_targetProductId_financialProducts_id",
  }),
}));

export const liabilityTagsRelations = relations(liabilityTags, ({ one }) => ({
  liability: one(liabilities, {
    fields: [liabilityTags.liabilityId],
    references: [liabilities.id],
  }),
  tag: one(tags, {
    fields: [liabilityTags.tagId],
    references: [tags.id],
  }),
}));

export const recurringTemplateTagsRelations = relations(recurringTemplateTags, ({ one }) => ({
  recurringTemplate: one(recurringTemplates, {
    fields: [recurringTemplateTags.recurringTemplateId],
    references: [recurringTemplates.id],
  }),
  tag: one(tags, {
    fields: [recurringTemplateTags.tagId],
    references: [tags.id],
  }),
}));

export const transactionTagsRelations = relations(transactionTags, ({ one }) => ({
  tag: one(tags, {
    fields: [transactionTags.tagId],
    references: [tags.id],
  }),
  transaction: one(transactions, {
    fields: [transactionTags.transactionId],
    references: [transactions.id],
  }),
}));
