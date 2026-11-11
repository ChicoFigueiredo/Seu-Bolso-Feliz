import { describe, expect, it } from "vitest";
import {
  cnpjValido,
  escolherDataChave,
  extrairCampos,
  extrairLinhaDigitavel,
  inferirTipoDocumento,
  parsearEml,
  parsearNfe,
  parsearOfx,
  primeiraData,
  valorParaCentavos,
  vencimentoDoFator,
} from "./extrair";

const HOJE = new Date("2026-11-11T12:00:00Z");

// Linha de teste com valor R$ 1.449,19; o vencimento sai do fator 9865.
const LINHA = "00190.00009 01234.567890 12345.678901 1 98650000144919";

describe("datas e valores", () => {
  it("lê datas em vários formatos", () => {
    expect(primeiraData("Vencimento 05/03/2026")).toBe("2026-03-05");
    expect(primeiraData("em 5.3.26")).toBe("2026-03-05");
    expect(primeiraData("12 de março de 2026")).toBe("2026-03-12");
    expect(primeiraData("10 MAR 2026 às 14:32")).toBe("2026-03-10");
    expect(primeiraData("2026-03-12T10:00:00-03:00")).toBe("2026-03-12");
    expect(primeiraData("31/02/2026")).toBeNull();
    expect(primeiraData("00/00/0000")).toBeNull();
  });

  it("converte dinheiro brasileiro em centavos", () => {
    expect(valorParaCentavos("R$ 1.234,56")).toBe(123456);
    expect(valorParaCentavos("12,5")).toBe(1250);
    expect(valorParaCentavos("0,99")).toBe(99);
    expect(valorParaCentavos("1234.56")).toBe(123456);
    expect(valorParaCentavos("abc")).toBeNull();
  });
});

describe("linha digitável", () => {
  it("tira valor e vencimento do boleto bancário", () => {
    const l = extrairLinhaDigitavel(`Linha: ${LINHA}`, HOJE)!;
    expect(l.tipo).toBe("banco");
    expect(l.valorCentavos).toBe(144919);
    expect(l.linha).toHaveLength(47);
    expect(l.vencimento).toBe(vencimentoDoFator(9865, HOJE));
  });

  it("respeita a virada do fator em 22/02/2025", () => {
    expect(vencimentoDoFator(1000, HOJE)).toBe("2025-02-22");
    expect(vencimentoDoFator(9999, HOJE)).toBe("2025-02-21");
    expect(vencimentoDoFator(4000, HOJE)).toBe("2008-09-19");
  });

  it("lê arrecadação (concessionária) com valor efetivo", () => {
    const l = extrairLinhaDigitavel("898400000000 370001041584 200000020128 988100000003", HOJE)!;
    expect(l.tipo).toBe("concessionaria");
    expect(l.valorCentavos).toBe(3700);
  });
});

describe("documentos", () => {
  it("boleto", () => {
    const texto = `
BANCO DO BRASIL                                   Recibo do Pagador
Beneficiário                                         CNPJ
COOPERFORTE LTDA                                     27.652.684/0001-62
Pagador
FRANCISCO LIMA FIGUEIREDO
Vencimento            Valor do Documento
20/10/2026            R$ 1.449,19
${LINHA}
`;
    const c = extrairCampos(texto, HOJE);
    expect(c.tipoDoc).toBe("boleto");
    expect(c.fornecedor).toBe("COOPERFORTE LTDA");
    expect(c.cnpj).toBe("27652684000162");
    expect(c.vencimento).toBe("2026-10-20");
    expect(c.valorCentavos).toBe(144919);
    expect(escolherDataChave(c, c.tipoDoc)).toEqual({ data: "2026-10-20", tipo: "vencimento" });
  });

  it("comprovante de Pix: pagamento vence o resto", () => {
    const texto = `
COMPROVANTE DE PIX ENVIADO
Valor pago: R$ 50,00
Data do pagamento: 11/07/2023 14:32
Favorecido: DOUGLAS APARECIDO MENESES
CPF: ***.123.456-**
`;
    const c = extrairCampos(texto, HOJE);
    expect(c.tipoDoc).toBe("comprovante");
    expect(c.valorCentavos).toBe(5000);
    expect(c.fornecedor).toBe("DOUGLAS APARECIDO MENESES");
    expect(escolherDataChave({ ...c, vencimento: "2023-07-20" }, c.tipoDoc)).toEqual({
      data: "2023-07-11",
      tipo: "pagamento",
    });
  });

  it("fatura de cartão: data-chave é o vencimento, não os pagamentos listados", () => {
    const texto = `
Fatura do cartão Platinum
Vencimento            21/09/2026
Total da fatura       R$ 13.747,49
Pagamento mínimo      R$ 2.062,12
Pagamento realizado em 20/08/2026   -R$ 9.000,00
`;
    const c = extrairCampos(texto, HOJE);
    expect(c.tipoDoc).toBe("fatura");
    expect(c.valorCentavos).toBe(1374749);
    expect(escolherDataChave(c, "fatura")).toEqual({ data: "2026-09-21", tipo: "vencimento" });
  });

  it("contracheque: líquido e competência", () => {
    const texto = `
DEMONSTRATIVO DE PAGAMENTO
Mês / Ano de Pagamento
SETEMBRO / 2025
Total de proventos   10.000,00
Líquido a receber    7.512,30
`;
    const c = extrairCampos(texto, HOJE);
    expect(c.tipoDoc).toBe("contracheque");
    expect(c.valorCentavos).toBe(751230);
    expect(c.competencia).toBe("2025-09");
    expect(escolherDataChave(c, c.tipoDoc)).toEqual({ data: "2025-09-30", tipo: "emissao" });
  });

  it("valida CNPJ", () => {
    expect(cnpjValido("27.652.684/0001-62")).toBe(true);
    expect(cnpjValido("11.111.111/1111-11")).toBe(false);
  });

  it("cai no arquivo quando nada foi achado", () => {
    expect(escolherDataChave({ arquivo: "2024-02-15" }, null)).toEqual({
      data: "2024-02-15",
      tipo: "arquivo",
    });
    expect(escolherDataChave({}, null)).toBeNull();
  });

  it("não infere tipo de texto qualquer", () => {
    expect(inferirTipoDocumento("Prova de matemática, questão 1")).toBeNull();
  });
});

