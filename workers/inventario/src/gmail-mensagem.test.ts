import { describe, expect, it } from "vitest";
import { anexosDe, cabecalho, consultaFinanceira, erroTransitorio } from "./gmail-mensagem";

const payload = {
  mimeType: "multipart/mixed",
  headers: [
    { name: "From", value: "Banco <fatura@banco.com.br>" },
    { name: "Subject", value: "Sua fatura chegou" },
  ],
  body: { size: 0 },
  parts: [
    {
      partId: "0",
      mimeType: "text/html",
      filename: "",
      headers: [],
      body: { size: 10, data: "PGI-" },
    },
    {
      partId: "1",
      mimeType: "multipart/mixed",
      filename: "",
      headers: [],
      body: { size: 0 },
      parts: [
        {
          partId: "1.0",
          mimeType: "application/pdf",
          filename: "fatura.pdf",
          headers: [],
          body: { size: 900, attachmentId: "att-1" },
        },
        {
          partId: "1.1",
          mimeType: "text/xml",
          filename: "nfe.xml",
          headers: [],
          body: { size: 20, data: "PHg-" },
        },
      ],
    },
  ],
};

describe("anexosDe", () => {
  it("acha anexos em qualquer profundidade, por id ou embutidos, e ignora o corpo", () => {
    expect(anexosDe(payload)).toEqual([
      {
        filename: "fatura.pdf",
        mimeType: "application/pdf",
        attachmentId: "att-1",
        data: undefined,
      },
      { filename: "nfe.xml", mimeType: "text/xml", attachmentId: undefined, data: "PHg-" },
    ]);
  });
});

describe("cabecalho", () => {
  it("ignora caixa no nome e devolve null se faltar", () => {
    expect(cabecalho(payload, "subject")).toBe("Sua fatura chegou");
    expect(cabecalho(payload, "Date")).toBeNull();
  });
});

describe("consultaFinanceira", () => {
  it("tira listas e promoções dos termos, mas aceita anexo de documento de qualquer categoria", () => {
    const q = consultaFinanceira();
    expect(q).toMatch(
      /^\(\{.*contracheque.*\} -category:promotions -category:forums -category:social\) OR \(has:attachment filename:\(pdf/,
    );
  });
});

describe("erroTransitorio", () => {
  it("trata cota estourada (403) e 429/5xx como espera, e o resto como erro de verdade", () => {
    expect(
      erroTransitorio("Gmail API error (403) on /messages: Quota exceeded for quota metric"),
    ).toBe(true);
    expect(erroTransitorio("Gmail API error (429) on /messages: x")).toBe(true);
    expect(erroTransitorio("Gmail API error (503) on /messages: x")).toBe(true);
    expect(erroTransitorio("Gmail API error (403) on /messages: Insufficient Permission")).toBe(
      false,
    );
    expect(erroTransitorio("Failed to refresh Gmail access token (HTTP 400)")).toBe(false);
  });
});
