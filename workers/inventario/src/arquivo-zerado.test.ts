import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { ehSoZeros } from "./arquivo-zerado";

const dir = mkdtempSync(join(tmpdir(), "zerado-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

function arquivo(nome: string, conteudo: Buffer): string {
  const p = join(dir, nome);
  writeFileSync(p, conteudo);
  return p;
}

describe("ehSoZeros", () => {
  it("reconhece o marcador do OneDrive (todo zero), inclusive maior que um bloco", async () => {
    expect(await ehSoZeros(arquivo("a.pdf", Buffer.alloc(10)))).toBe(true);
    expect(await ehSoZeros(arquivo("b.pdf", Buffer.alloc(200 * 1024)))).toBe(true);
  });

  it("recusa arquivo com qualquer byte não zero, mesmo no fim", async () => {
    expect(await ehSoZeros(arquivo("c.pdf", Buffer.from("%PDF-1.4")))).toBe(false);
    const fim = Buffer.alloc(200 * 1024);
    fim[fim.length - 1] = 1;
    expect(await ehSoZeros(arquivo("d.pdf", fim))).toBe(false);
  });

  it("arquivo vazio não é marcador", async () => {
    expect(await ehSoZeros(arquivo("e.pdf", Buffer.alloc(0)))).toBe(false);
  });
});
