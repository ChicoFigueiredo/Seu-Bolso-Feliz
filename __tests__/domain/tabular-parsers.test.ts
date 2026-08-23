/**
 * Parsers de formato tabular: CSV, OFX e XLSX.
 *
 * MOTIVO: a UI anunciava XLSX, OFX, DOC e QIF e o pipeline não lia nenhum
 * deles. XLSX caía num `Buffer.toString()` — e XLSX é um ZIP, então o
 * "texto extraído" era lixo binário que seguia pelo pipeline até virar um
 * `extraction_result` vazio, sem erro nenhum. CSV era lido como texto puro,
 * sem detectar delimitador nem codificação, o que quebrava qualquer extrato
 * brasileiro (`;` como separador, latin-1 nos acentos).
 */
import { describe, expect, it } from "vitest";
import ExcelJS from "exceljs";
import {
  detectarDelimitador,
  decodificarTexto,
  parseCsv,
  parseOfx,
  csvParaTexto,
  ofxParaTexto,
  xlsxParaTexto,
} from "../../workers/ingestion/src/parsers/tabular";

describe("decodificarTexto", () => {
  it("lê UTF-8 quando o conteúdo é UTF-8 válido", () => {
    const buf = Buffer.from("Água e Esgoto — Março", "utf-8");
    expect(decodificarTexto(buf)).toBe("Água e Esgoto — Março");
  });

  it("cai para latin-1 quando o conteúdo não é UTF-8 válido", () => {
    // Extrato de banco brasileiro exportado no Windows: cp1252/latin-1.
    const buf = Buffer.from("Manuten\xE7\xE3o de conta", "latin1");
    expect(decodificarTexto(buf)).toBe("Manutenção de conta");
  });

  it("descarta o BOM, que senão vira parte do primeiro cabeçalho", () => {
    const buf = Buffer.concat([
      Buffer.from([0xef, 0xbb, 0xbf]),
      Buffer.from("Data;Valor", "utf-8"),
    ]);
    expect(decodificarTexto(buf)).toBe("Data;Valor");
  });
});

describe("detectarDelimitador", () => {
  it("detecta ponto e vírgula — o padrão dos bancos brasileiros", () => {
    expect(detectarDelimitador("Data;Descricao;Valor\n01/03/2026;Mercado;-120,50")).toBe(";");
  });

  it("detecta vírgula", () => {
    expect(detectarDelimitador("Date,Description,Amount\n2026-03-01,Market,-120.50")).toBe(",");
  });

  it("detecta tabulação", () => {
    expect(detectarDelimitador("Data\tValor\n01/03/2026\t-120,50")).toBe("\t");
  });

  it("não confunde vírgula decimal com delimitador", () => {
    // Só há `;` separando campos; as vírgulas são decimais. Contar ocorrências
    // brutas elegeria a vírgula e quebraria cada linha em pedaços errados.
    const csv = "Data;Valor;Saldo\n01/03/2026;-120,50;1.234,56\n02/03/2026;-80,00;1.154,56";
    expect(detectarDelimitador(csv)).toBe(";");
  });

  it("devolve null para texto corrido salvo como .csv", () => {
    // A regressão que custou cinquenta centavos: uma conta de luz exportada em
    // .csv não tem delimitador nenhum. Eleger o "menos ruim" quebrava
    // `Valor a Pagar R$ 245,50` em dois campos e o valor virava R$ 245,00.
    const conta = [
      "CEMIG DISTRIBUIÇÃO S.A.",
      "Mês Referência MARÇO/2026",
      "Valor a Pagar R$ 245,50",
      "Vencimento: 15/04/2026",
    ].join("\n");

    expect(detectarDelimitador(conta)).toBeNull();
  });

  it("devolve null para texto vazio", () => {
    expect(detectarDelimitador("")).toBeNull();
  });
});

describe("parseCsv", () => {
  it("separa cabeçalho e linhas", () => {
    const { cabecalho, linhas } = parseCsv("Data;Descricao;Valor\n01/03/2026;Mercado;-120,50");

    expect(cabecalho).toEqual(["Data", "Descricao", "Valor"]);
    expect(linhas).toEqual([["01/03/2026", "Mercado", "-120,50"]]);
  });

  it("respeita campos entre aspas com o delimitador dentro", () => {
    const { linhas } = parseCsv('Data;Descricao\n01/03/2026;"Mercado; feira"');
    expect(linhas[0]).toEqual(["01/03/2026", "Mercado; feira"]);
  });

  it("entende aspas duplicadas como aspa literal", () => {
    // Duas colunas de propósito: uma coluna só não é tabela, e a detecção
    // devolve `null` nesse caso — ver o teste de texto corrido acima.
    const { linhas } = parseCsv('Descricao;Valor\n"Loja ""Central""";-10,00');
    expect(linhas[0]).toEqual(['Loja "Central"', "-10,00"]);
  });

  it("ignora linhas em branco no fim do arquivo", () => {
    const { linhas } = parseCsv("Data;Valor\n01/03/2026;-10,00\n\n\n");
    expect(linhas).toHaveLength(1);
  });

  it("devolve tabela vazia com delimitador nulo quando não é tabular", () => {
    const t = parseCsv("Valor a Pagar R$ 245,50\nVencimento: 15/04/2026");

    expect(t.delimitador).toBeNull();
    expect(t.linhas).toEqual([]);
  });

  it("aceita CRLF", () => {
    const { cabecalho, linhas } = parseCsv("Data;Valor\r\n01/03/2026;-10,00\r\n");
    expect(cabecalho).toEqual(["Data", "Valor"]);
    expect(linhas[0]).toEqual(["01/03/2026", "-10,00"]);
  });
});

