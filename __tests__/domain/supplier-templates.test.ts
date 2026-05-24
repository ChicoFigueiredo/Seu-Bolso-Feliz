import { describe, expect, it } from "vitest";
import { applySupplierTemplate } from "../../workers/ingestion/src/parsers/supplier-templates";

describe("supplier-templates", () => {
  it("aplica template CEMIG com extração de campos principais", () => {
    const text = [
      "CEMIG DISTRIBUIÇÃO S.A.",
      "MÊS REFERÊNCIA: MARÇO/2026",
      "VENCIMENTO: 15/04/2026",
      "VALOR A PAGAR: R$ 245,50",
      "NÚMERO DA NOTA: NF1234",
      "CNPJ: 06.981.180/0001-16",
    ].join("\n");

    const result = applySupplierTemplate(text);

    expect(result).toBeTruthy();
    expect(result?.templateId).toBe("tpl-cemig-energia-v1");
    expect(result?.supplierNameRaw).toContain("CEMIG");
    expect(result?.totalAmount).toBe(245.5);
    expect(result?.dueDate).toBe("2026-04-15");
    expect(result?.competenceDate).toBe("2026-03-01");
    expect(result?.supplierCnpj).toBe("06981180000116");
  });

  it("aplica template VIVO com competência MM/YYYY", () => {
    const text = [
      "VIVO S.A.",
      "TOTAL A PAGAR: R$ 99,90",
      "VENCIMENTO: 20/04/2026",
      "REFERÊNCIA: 03/2026",
      "NÚMERO DO DOCUMENTO: 987654321",
      "CNPJ: 02.558.157/0001-62",
    ].join("\n");

    const result = applySupplierTemplate(text);

    expect(result).toBeTruthy();
    expect(result?.templateId).toBe("tpl-vivo-telecom-v1");
    expect(result?.competenceDate).toBe("2026-03-01");
    expect(result?.documentNumber).toBe("987654321");
  });

  it("retorna null quando nenhum template combina", () => {
    const result = applySupplierTemplate("Documento aleatório sem fornecedor conhecido");
    expect(result).toBeNull();
  });

  it("gera confiança em faixa plausível", () => {
    const text = ["VIVO S.A.", "TOTAL A PAGAR: R$ 10,00", "VENCIMENTO: 01/05/2026"].join("\n");

    const result = applySupplierTemplate(text);
    expect(result).toBeTruthy();
    expect((result?.confidence ?? 0) > 0.55).toBe(true);
    expect((result?.confidence ?? 1) <= 0.93).toBe(true);
  });
});
