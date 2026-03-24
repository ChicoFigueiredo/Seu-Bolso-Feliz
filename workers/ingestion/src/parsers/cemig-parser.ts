/**
 * Parser determinístico: CEMIG (Conta de Energia Elétrica)
 *
 * Extrai dados estruturados de contas de luz da CEMIG via regex/patterns.
 * Padrões comuns encontrados em contas CEMIG:
 * - MÊS REFERÊNCIA / COMPETÊNCIA
 * - VENCIMENTO
 * - VALOR (R$)
 * - CONSUMO (kWh)
 * - NÚMERO DO DOCUMENTO / NOTA FISCAL
 * - NÚMERO DO CONTRATO / UC
 */

export interface CemigExtractionResult {
  supplierNameRaw: string;
  competenceDate: string | null;      // YYYY-MM-DD
  dueDate: string | null;             // YYYY-MM-DD
  totalAmount: number | null;
  documentNumber: string | null;
  contractIdentifier: string | null;  // Unidade Consumidora (UC)
  consumption: {
    kwh: number | null;
    days: number | null;
  };
  confidence: number;
  breakdown: Record<string, number>;  // Itens da fatura
}

const MONTH_MAP: Record<string, string> = {
  JAN: "01", FEV: "02", MAR: "03", ABR: "04",
  MAI: "05", JUN: "06", JUL: "07", AGO: "08",
  SET: "09", OUT: "10", NOV: "11", DEZ: "12",
  JANEIRO: "01", FEVEREIRO: "02", MARÇO: "03", ABRIL: "04",
  MAIO: "05", JUNHO: "06", JULHO: "07", AGOSTO: "08",
  SETEMBRO: "09", OUTUBRO: "10", NOVEMBRO: "11", DEZEMBRO: "12",
};

/** Verifica se o texto parece ser uma conta da CEMIG */
export function isCemig(text: string): boolean {
  const normalized = text.toUpperCase();
  return (
    normalized.includes("CEMIG") ||
    normalized.includes("COMPANHIA ENERGETICA DE MINAS GERAIS") ||
    normalized.includes("COMPANHIA ENERGÉTICA DE MINAS GERAIS")
  );
}

/** Extrai dados estruturados de uma conta CEMIG */
export function parseCemig(text: string): CemigExtractionResult {
  const result: CemigExtractionResult = {
    supplierNameRaw: "CEMIG",
    competenceDate: null,
    dueDate: null,
    totalAmount: null,
    documentNumber: null,
    contractIdentifier: null,
    consumption: { kwh: null, days: null },
    confidence: 0,
    breakdown: {},
  };

  let hits = 0;
  const maxHits = 6; // competência, vencimento, valor, doc, contrato, consumo

  // ── Competência (mês de referência) ──
  const compMatch =
    text.match(/(?:M[EÊ]S\s+(?:REF(?:ER[EÊ]NCIA)?|COMPETÊNCIA))[:\s]*([A-ZÀ-ÖØ-öø-ÿ]+)[\/\s]*(\d{4})/i) ??
    text.match(/(?:REFER[EÊ]NCIA|COMPETÊNCIA)[:\s]*([A-ZÀ-ÖØ-öø-ÿ]+)[\/\s]*(\d{4})/i);
  if (compMatch) {
    const month = MONTH_MAP[compMatch[1]!.toUpperCase()];
    if (month) {
      result.competenceDate = `${compMatch[2]}-${month}-01`;
      hits++;
    }
  }

  // ── Vencimento ──
  const vencMatch =
    text.match(/VENCIMENTO[:\s]*(\d{2})[\/\-](\d{2})[\/\-](\d{4})/i) ??
    text.match(/DATA\s+(?:DE\s+)?VENCIMENTO[:\s]*(\d{2})[\/\-](\d{2})[\/\-](\d{4})/i);
  if (vencMatch) {
    result.dueDate = `${vencMatch[3]}-${vencMatch[2]}-${vencMatch[1]}`;
    hits++;
  }

  // ── Valor total ──
  const valorMatch =
    text.match(/VALOR\s+(?:A\s+)?PAGAR[:\s]*R?\$?\s*([\d.,]+)/i) ??
    text.match(/TOTAL[:\s]*R?\$?\s*([\d.,]+)/i) ??
    text.match(/VALOR\s+(?:DA\s+)?FATURA[:\s]*R?\$?\s*([\d.,]+)/i);
  if (valorMatch) {
    result.totalAmount = parseBrlAmount(valorMatch[1]!);
    if (result.totalAmount !== null) hits++;
  }

  // ── Número do documento / NF ──
  const docMatch =
    text.match(/(?:N[ÚU]MERO\s+(?:DA\s+)?(?:NOTA|NF))[:\s]*(\d+)/i) ??
    text.match(/(?:DOC(?:UMENTO)?|NF)[:\s]*(\d{6,})/i);
  if (docMatch) {
    result.documentNumber = docMatch[1]!;
    hits++;
  }

  // ── Unidade Consumidora (contrato) ──
  const ucMatch =
    text.match(/(?:UC|UNIDADE\s+CONSUMIDORA)[:\s]*(\d[\d.\-/]*\d)/i) ??
    text.match(/(?:INSTALAÇÃO|INSTALA[CÇ]ÃO)[:\s]*(\d[\d.\-/]*\d)/i);
  if (ucMatch) {
    result.contractIdentifier = ucMatch[1]!.replace(/[.\-/]/g, "");
    hits++;
  }

  // ── Consumo (kWh) ──
  const kwhMatch =
    text.match(/CONSUMO[:\s]*([\d.,]+)\s*kWh/i) ??
    text.match(/([\d.,]+)\s*kWh/i);
  if (kwhMatch) {
    const kwh = parseFloat(kwhMatch[1]!.replace(",", "."));
    if (!isNaN(kwh)) {
      result.consumption.kwh = kwh;
      hits++;
    }
  }

  // ── Dias de consumo ──
  const diasMatch = text.match(/(\d+)\s*dias?\s+(?:de\s+)?(?:consumo|faturamento)/i);
  if (diasMatch) {
    result.consumption.days = parseInt(diasMatch[1]!, 10);
  }

  // ── Breakdown: itens da fatura ──
  const itemPattern = /(?:ENERGIA|ILUMINAÇÃO|BANDEIRA|CIP|COSIP|ICMS|PIS|COFINS|CONTRIB)[^:]*[:\s]*R?\$?\s*([\d.,]+)/gi;
  let itemMatch: RegExpExecArray | null;
  while ((itemMatch = itemPattern.exec(text)) !== null) {
    const label = itemMatch[0]!.split(/[:\s]*R?\$?/)[0]!.trim();
    const amount = parseBrlAmount(itemMatch[1]!);
    if (amount !== null && label) {
      result.breakdown[label] = amount;
    }
  }

  // ── Calcular confiança ──
  result.confidence = Math.min(hits / maxHits, 1);

  return result;
}

/** Converte string de valor BRL para número */
function parseBrlAmount(raw: string): number | null {
  // "1.234,56" → 1234.56
  const normalized = raw.replace(/\./g, "").replace(",", ".");
  const val = parseFloat(normalized);
  return isNaN(val) ? null : val;
}
