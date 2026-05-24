/**
 * Testes unitários: política de idempotência (Phase 2D — item 2.25)
 */
import { describe, it, expect } from "vitest";
import { checkIdempotency } from "@sbf/operations";

describe("checkIdempotency", () => {
  const baseCheck = {
    contentHash: "abc123hash",
    canonicalFingerprint: "fp456",
    originKey: "local:/docs/conta.pdf:1234567890",
  };

  describe("documento novo (sem duplicatas)", () => {
    it("retorna proceed quando nada existe", () => {
      const result = checkIdempotency(baseCheck, {});

      expect(result.action).toBe("proceed");
      expect(result.existingDocumentId).toBeUndefined();
    });
  });

  describe("duplicata exata por content_hash", () => {
    it("rejeita quando content_hash já existe", () => {
      const result = checkIdempotency(baseCheck, {
        byContentHash: { sourceDocumentId: "doc-1" },
      });

      expect(result.action).toBe("reject_exact");
      expect(result.existingDocumentId).toBe("doc-1");
    });
  });

  describe("duplicata semântica por canonical_fingerprint", () => {
    it("alerta quando fingerprint existe mas hash é novo", () => {
      const result = checkIdempotency(baseCheck, {
        byCanonicalFingerprint: { sourceDocumentId: "doc-2" },
      });

      expect(result.action).toBe("alert_semantic");
      expect(result.existingDocumentId).toBe("doc-2");
    });

    it("ignora fingerprint check se canonicalFingerprint não foi fornecido", () => {
      const result = checkIdempotency(
        { ...baseCheck, canonicalFingerprint: undefined },
        { byCanonicalFingerprint: { sourceDocumentId: "doc-2" } },
      );

      expect(result.action).toBe("proceed");
    });
  });

  describe("origin_key existente", () => {
    it("update_origin quando origin_key existe com hash diferente", () => {
      const result = checkIdempotency(baseCheck, {
        byOriginKey: { sourceDocumentId: "doc-3", contentHash: "hash-antigo" },
      });

      expect(result.action).toBe("update_origin");
      expect(result.existingDocumentId).toBe("doc-3");
    });

    it("reject_exact quando origin_key existe com mesmo hash", () => {
      const result = checkIdempotency(baseCheck, {
        byOriginKey: { sourceDocumentId: "doc-3", contentHash: "abc123hash" },
      });

      expect(result.action).toBe("reject_exact");
      expect(result.existingDocumentId).toBe("doc-3");
    });
  });

  describe("force_reprocess", () => {
    it("permite reprocessamento forçado quando origin_key existe", () => {
      const result = checkIdempotency(
        { ...baseCheck, forceReprocess: true },
        { byOriginKey: { sourceDocumentId: "doc-4", contentHash: "abc123hash" } },
      );

      expect(result.action).toBe("proceed");
      expect(result.existingDocumentId).toBe("doc-4");
      expect(result.reason).toBe("force_reprocess");
    });

    it("não ignora content_hash check sem origin_key", () => {
      const result = checkIdempotency(
        { ...baseCheck, forceReprocess: true },
        { byContentHash: { sourceDocumentId: "doc-5" } },
      );

      expect(result.action).toBe("reject_exact");
    });
  });

  describe("precedência de checks", () => {
    it("content_hash tem precedência sobre fingerprint", () => {
      const result = checkIdempotency(baseCheck, {
        byContentHash: { sourceDocumentId: "doc-1" },
        byCanonicalFingerprint: { sourceDocumentId: "doc-2" },
      });

      expect(result.action).toBe("reject_exact");
      expect(result.existingDocumentId).toBe("doc-1");
    });

    it("force_reprocess + origin_key tem precedência sobre content_hash", () => {
      const result = checkIdempotency(
        { ...baseCheck, forceReprocess: true },
        {
          byContentHash: { sourceDocumentId: "doc-1" },
          byOriginKey: { sourceDocumentId: "doc-3", contentHash: "abc123hash" },
        },
      );

      expect(result.action).toBe("proceed");
      expect(result.existingDocumentId).toBe("doc-3");
    });
  });
});
