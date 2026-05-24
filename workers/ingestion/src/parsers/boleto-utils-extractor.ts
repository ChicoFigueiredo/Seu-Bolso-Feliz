interface BoletoUtilsApi {
  validarBoleto: (codigo: string) => {
    sucesso: boolean;
    tipoBoleto?: string;
    codigoBarras?: string;
    linhaDigitavel?: string;
    vencimento?: string | Date;
    vencimentoComNovoFator2025?: string | Date;
    valor?: number;
  };
}

export interface BoletoUtilsResult {
  supplierNameRaw: string | null;
  dueDate: string | null;
  totalAmount: number | null;
  barcodeDigitableLine: string | null;
  boletoType: string | null;
  confidence: number;
  reason: string;
}

function normalizeDate(value: string | Date | undefined): string | null {
  if (!value) return null;
  const dt = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(dt.getTime())) return null;
  const iso = dt.toISOString();
  return iso.slice(0, 10);
}

function detectBoletoCandidates(text: string): string[] {
  const found = new Set<string>();

  const lines47or48 = text.match(/\b\d{46,48}\b/g) ?? [];
  for (const item of lines47or48) {
    found.add(item.replace(/\D/g, ""));
  }

  const codedFormatted =
    text.match(/\d{5}\.\d{5}\s*\d{5}\.\d{6}\s*\d{5}\.\d{6}\s*\d\s*\d{14}/g) ?? [];
  for (const item of codedFormatted) {
    found.add(item.replace(/\D/g, ""));
  }

  const barcode44 = text.match(/\b\d{44}\b/g) ?? [];
  for (const item of barcode44) {
    found.add(item);
  }

  return Array.from(found);
}

async function importBoletoUtils(): Promise<BoletoUtilsApi | null> {
  try {
    const mod = (await import("@mrmgomes/boleto-utils")) as unknown as BoletoUtilsApi;
    if (typeof mod.validarBoleto !== "function") return null;
    return mod;
  } catch {
    return null;
  }
}

export async function extractWithBoletoUtils(text: string): Promise<BoletoUtilsResult | null> {
  const api = await importBoletoUtils();
  if (!api) return null;

  const candidates = detectBoletoCandidates(text);
  for (const candidate of candidates) {
    try {
      const parsed = api.validarBoleto(candidate);
      if (!parsed?.sucesso) continue;

      const dueDate = normalizeDate(parsed.vencimento ?? parsed.vencimentoComNovoFator2025);
      const totalAmount =
        typeof parsed.valor === "number" && Number.isFinite(parsed.valor) ? parsed.valor : null;
      const line = parsed.linhaDigitavel?.replace(/\s+/g, "") ?? candidate;

      const supplierNameRaw =
        parsed.tipoBoleto === "BANCO" || parsed.tipoBoleto === "CARTAO_DE_CREDITO"
          ? "BOLETO BANCARIO"
          : parsed.tipoBoleto
            ? `BOLETO ${parsed.tipoBoleto}`
            : "BOLETO";

      return {
        supplierNameRaw,
        dueDate,
        totalAmount,
        barcodeDigitableLine: line,
        boletoType: parsed.tipoBoleto ?? null,
        confidence: 0.99,
        reason: "validated_by_boleto_utils",
      };
    } catch {
      continue;
    }
  }

  return null;
}
