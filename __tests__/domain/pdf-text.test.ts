/**
 * Extração de PDF sobre pdfjs-dist.
 *
 * O foco é a propriedade que os parsers determinísticos exigem e que a
 * concatenação ingênua de fragmentos destrói: LINHAS. `boleto-parser` e
 * `cemig-parser` casam expressões como `/Vencimento:\s*(\d{2}\/\d{2}\/\d{4})/`
 * contra linhas; se o texto vier como um parágrafo único, eles ainda casam por
 * sorte, mas templates de fornecedor que ancoram em início de linha não.
 */
import { describe, expect, it } from "vitest";
import { extrairTextoPdf, reconstruirLinhas } from "../../workers/ingestion/src/parsers/pdf-text";
import { boletoCemig } from "../fixtures/documents/boleto-cemig";

describe("reconstruirLinhas", () => {
  it("devolve vazio sem fragmentos", () => {
    expect(reconstruirLinhas([])).toBe("");
  });

  it("ordena de cima para baixo e da esquerda para a direita", () => {
    const texto = reconstruirLinhas([
      { x: 50, y: 700, texto: "segunda" },
      { x: 90, y: 780, texto: "linha" },
      { x: 50, y: 780, texto: "primeira " },
    ]);

    expect(texto).toBe("primeira linha\nsegunda");
  });

  it("agrupa fragmentos com diferença vertical mínima na mesma linha", () => {
    // Acento posicionado por conta própria: Y difere por fração de ponto.
    const texto = reconstruirLinhas([
      { x: 50, y: 780, texto: "Vencimento: " },
      { x: 120, y: 779.4, texto: "15/04/2026" },
    ]);

    expect(texto).toBe("Vencimento: 15/04/2026");
  });

  it("separa linhas visualmente distintas", () => {
    const texto = reconstruirLinhas([
      { x: 50, y: 780, texto: "Vencimento: 15/04/2026" },
      { x: 50, y: 765, texto: "Valor a pagar: R$ 245,50" },
    ]);

    expect(texto.split("\n")).toHaveLength(2);
  });
});

describe("extrairTextoPdf", () => {
  it("lê o boleto e preserva as linhas que os parsers procuram", async () => {
    const { text, pages } = await extrairTextoPdf(boletoCemig.bytes());

    expect(pages).toBe(1);
    const linhas = text.split("\n").map((l) => l.trim());

    expect(linhas).toContain("CEMIG DISTRIBUICAO S.A.");
    expect(linhas).toContain("Vencimento: 15/04/2026");
    expect(linhas).toContain("Valor a pagar: R$ 245,50");
    expect(linhas).toContain("Referencia: MARCO/2026");
  });

  it("não desanexa o buffer do chamador — o retry e o OCR dependem disso", async () => {
    const bytes = boletoCemig.bytes();
    await extrairTextoPdf(bytes);

    expect(bytes.length).toBe(1243);
    expect(bytes.subarray(0, 8).toString("latin1")).toBe("%PDF-1.4");
  });

  it("é determinístico: 20 extrações seguidas do mesmo PDF dão o mesmo texto", async () => {
    // Este é o teste de regressão do defeito que motivou a troca de biblioteca:
    // o pdf.js v1.10.100 do pdf-parse devolvia erros diferentes para os mesmos
    // bytes no mesmo processo.
    const primeiro = (await extrairTextoPdf(boletoCemig.bytes())).text;

    for (let i = 0; i < 19; i++) {
      const { text } = await extrairTextoPdf(boletoCemig.bytes());
      expect(text).toBe(primeiro);
    }
  }, 60_000);

  it("rejeita PDF inválido com erro, não com texto vazio", async () => {
    await expect(extrairTextoPdf(Buffer.from("isto não é um PDF"))).rejects.toThrow();
  });
});
