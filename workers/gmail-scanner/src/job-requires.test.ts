import { describe, it, expect } from "vitest";
import {
  CAPACIDADE_OCR,
  resolveAttachmentRequires,
  resolveEmailBodyRequires,
} from "./job-requires";

describe("resolveAttachmentRequires", () => {
  it.each([
    ["boleto.pdf", "application/pdf"],
    ["comprovante.png", "image/png"],
    ["foto.jpg", "image/jpeg"],
    ["recibo.webp", "image/webp"],
  ])("exige OCR para %s (%s)", (filename, mimeType) => {
    expect(resolveAttachmentRequires({ filename, mimeType })).toEqual([CAPACIDADE_OCR]);
  });

  it.each([
    ["extrato.csv", "text/csv"],
    ["planilha.xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
    ["extrato.ofx", "application/x-ofx"],
    ["nota.xml", "application/xml"],
  ])("não exige nada para %s (%s)", (filename, mimeType) => {
    expect(resolveAttachmentRequires({ filename, mimeType })).toEqual([]);
  });

  it("reconhece o PDF pela extensão quando o Gmail serve como octet-stream", () => {
    // Caso real: anexo entregue sem MIME útil. Sem o desempate por extensão,
    // um boleto escaneado iria para um worker sem OCR e pararia sem campo
    // nenhum — o modo de falha que o roteamento existe para evitar.
    expect(
      resolveAttachmentRequires({
        filename: "fatura.pdf",
        mimeType: "application/octet-stream",
      }),
    ).toEqual([CAPACIDADE_OCR]);
  });

  it("reconhece o PDF pelo MIME quando o nome não tem extensão", () => {
    expect(resolveAttachmentRequires({ filename: "anexo", mimeType: "application/pdf" })).toEqual([
      CAPACIDADE_OCR,
    ]);
  });

  it("ignora parâmetros e caixa no MIME declarado", () => {
    expect(
      resolveAttachmentRequires({ filename: "anexo", mimeType: "Image/PNG; name=foo.png" }),
    ).toEqual([CAPACIDADE_OCR]);
  });

  it("não exige nada quando nome e MIME não dizem nada", () => {
    expect(resolveAttachmentRequires({ filename: "anexo", mimeType: "" })).toEqual([]);
  });

  it("devolve um array novo a cada chamada", () => {
    // O chamador entrega isto ao `enqueueJob`; um array compartilhado entre
    // jobs viraria um bug silencioso se alguém o mutasse.
    const a = resolveAttachmentRequires({ filename: "a.pdf", mimeType: "application/pdf" });
    const b = resolveAttachmentRequires({ filename: "b.pdf", mimeType: "application/pdf" });
    expect(a).not.toBe(b);
  });
});

describe("resolveEmailBodyRequires", () => {
  it("não exige capacidade nenhuma — corpo de e-mail é texto puro", () => {
    expect(resolveEmailBodyRequires()).toEqual([]);
  });
});
