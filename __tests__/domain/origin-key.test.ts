/**
 * Testes unitários: buildOriginKey (Phase 2D — complemento 2.24)
 */
import { describe, it, expect } from "vitest";
import { buildOriginKey } from "@sbf/operations";

describe("buildOriginKey", () => {
  it("gmail: gera chave com messageId e contentHash", () => {
    const key = buildOriginKey({
      type: "gmail",
      messageId: "msg-abc123",
      contentHash: "a1b2c3d4e5f6",
    });

    expect(key).toBe("gmail:msg-abc123:a1b2c3d4e5f6");
  });

  it("local_file: gera chave com filepath e mtimeMs", () => {
    const key = buildOriginKey({
      type: "local_file",
      filepath: "/docs/conta-luz.pdf",
      mtimeMs: 1711234567890,
    });

    expect(key).toBe("local:/docs/conta-luz.pdf:1711234567890");
  });

  it("manual_upload: gera chave com filename e uploadedAt", () => {
    const key = buildOriginKey({
      type: "manual_upload",
      filename: "fatura-nubank.pdf",
      uploadedAt: "2026-03-15T10:30:00Z",
    });

    expect(key).toBe("upload:fatura-nubank.pdf:2026-03-15T10:30:00Z");
  });

  it("origin_keys diferentes para origens diferentes do mesmo arquivo", () => {
    const gmailKey = buildOriginKey({
      type: "gmail",
      messageId: "msg-1",
      contentHash: "hash-1",
    });
    const localKey = buildOriginKey({
      type: "local_file",
      filepath: "/inbox/doc.pdf",
      mtimeMs: 1711234567890,
    });

    expect(gmailKey).not.toBe(localKey);
  });

  it("origin_keys diferentes para mesmo arquivo com mtime diferente", () => {
    const key1 = buildOriginKey({
      type: "local_file",
      filepath: "/docs/conta.pdf",
      mtimeMs: 1000,
    });
    const key2 = buildOriginKey({
      type: "local_file",
      filepath: "/docs/conta.pdf",
      mtimeMs: 2000,
    });

    expect(key1).not.toBe(key2);
  });
});