describe("NF-e, OFX e e-mail", () => {
  it("lê NF-e", () => {
    const xml = `<?xml version="1.0"?><nfeProc><NFe><infNFe Id="NFe123">
<ide><nNF>4521</nNF><dhEmi>2026-03-12T10:15:00-03:00</dhEmi></ide>
<emit><CNPJ>27652684000162</CNPJ><xNome>LOJA EXEMPLO LTDA</xNome></emit>
<total><ICMSTot><vNF>1234.56</vNF></ICMSTot></total></infNFe></NFe></nfeProc>`;
    expect(parsearNfe(xml)).toEqual({
      emitente: "LOJA EXEMPLO LTDA",
      cnpj: "27652684000162",
      emissao: "2026-03-12",
      valorCentavos: 123456,
      numero: "4521",
    });
    expect(parsearNfe("<html></html>")).toBeNull();
  });

  it("lê cabeçalho de OFX 1.x", () => {
    const ofx = `OFXHEADER:100
DATA:OFXSGML
<OFX><SIGNONMSGSRSV1><SONRS><FI><ORG>Nubank<FID>260</FI></SONRS></SIGNONMSGSRSV1>
<BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>
<DTSTART>20260201000000[-3:BRT]
<DTEND>20260228235959[-3:BRT]
<STMTTRN><TRNAMT>-10.00</STMTTRN><STMTTRN><TRNAMT>5.00</STMTTRN>`;
    expect(parsearOfx(ofx)).toEqual({
      banco: "Nubank",
      inicio: "2026-02-01",
      fim: "2026-02-28",
      transacoes: 2,
    });
  });

  it("lê e-mail de pedido (quoted-printable e assunto codificado)", () => {
    const eml = [
      "From: =?UTF-8?Q?Loja_Exemplo?= <pedidos@loja.example>",
      "Subject: =?UTF-8?B?U2V1IHBlZGlkbyBmb2kgY29uZmlybWFkbw==?=",
      "Date: Wed, 11 Mar 2026 21:30:00 -0300",
      "MIME-Version: 1.0",
      'Content-Type: multipart/alternative; boundary="XX"',
      "",
      "--XX",
      'Content-Type: text/html; charset="utf-8"',
      "Content-Transfer-Encoding: quoted-printable",
      "",
      "<html><body><p>N=C3=BAmero do pedido: 123</p><p>Total do pedido: R$ 89,90</p></body></html>",
      "--XX--",
    ].join("\r\n");
    const m = parsearEml(eml);
    expect(m.deNome).toBe("Loja Exemplo");
    expect(m.de).toBe("pedidos@loja.example");
    expect(m.assunto).toBe("Seu pedido foi confirmado");
    expect(m.data).toBe("2026-03-11");
    expect(m.corpo).toContain("Número do pedido: 123");
    const c = extrairCampos(`${m.assunto}\n${m.corpo}`, HOJE);
    expect(c.tipoDoc).toBe("pedido");
    expect(c.valorCentavos).toBe(8990);
  });
});
