import { describe, expect, it } from "vitest";
import { extractWithBoletoUtils } from "../../workers/ingestion/src/parsers/boleto-utils-extractor";

describe("boleto-utils-extractor", () => {
  it("retorna null quando texto não contém candidatos de linha digitável/código de barras", async () => {
    const result = await extractWithBoletoUtils("texto sem boleto");
    expect(result).toBeNull();
  });

  it("extrai campos de um boleto válido conhecido", async () => {
    const text = ["23794150099001980167035000211405700000000000000", "Documento de teste"].join(
      "\n",
    );

    const result = await extractWithBoletoUtils(text);

    expect(result).toBeTruthy();
    expect(result?.confidence).toBe(0.99);
    expect(result?.barcodeDigitableLine).toBeTruthy();
    expect((result?.barcodeDigitableLine?.length ?? 0) >= 44).toBe(true);
    expect(result?.supplierNameRaw?.startsWith("BOLETO")).toBe(true);
  });
});
