/**
 * Testes unitários: hash e fingerprint (Phase 2D — item 2.24)
 */
import { describe, it, expect } from "vitest";
import { computeContentHash, computeCanonicalFingerprint } from "@sbf/operations";

describe("computeContentHash", () => {
  it("retorna hex SHA-256 de 64 caracteres", async () => {
    const data = new TextEncoder().encode("hello world");
    const hash = await computeContentHash(data);

    expect(hash).toHaveLength(64);
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
  });

  it("mesmo conteúdo → mesmo hash", async () => {
    const data = new TextEncoder().encode("documento financeiro CEMIG");
    const hash1 = await computeContentHash(data);
    const hash2 = await computeContentHash(data);

    expect(hash1).toBe(hash2);
  });

  it("conteúdo diferente → hash diferente", async () => {
    const data1 = new TextEncoder().encode("conta de luz janeiro");
    const data2 = new TextEncoder().encode("conta de luz fevereiro");
    const hash1 = await computeContentHash(data1);
    const hash2 = await computeContentHash(data2);

    expect(hash1).not.toBe(hash2);
  });

  it("aceita Uint8Array", async () => {
    const data = new Uint8Array([0x50, 0x44, 0x46]); // "PDF"
    const hash = await computeContentHash(data);

    expect(hash).toHaveLength(64);
  });

  it("aceita ArrayBuffer", async () => {
    const data = new TextEncoder().encode("test").buffer;
    const hash = await computeContentHash(data);

    expect(hash).toHaveLength(64);
  });

  it("bytes iguais em Uint8Array e ArrayBuffer → mesmo hash", async () => {
    const encoder = new TextEncoder();
    const bytes = encoder.encode("iguais");
    const h1 = await computeContentHash(bytes);
    const h2 = await computeContentHash(bytes.buffer);

    expect(h1).toBe(h2);
  });
});

describe("computeCanonicalFingerprint", () => {
  it("retorna hex SHA-256 de 64 caracteres", async () => {
    const hash = await computeCanonicalFingerprint("texto qualquer");

    expect(hash).toHaveLength(64);
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
  });

  it("normaliza espaços múltiplos", async () => {
    const h1 = await computeCanonicalFingerprint("conta  de   luz");
    const h2 = await computeCanonicalFingerprint("conta de luz");

    expect(h1).toBe(h2);
  });

  it("normaliza case (lowercase)", async () => {
    const h1 = await computeCanonicalFingerprint("CONTA DE LUZ");
    const h2 = await computeCanonicalFingerprint("conta de luz");

    expect(h1).toBe(h2);
  });

  it("normaliza tabs e newlines", async () => {
    const h1 = await computeCanonicalFingerprint("linha1\n\nlinha2\tlinha3");
    const h2 = await computeCanonicalFingerprint("linha1 linha2 linha3");

    expect(h1).toBe(h2);
  });

  it("remove espaços leading/trailing", async () => {
    const h1 = await computeCanonicalFingerprint("  conta de luz  ");
    const h2 = await computeCanonicalFingerprint("conta de luz");

    expect(h1).toBe(h2);
  });

  it("textos diferentes → fingerprints diferentes", async () => {
    const h1 = await computeCanonicalFingerprint("CEMIG janeiro 2026");
    const h2 = await computeCanonicalFingerprint("COPASA fevereiro 2026");

    expect(h1).not.toBe(h2);
  });
});
