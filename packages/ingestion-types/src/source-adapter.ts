/**
 * Interfaces comuns para adapters de fonte de evidência financeira.
 * Qualquer fonte (Gmail, pasta local, upload manual) implementa SourceAdapter.
 */

export type SourceKind =
  | "gmail_label"
  | "gmail_query"
  | "gmail_period"
  | "local_directory"
  | "manual_upload"
  | "chat_upload";

export interface SourceEvent {
  sourceKind: SourceKind;
  originType: "gmail" | "local_file" | "manual_upload";
  originKey: string;
  userId: string;
  occurredAt?: string;
  metadata: Record<string, unknown>;
}

export interface RawEvidenceBundle {
  event: SourceEvent;
  primaryText?: string;
  htmlText?: string;
  subject?: string;
  from?: string;
  to?: string;
  attachments: Array<{
    filename: string;
    mimeType: string;
    sizeBytes: number;
    content: Uint8Array;
    contentHash?: string;
  }>;
  rawPayload?: Record<string, unknown>;
}

export interface SourceAdapter {
  discover(): Promise<SourceEvent[]>;
  fetch(event: SourceEvent): Promise<RawEvidenceBundle>;
}
