/**
 * Testes unitários: Parsers de documentos financeiros
 *
 * 4.4 — Parser CEMIG (regex determinístico)
 * 4.5 — Parser Boleto genérico
 * 4.1 — Extração de texto (CSV, fallback)
 */
import { describe, it, expect } from "vitest";
import { isCemig, parseCemig } from "../../workers/ingestion/src/parsers/cemig-parser";
import { parseBoleto } from "../../workers/ingestion/src/parsers/boleto-parser";
import { extractText } from "../../workers/ingestion/src/parsers/text-extractor";

// ══════════════════════════════════════════════════════════════
// 4.4 — Parser CEMIG
// ══════════════════════════════════════════════════════════════

describe("4.4: Parser CEMIG", () => {
  describe("isCemig()", () => {
    it("detecta texto contendo 'CEMIG'", () => {
      expect(isCemig("Conta de Energia CEMIG")).toBe(true);
    });

    it("detecta texto contendo nome completo em maiúsculas", () => {
      expect(isCemig("COMPANHIA ENERGETICA DE MINAS GERAIS")).toBe(true);
    });

    it("detecta com acentuação", () => {
      expect(isCemig("Companhia Energética de Minas Gerais")).toBe(true);
    });

    it("rejeita texto que não menciona CEMIG", () => {
      expect(isCemig("Fatura Vivo Telefone")).toBe(false);
    });

    it("case-insensitive", () => {
      expect(isCemig("cemig distribuição")).toBe(true);
    });
  });

  describe("parseCemig()", () => {
    const CEMIG_FULL = [
      "CEMIG DISTRIBUIÇÃO S.A.",
      "MÊS REFERÊNCIA: MARÇO/2026",
      "VENCIMENTO: 15/04/2026",
      "VALOR A PAGAR: R$ 245,50",
      "NÚMERO DA NOTA: 123456",
      "UC: 12345678",
      "CONSUMO: 320 kWh",
      "30 dias de consumo",
      "ENERGIA ELÉTRICA: R$ 180,00",
      "ILUMINAÇÃO PÚBLICA: R$ 25,50",
      "BANDEIRA VERMELHA: R$ 15,00",
      "ICMS: R$ 25,00",
    ].join("\n");

    it("extrai competência corretamente", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.competenceDate).toBe("2026-03-01");
    });

    it("extrai vencimento corretamente", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.dueDate).toBe("2026-04-15");
    });

    it("extrai valor total corretamente", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.totalAmount).toBe(245.5);
    });

    it("extrai número do documento", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.documentNumber).toBe("123456");
    });

    it("extrai UC (contrato)", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.contractIdentifier).toBe("12345678");
    });

    it("extrai consumo kWh", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.consumption.kwh).toBe(320);
    });

    it("extrai dias de consumo", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.consumption.days).toBe(30);
    });

    it("extrai itens de breakdown", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(Object.keys(result.breakdown).length).toBeGreaterThanOrEqual(2);
    });

    it("confiança alta quando todos os campos são extraídos", () => {
      const result = parseCemig(CEMIG_FULL);
      expect(result.confidence).toBeGreaterThanOrEqual(0.8);
    });

    it("confiança baixa com texto mínimo", () => {
      const result = parseCemig("CEMIG alguma coisa sem dados relevantes");
      expect(result.confidence).toBeLessThan(0.3);
    });

    it("fornecedor sempre é 'CEMIG'", () => {
      const result = parseCemig("CEMIG qualquer coisa");
      expect(result.supplierNameRaw).toBe("CEMIG");
    });

    it("resolve valor com formato 1.234,56", () => {
      const text = "CEMIG\nValor a pagar: R$ 1.234,56";
      const result = parseCemig(text);
      expect(result.totalAmount).toBe(1234.56);
    });

    it("competência com padrão alternativo", () => {
      const text = "CEMIG\nREFERÊNCIA: JANEIRO/2026";
      const result = parseCemig(text);
      expect(result.competenceDate).toBe("2026-01-01");
    });
  });
});

// ══════════════════════════════════════════════════════════════
// 4.5 — Parser Boleto genérico
// ══════════════════════════════════════════════════════════════

