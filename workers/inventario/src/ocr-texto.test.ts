import { describe, expect, it } from "vitest";
import { limparOcr, textoParaGravar, textoSignificativo } from "./ocr-texto";

describe("limparOcr", () => {
  it("normaliza espaços, controles e linhas vazias", () => {
    expect(limparOcr("  Pix   enviado \r\n\n\n\n valor\t\t10,00\u000c")).toBe(
      "Pix enviado\n\nvalor 10,00",
    );
  });
});

describe("textoSignificativo", () => {
  it("rejeita ruído e aceita frase", () => {
    expect(textoSignificativo("| . ~ 12 ;;")).toBe(false);
    expect(textoSignificativo("Comprovante de transferência Pix")).toBe(true);
  });
});

describe("textoParaGravar", () => {
  it("devolve vazio para ruído e corta texto longo", () => {
    expect(textoParaGravar("..  ,, 1 2")).toBe("");
    expect(textoParaGravar("palavra ".repeat(5000)).length).toBeLessThanOrEqual(20 * 1024);
  });
});