describe("csvParaTexto", () => {
  it("devolve linhas legíveis para os parsers de regex", () => {
    const texto = csvParaTexto(parseCsv("Data;Descricao;Valor\n01/03/2026;CEMIG;-245,50"));

    expect(texto).toContain("Data: 01/03/2026");
    expect(texto).toContain("Descricao: CEMIG");
    expect(texto).toContain("Valor: -245,50");
  });
});

const OFX_V1 = `OFXHEADER:100
DATA:OFXSGML
VERSION:102

<OFX>
<BANKMSGSRSV1><STMTTRNRS><STMTRS>
<CURDEF>BRL
<BANKACCTFROM><BANKID>001<ACCTID>12345-6<ACCTTYPE>CHECKING</BANKACCTFROM>
<BANKTRANLIST>
<STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20260301<TRNAMT>-245.50<FITID>001<MEMO>CEMIG CONTA DE ENERGIA</STMTTRN>
<STMTTRN><TRNTYPE>CREDIT<DTPOSTED>20260305<TRNAMT>3200.00<FITID>002<MEMO>SALARIO</STMTTRN>
</BANKTRANLIST>
</STMTRS></STMTTRNRS></BANKMSGSRSV1>
</OFX>`;

describe("parseOfx", () => {
  it("extrai as transações do formato SGML (OFX 1.x)", () => {
    const { transacoes, moeda, conta } = parseOfx(OFX_V1);

    expect(moeda).toBe("BRL");
    expect(conta).toBe("12345-6");
    expect(transacoes).toHaveLength(2);
    expect(transacoes[0]).toEqual({
      tipo: "DEBIT",
      data: "2026-03-01",
      valor: -245.5,
      identificador: "001",
      descricao: "CEMIG CONTA DE ENERGIA",
    });
    expect(transacoes[1]!.valor).toBe(3200);
  });

  it("entende OFX 2.x, que é XML com as mesmas tags", () => {
    const xml = `<?xml version="1.0"?>
<OFX><BANKTRANLIST>
<STMTTRN><TRNTYPE>DEBIT</TRNTYPE><DTPOSTED>20260301120000[-3:BRT]</DTPOSTED><TRNAMT>-99.90</TRNAMT><FITID>A1</FITID><MEMO>NETFLIX</MEMO></STMTTRN>
</BANKTRANLIST></OFX>`;

    const { transacoes } = parseOfx(xml);
    expect(transacoes).toHaveLength(1);
    expect(transacoes[0]!.data).toBe("2026-03-01");
    expect(transacoes[0]!.valor).toBe(-99.9);
    expect(transacoes[0]!.descricao).toBe("NETFLIX");
  });

  it("não inventa transação quando o arquivo não tem nenhuma", () => {
    expect(parseOfx("<OFX><SIGNONMSGSRSV1></SIGNONMSGSRSV1></OFX>").transacoes).toEqual([]);
  });

  it("aceita vírgula decimal, que alguns bancos emitem fora do padrão", () => {
    const ofx = "<OFX><STMTTRN><TRNAMT>-1234,56<MEMO>TESTE</STMTTRN></OFX>";
    expect(parseOfx(ofx).transacoes[0]!.valor).toBe(-1234.56);
  });
});

describe("ofxParaTexto", () => {
  it("gera linhas que os parsers de regex conseguem ler", () => {
    const texto = ofxParaTexto(parseOfx(OFX_V1));

    expect(texto).toContain("Conta: 12345-6");
    expect(texto).toContain("2026-03-01");
    expect(texto).toContain("CEMIG CONTA DE ENERGIA");
    expect(texto).toContain("-245.50");
  });
});

async function planilhaDeTeste(): Promise<Buffer> {
  const wb = new ExcelJS.Workbook();
  const ws = wb.addWorksheet("Extrato");
  ws.addRow(["Data", "Descricao", "Valor"]);
  ws.addRow([new Date(Date.UTC(2026, 2, 1)), "CEMIG", -245.5]);
  ws.addRow([new Date(Date.UTC(2026, 2, 5)), "SALARIO", 3200]);
  ws.addRow([]);
  return Buffer.from(await wb.xlsx.writeBuffer());
}

describe("xlsxParaTexto", () => {
  it("lê a planilha em vez de devolver o ZIP como texto", async () => {
    const texto = await xlsxParaTexto(await planilhaDeTeste());

    // A regressão que este teste tranca: antes, XLSX caía num
    // `Buffer.toString()` e o resultado começava com "PK", a assinatura do ZIP.
    expect(texto.startsWith("PK")).toBe(false);
    expect(texto).toContain("Planilha: Extrato");
    expect(texto).toContain("Descricao: CEMIG");
    expect(texto).toContain("Valor: -245.5");
  });

  it("formata datas no padrão brasileiro, não como número de série nem Date", async () => {
    const texto = await xlsxParaTexto(await planilhaDeTeste());

    expect(texto).toContain("Data: 01/03/2026");
    expect(texto).not.toContain("GMT");
  });

  it("descarta linhas totalmente vazias", async () => {
    const texto = await xlsxParaTexto(await planilhaDeTeste());
    const registros = texto.split("\n\n").filter((b) => b.startsWith("Data:"));

    expect(registros).toHaveLength(2);
  });
});
