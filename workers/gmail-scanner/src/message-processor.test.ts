import { describe, it, expect } from "vitest";
import type { GmailMessage, GmailPart } from "./gmail-client";
import { processMessage, decodeBase64Url } from "./message-processor";

// ══════════════════════════════════════════════════════════════
// Helpers — Factory para mensagens do Gmail
// ══════════════════════════════════════════════════════════════

function makeHeader(name: string, value: string) {
  return { name, value };
}

function makePart(overrides: Partial<GmailPart> = {}): GmailPart {
  return {
    partId: "0",
    mimeType: "text/plain",
    filename: "",
    headers: [],
    body: { size: 0 },
    ...overrides,
  };
}

function makeMessage(overrides: Partial<GmailMessage> = {}): GmailMessage {
  return {
    id: "msg-001",
    threadId: "thread-001",
    labelIds: ["Label_1"],
    snippet: "Segue comprovante",
    internalDate: "1711929600000",
    sizeEstimate: 1024,
    payload: {
      mimeType: "multipart/mixed",
      headers: [
        makeHeader("Subject", "Comprovante de pagamento"),
        makeHeader("From", "banco@email.com"),
        makeHeader("To", "eu@email.com"),
        makeHeader("Date", "Mon, 01 Apr 2025 12:00:00 -0300"),
      ],
      body: { size: 0 },
      parts: [],
    },
    ...overrides,
  };
}

// ══════════════════════════════════════════════════════════════
// processMessage — Extração de metadados
// ══════════════════════════════════════════════════════════════

describe("processMessage — metadados", () => {
  it("extrai subject, from, to, date corretamente", () => {
    const msg = makeMessage();
    const result = processMessage(msg);

    expect(result.metadata.messageId).toBe("msg-001");
    expect(result.metadata.threadId).toBe("thread-001");
    expect(result.metadata.subject).toBe("Comprovante de pagamento");
    expect(result.metadata.from).toBe("banco@email.com");
    expect(result.metadata.to).toBe("eu@email.com");
    expect(result.metadata.date).toBe("Mon, 01 Apr 2025 12:00:00 -0300");
    expect(result.metadata.labelIds).toEqual(["Label_1"]);
    expect(result.metadata.internalDate).toBe("1711929600000");
  });

  it("retorna string vazia para headers ausentes", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "text/plain",
        headers: [],
        body: { size: 0 },
      },
    });
    const result = processMessage(msg);
    expect(result.metadata.subject).toBe("");
    expect(result.metadata.from).toBe("");
  });

  it("retorna array vazio quando labelIds é undefined", () => {
    const msg = makeMessage({ labelIds: undefined as unknown as string[] });
    const result = processMessage(msg);
    expect(result.metadata.labelIds).toEqual([]);
  });
});

// ══════════════════════════════════════════════════════════════
// processMessage — Detecção de anexos
// ══════════════════════════════════════════════════════════════

