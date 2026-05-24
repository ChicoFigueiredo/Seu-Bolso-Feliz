/**
 * EvidenceEnvelope — estrutura unificada para persistir evidência financeira
 * extraída de qualquer fonte (email, arquivo, upload).
 * Armazenada em source_documents.metadata e/ou parsed_document_versions.structured_data.
 */

export type FinancialIntent =
  | "bill_to_pay"
  | "bill_reminder"
  | "invoice_statement"
  | "bank_statement"
  | "payment_receipt"
  | "payment_confirmation"
  | "transaction_history"
  | "contract_or_debt"
  | "recurring_charge"
  | "unknown";

export type FieldCandidateSource =
  | "gmail_subject"
  | "gmail_body"
  | "attachment_text"
  | "pdf_native"
  | "ocr"
  | "boleto_utils"
  | "regex"
  | "supplier_template"
  | "ai_lite"
  | "ai_full"
  | "manual";

export interface FieldCandidate {
  value: unknown;
  confidence: number;
  source: FieldCandidateSource;
  reason?: string;
}

export interface ReconciliationCandidate {
  targetType:
    | "transaction"
    | "statement_cycle"
    | "statement_item"
    | "liability"
    | "liability_installment"
    | "recurring_template"
    | "recurring_instance"
    | "draft_record"
    | "financial_obligation";
  targetId: string;
  score: number;
  reasons: string[];
  blockingConflicts: string[];
}

export interface EvidenceEnvelope {
  schemaVersion: "1.0";
  userId: string;
  sourceDocumentId: string;
  originType: "gmail" | "local_file" | "manual_upload";
  originKey: string;

  email?: {
    messageId?: string;
    threadId?: string;
    from?: string;
    to?: string;
    subject?: string;
    date?: string;
    labels?: string[];
    bodyText?: string;
  };

  file?: {
    filename: string;
    mimeType?: string;
    sizeBytes?: number;
    storagePath?: string;
    contentHash?: string;
  };

  extractedText?: {
    textHash?: string;
    textLength: number;
    extractionMethod: string;
    ocrApplied: boolean;
  };

  fieldCandidates: {
    supplierName?: FieldCandidate[];
    supplierCnpj?: FieldCandidate[];
    totalAmount?: FieldCandidate[];
    dueDate?: FieldCandidate[];
    paymentDate?: FieldCandidate[];
    competenceDate?: FieldCandidate[];
    documentNumber?: FieldCandidate[];
    barcodeDigitableLine?: FieldCandidate[];
    cardLast4?: FieldCandidate[];
    financialProductHint?: FieldCandidate[];
  };

  financialInference?: {
    intent: FinancialIntent;
    confidence: number;
    reasons: string[];
    missingFields: string[];
  };

  identity?: {
    financialIdentityKey?: string;
    contentHash?: string;
    canonicalFingerprint?: string;
    textHash?: string;
  };

  reconciliation?: {
    status: string;
    candidates: ReconciliationCandidate[];
  };
}
