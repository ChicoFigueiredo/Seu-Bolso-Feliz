/**
 * @sbf/worker-gmail-scanner — Message processor
 *
 * Extrai metadados de mensagens do Gmail (subject, from, date, anexos)
 * e gera fingerprint para idempotência.
 */

import type { GmailMessage, GmailPart } from "./gmail-client";

/** Tipos de MIME aceitos para ingestão de anexos */
const ACCEPTED_MIME_TYPES = new Set([
  "application/pdf",
  "image/png",
  "image/jpeg",
  "image/webp",
  "text/csv",
  "application/vnd.ms-excel",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/xml",
  "application/octet-stream",
]);

/** Extensões aceitas (fallback se MIME não for confiável) */
const ACCEPTED_EXTENSIONS = new Set([
  ".pdf",
  ".png",
  ".jpg",
  ".jpeg",
  ".webp",
  ".csv",
  ".xls",
  ".xlsx",
  ".xml",
  ".ofx",
]);

export interface MessageMetadata {
  messageId: string;
  threadId: string;
  subject: string;
  from: string;
  to: string;
  date: string;
  internalDate: string;
  labelIds: string[];
}

export interface AttachmentInfo {
  partId: string;
  attachmentId: string;
  filename: string;
  mimeType: string;
  size: number;
}

export interface ProcessedMessage {
  metadata: MessageMetadata;
  attachments: AttachmentInfo[];
}

/**
 * Extrai valor de um header da mensagem.
 */
function getHeader(message: GmailMessage, name: string): string {
  const header = message.payload.headers.find((h) => h.name.toLowerCase() === name.toLowerCase());
  return header?.value ?? "";
}

/**
 * Verifica se um attachment tem tipo/extensão aceita para ingestão.
 */
function isAcceptedAttachment(part: GmailPart): boolean {
  if (!part.filename || part.filename.length === 0) return false;
  if (!part.body.attachmentId) return false;

  // Checar por MIME type
  if (ACCEPTED_MIME_TYPES.has(part.mimeType)) return true;

  // Fallback: checar extensão do filename
  const dotIndex = part.filename.lastIndexOf(".");
  if (dotIndex >= 0) {
    const ext = part.filename.substring(dotIndex).toLowerCase();
    if (ACCEPTED_EXTENSIONS.has(ext)) return true;
  }

  return false;
}

/**
 * Percorre recursivamente as parts de uma mensagem para encontrar anexos.
 */
function findAttachments(parts: GmailPart[] | undefined): AttachmentInfo[] {
  if (!parts) return [];

  const attachments: AttachmentInfo[] = [];

  for (const part of parts) {
    if (isAcceptedAttachment(part)) {
      attachments.push({
        partId: part.partId,
        attachmentId: part.body.attachmentId!,
        filename: part.filename,
        mimeType: part.mimeType,
        size: part.body.size,
      });
    }

    // Recursão para mensagens multipart aninhadas
    if (part.parts) {
      attachments.push(...findAttachments(part.parts));
    }
  }

  return attachments;
}

/**
 * Processa uma mensagem do Gmail: extrai metadados e lista de anexos aceitos.
 */
export function processMessage(message: GmailMessage): ProcessedMessage {
  const metadata: MessageMetadata = {
    messageId: message.id,
    threadId: message.threadId,
    subject: getHeader(message, "Subject"),
    from: getHeader(message, "From"),
    to: getHeader(message, "To"),
    date: getHeader(message, "Date"),
    internalDate: message.internalDate,
    labelIds: message.labelIds ?? [],
  };

  const attachments = findAttachments(message.payload.parts);

  return { metadata, attachments };
}

/**
 * Remove tags HTML de uma string, preservando espaços entre elementos.
 */
function stripHtml(html: string): string {
  return html
    .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(/<script[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&quot;/gi, '"')
    .replace(/\s{2,}/g, " ")
    .trim();
}

/**
 * Decodes base64url to UTF-8 string.
 */
function decodeBase64UrlText(data: string): string {
  const base64 = data.replace(/-/g, "+").replace(/_/g, "/");
  const binaryString = atob(base64);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
}

/**
 * Extrai texto do corpo de uma mensagem Gmail.
 * Prioridade: text/plain > text/html (com strip de tags).
 * Retorna string vazia se não houver corpo.
 */
export function extractBodyText(message: GmailMessage): string {
  // Recursively find parts by mimeType
  function findPart(parts: GmailPart[] | undefined, mime: string): GmailPart | null {
    if (!parts) return null;
    for (const part of parts) {
      if (part.mimeType === mime && !part.filename && part.body.data) return part;
      const nested = findPart(part.parts, mime);
      if (nested) return nested;
    }
    return null;
  }

  // Simple payload (no multipart)
  if (message.payload.body.data) {
    const text = decodeBase64UrlText(message.payload.body.data);
    if (message.payload.mimeType === "text/html") return stripHtml(text);
    return text;
  }

  const plainPart = findPart(message.payload.parts, "text/plain");
  if (plainPart?.body.data) {
    return decodeBase64UrlText(plainPart.body.data);
  }

  const htmlPart = findPart(message.payload.parts, "text/html");
  if (htmlPart?.body.data) {
    return stripHtml(decodeBase64UrlText(htmlPart.body.data));
  }

  return "";
}

/**
 * Decodifica base64url (formato Gmail) para ArrayBuffer.
 */
export function decodeBase64Url(base64url: string): ArrayBuffer {
  // Gmail usa base64url: substitui - por + e _ por /
  const base64 = base64url.replace(/-/g, "+").replace(/_/g, "/");
  const binaryString = atob(base64);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes.buffer;
}
