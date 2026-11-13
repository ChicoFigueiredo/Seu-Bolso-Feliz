import { describe, expect, it } from "vitest";
import { dentroDaPastaDoSbf } from "./pasta-sbf";

describe("dentroDaPastaDoSbf", () => {
  it("reconhece a estante e a cópia de segurança", () => {
    expect(dentroDaPastaDoSbf("SBF/Estante/2026-10/faturas/a.pdf")).toBe(true);
    expect(dentroDaPastaDoSbf("SBF/Copia-de-seguranca/xyz")).toBe(true);
    expect(dentroDaPastaDoSbf("/SBF/Estante/a.pdf")).toBe(true);
    expect(dentroDaPastaDoSbf("SBF")).toBe(true);
  });
  it("não pega pastas parecidas nem SBF mais fundo", () => {
    expect(dentroDaPastaDoSbf("SBF-antigo/a.pdf")).toBe(false);
    expect(dentroDaPastaDoSbf("Documentos/SBF/a.pdf")).toBe(false);
    expect(dentroDaPastaDoSbf("sbf/a.pdf")).toBe(false);
  });
});
