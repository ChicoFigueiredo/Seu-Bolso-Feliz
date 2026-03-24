/**
 * Testes unitários: máquina de estados do job de ingestão (Phase 2D — item 2.26)
 */
import { describe, it, expect } from "vitest";
import { isValidTransition, getValidNextStates } from "../../workers/ingestion/src/state-machine";
import { IngestionJobStatus } from "@sbf/ingestion-types";

const S = IngestionJobStatus;

describe("isValidTransition", () => {
  describe("transições do caminho feliz", () => {
    const happyPath: [string, string][] = [
      [S.DISCOVERED, S.DOWNLOADED],
      [S.DOWNLOADED, S.HASHED],
      [S.HASHED, S.QUEUED],
      [S.QUEUED, S.PARSING],
      [S.PARSING, S.PARSED],
      [S.PARSED, S.CLASSIFIED],
      [S.CLASSIFIED, S.RECONCILED],
      [S.RECONCILED, S.DRAFTED],
      [S.DRAFTED, S.PENDING_REVIEW],
      [S.PENDING_REVIEW, S.APPROVED],
      [S.APPROVED, S.POSTED],
    ];

    it.each(happyPath)("%s → %s é válida", (from, to) => {
      expect(isValidTransition(from as any, to as any)).toBe(true);
    });
  });

  describe("qualquer estado pode ir para FAILED", () => {
    const allButPosted = [
      S.DISCOVERED, S.DOWNLOADED, S.HASHED, S.QUEUED,
      S.PARSING, S.PARSED, S.CLASSIFIED, S.RECONCILED,
      S.DRAFTED, S.PENDING_REVIEW, S.APPROVED,
    ];

    it.each(allButPosted)("%s → FAILED é válida", (from) => {
      expect(isValidTransition(from as any, S.FAILED)).toBe(true);
    });
  });

  describe("POSTED é estado terminal", () => {
    const allStates = Object.values(S);

    it.each(allStates)("POSTED → %s é inválida", (to) => {
      expect(isValidTransition(S.POSTED, to as any)).toBe(false);
    });
  });

  describe("transições inválidas (pular etapas)", () => {
    const invalid: [string, string][] = [
      [S.DISCOVERED, S.PARSED],
      [S.DISCOVERED, S.POSTED],
      [S.DOWNLOADED, S.QUEUED],
      [S.HASHED, S.PARSED],
      [S.QUEUED, S.CLASSIFIED],
      [S.PARSING, S.POSTED],
    ];

    it.each(invalid)("%s → %s é inválida", (from, to) => {
      expect(isValidTransition(from as any, to as any)).toBe(false);
    });
  });

  describe("FAILED pode ser retomado", () => {
    it("FAILED → DISCOVERED é válida (retry)", () => {
      expect(isValidTransition(S.FAILED, S.DISCOVERED)).toBe(true);
    });

    it("FAILED → QUEUED é válida (retry direto)", () => {
      expect(isValidTransition(S.FAILED, S.QUEUED)).toBe(true);
    });

    it("FAILED → POSTED é inválida", () => {
      expect(isValidTransition(S.FAILED, S.POSTED)).toBe(false);
    });
  });
});

describe("getValidNextStates", () => {
  it("DISCOVERED pode ir para DOWNLOADED ou FAILED", () => {
    const next = getValidNextStates(S.DISCOVERED);
    expect(next).toContain(S.DOWNLOADED);
    expect(next).toContain(S.FAILED);
    expect(next).toHaveLength(2);
  });

  it("POSTED não tem próximos estados", () => {
    const next = getValidNextStates(S.POSTED);
    expect(next).toHaveLength(0);
  });

  it("FAILED pode voltar a DISCOVERED ou QUEUED", () => {
    const next = getValidNextStates(S.FAILED);
    expect(next).toContain(S.DISCOVERED);
    expect(next).toContain(S.QUEUED);
    expect(next).toHaveLength(2);
  });
});
