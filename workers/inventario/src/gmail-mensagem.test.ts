import { describe, expect, it } from "vitest";
import { anexosDe, cabecalho, consultaFinanceira } from "./gmail-mensagem";

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
  it("é um OR único de termos e anexos de documento", () => {
    const q = consultaFinanceira();
    expect(q.startsWith("{") && q.endsWith("}")).toBe(true);
    expect(q).toContain("contracheque");
    expect(q).toContain("has:attachment filename:(pdf");
  });
});