describe("4.5: Parser Boleto genérico", () => {
  const BOLETO_FULL = [
    "BENEFICIÁRIO: Vivo S.A.",
    "CNPJ: 02.558.157/0001-62",
    "VALOR DO DOCUMENTO: R$ 99,90",
    "VENCIMENTO: 20/04/2026",
    "REFERÊNCIA: 03/2026",
    "NÚMERO DO DOCUMENTO: 987654321",
    "23793.38128 60000.000003 00000.000405 1 89920000009990",
  ].join("\n");

  it("extrai valor total", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.totalAmount).toBe(99.9);
  });

  it("extrai vencimento", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.dueDate).toBe("2026-04-20");
  });

  it("extrai competência MM/YYYY", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.competenceDate).toBe("2026-03-01");
  });

  it("extrai CNPJ limpo", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.supplierCnpj).toBe("02558157000162");
  });

  it("extrai nome do beneficiário", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.supplierNameRaw).toBe("Vivo S.A.");
  });

  it("extrai número do documento", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.documentNumber).toBe("987654321");
  });

  it("extrai linha digitável", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.barcodeDigitableLine).toBeTruthy();
    expect(result.barcodeDigitableLine!.length).toBeGreaterThan(40);
  });

  it("confiança alta com vários campos preenchidos", () => {
    const result = parseBoleto(BOLETO_FULL);
    expect(result.confidence).toBeGreaterThanOrEqual(0.6);
  });

  it("confiança baixa com texto sem dados úteis", () => {
    const result = parseBoleto("Apenas um texto qualquer sem estrutura");
    expect(result.confidence).toBe(0);
  });

  it("valor formato milhar: 1.500,00", () => {
    const text = "VALOR A PAGAR: R$ 1.500,00";
    const result = parseBoleto(text);
    expect(result.totalAmount).toBe(1500);
  });

  it("vencimento formato alternativo com DATA DE VENCIMENTO", () => {
    const text = "DATA DE VENCIMENTO: 05/05/2026";
    const result = parseBoleto(text);
    expect(result.dueDate).toBe("2026-05-05");
  });

  it("NOSSO NÚMERO como número do documento", () => {
    const text = "NOSSO NÚMERO: ABC123456";
    const result = parseBoleto(text);
    expect(result.documentNumber).toBe("ABC123456");
  });
});

// ══════════════════════════════════════════════════════════════
// 4.1 — Extração de texto (text-extractor)
// ══════════════════════════════════════════════════════════════

describe("4.1: Text extractor", () => {
  it("extrai texto de CSV (text/csv)", async () => {
    const content = "data,valor,descricao\n2026-03-01,100.50,Teste";
    const buf = Buffer.from(content);
    const result = await extractText(buf, "text/csv");
    expect(result.text).toBe(content);
    expect(result.pages).toBe(1);
    expect(result.wasProtected).toBe(false);
  });

  it("extrai texto de XML (application/xml)", async () => {
    const xml = "<root><item>Teste</item></root>";
    const buf = Buffer.from(xml);
    const result = await extractText(buf, "application/xml");
    expect(result.text).toBe(xml);
    expect(result.mimeType).toBe("application/xml");
  });

  it("fallback para texto plano em MIME desconhecido", async () => {
    const content = "Texto qualquer para fallback";
    const buf = Buffer.from(content);
    const result = await extractText(buf, "application/octet-stream");
    expect(result.text).toBe(content);
    expect(result.pages).toBe(1);
  });

  it("imagem retorna texto vazio (futuro: OCR)", async () => {
    const buf = Buffer.from([0x89, 0x50, 0x4e, 0x47]); // PNG header
    const result = await extractText(buf, "image/png");
    expect(result.text).toBe("");
    expect(result.mimeType).toBe("image/png");
  });

  it("ArrayBuffer é aceito como entrada", async () => {
    const content = "Teste com ArrayBuffer";
    const arrayBuf = new TextEncoder().encode(content).buffer;
    const result = await extractText(arrayBuf, "text/csv");
    expect(result.text).toBe(content);
  });
});
