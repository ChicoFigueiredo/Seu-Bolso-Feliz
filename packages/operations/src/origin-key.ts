/**
 * Construção de origin_key para deduplicação por proveniência.
 * Vincula a proveniência sem depender de hash de conteúdo.
 */

export type OriginType = "gmail" | "local_file" | "manual_upload";

interface GmailOrigin {
  type: "gmail";
  messageId: string;
  contentHash: string;
}

interface LocalFileOrigin {
  type: "local_file";
  filepath: string;
  mtimeMs: number;
}

interface ManualUploadOrigin {
  type: "manual_upload";
  filename: string;
  uploadedAt: string;
}

export type OriginInput = GmailOrigin | LocalFileOrigin | ManualUploadOrigin;

/**
 * Gera chave de proveniência única para um documento.
 */
export function buildOriginKey(origin: OriginInput): string {
  switch (origin.type) {
    case "gmail":
      return `gmail:${origin.messageId}:${origin.contentHash}`;
    case "local_file":
      return `local:${origin.filepath}:${origin.mtimeMs}`;
    case "manual_upload":
      return `upload:${origin.filename}:${origin.uploadedAt}`;
  }
}