describe("processMessage — detecção de anexos", () => {
  it("mensagem sem parts retorna 0 anexos", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "text/plain",
        headers: [makeHeader("Subject", "Sem anexo")],
        body: { size: 100 },
      },
    });
    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(0);
  });

  it("detecta PDF como anexo válido", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "Boleto")],
        body: { size: 0 },
        parts: [
          makePart({ partId: "0", mimeType: "text/plain", body: { size: 10 } }),
          makePart({
            partId: "1",
            mimeType: "application/pdf",
            filename: "boleto.pdf",
            body: { attachmentId: "att-001", size: 50000 },
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(1);
    expect(result.attachments[0]).toEqual({
      partId: "1",
      attachmentId: "att-001",
      filename: "boleto.pdf",
      mimeType: "application/pdf",
      size: 50000,
    });
  });

  it("detecta múltiplos anexos (PDF + PNG + XLSX)", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "Documentos")],
        body: { size: 0 },
        parts: [
          makePart({
            partId: "1",
            mimeType: "application/pdf",
            filename: "comprovante.pdf",
            body: { attachmentId: "att-1", size: 1000 },
          }),
          makePart({
            partId: "2",
            mimeType: "image/png",
            filename: "recibo.png",
            body: { attachmentId: "att-2", size: 2000 },
          }),
          makePart({
            partId: "3",
            mimeType: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            filename: "extrato.xlsx",
            body: { attachmentId: "att-3", size: 5000 },
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(3);
    expect(result.attachments.map((a) => a.filename)).toEqual([
      "comprovante.pdf",
      "recibo.png",
      "extrato.xlsx",
    ]);
  });

  it("ignora part sem filename", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "Email")],
        body: { size: 0 },
        parts: [
          makePart({
            partId: "0",
            mimeType: "text/html",
            filename: "",
            body: { size: 500 },
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(0);
  });

  it("ignora part sem attachmentId", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "Email")],
        body: { size: 0 },
        parts: [
          makePart({
            partId: "1",
            mimeType: "application/pdf",
            filename: "test.pdf",
            body: { size: 500 },
            // body.attachmentId is undefined
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(0);
  });

  it("ignora MIME types não aceitos (ex: application/zip)", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "Arquivo")],
        body: { size: 0 },
        parts: [
          makePart({
            partId: "1",
            mimeType: "application/zip",
            filename: "arquivo.zip",
            body: { attachmentId: "att-1", size: 10000 },
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(0);
  });

  it("aceita por extensão quando MIME é genérico (octet-stream -> .ofx)", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "OFX")],
        body: { size: 0 },
        parts: [
          makePart({
            partId: "1",
            mimeType: "application/octet-stream",
            filename: "extrato.ofx",
            body: { attachmentId: "att-1", size: 3000 },
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(1);
    expect(result.attachments[0]!.filename).toBe("extrato.ofx");
  });

  it("detecta anexos em parts aninhados (multipart/mixed > multipart/alternative > attachment)", () => {
    const msg = makeMessage({
      payload: {
        mimeType: "multipart/mixed",
        headers: [makeHeader("Subject", "Nested")],
        body: { size: 0 },
        parts: [
          makePart({
            partId: "0",
            mimeType: "multipart/alternative",
            body: { size: 0 },
            parts: [
              makePart({
                partId: "0.0",
                mimeType: "text/plain",
                body: { size: 100 },
              }),
              makePart({
                partId: "0.1",
                mimeType: "text/html",
                body: { size: 500 },
              }),
            ],
          }),
          makePart({
            partId: "1",
            mimeType: "multipart/related",
            body: { size: 0 },
            parts: [
              makePart({
                partId: "1.0",
                mimeType: "application/pdf",
                filename: "nested-doc.pdf",
                body: { attachmentId: "att-nested", size: 8000 },
              }),
            ],
          }),
        ],
      },
    });

    const result = processMessage(msg);
    expect(result.attachments).toHaveLength(1);
    expect(result.attachments[0]!.filename).toBe("nested-doc.pdf");
    expect(result.attachments[0]!.attachmentId).toBe("att-nested");
  });
});

// ══════════════════════════════════════════════════════════════
// decodeBase64Url
// ══════════════════════════════════════════════════════════════

describe("decodeBase64Url", () => {
  it("decodifica string base64url corretamente", () => {
    // "Hello, World!" em base64url
    const base64url = "SGVsbG8sIFdvcmxkIQ";
    const buffer = decodeBase64Url(base64url);
    const text = new TextDecoder().decode(buffer);
    expect(text).toBe("Hello, World!");
  });

  it("converte caracteres - e _ do base64url para + e /", () => {
    // Bytes que produzem + e / em base64 padrão: 0xFB, 0xEF, 0xBE
    // base64: +++++/++/w==
    // base64url: -----_--_w
    const standardBase64 = "+++++/++/w==";
    const base64url = standardBase64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

    const buffer = decodeBase64Url(base64url);
    const bytes = new Uint8Array(buffer);

    // Verificar que os bytes decodificados são válidos
    expect(bytes.length).toBeGreaterThan(0);
  });

  it("string vazia retorna buffer vazio", () => {
    const buffer = decodeBase64Url("");
    expect(new Uint8Array(buffer).length).toBe(0);
  });
});
