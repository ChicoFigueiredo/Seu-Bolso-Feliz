/**
 * Parser determinístico: Boleto/Conta Genérica
 *
 * Tenta extrair dados básicos de qualquer boleto ou conta de serviço:
 * - Valor
 * - Vencimento
 * - CNPJ/Nome do cedente
 * - Número do documento
 * - Linha digitável (código de barras)
 */

export interface BoletoExtractionResult {
  supplierNameRaw: string | null;
  supplierCnpj: string | null;
  competenceDate: string | null;
  dueDate: string | null;
  totalAmount: number | null;
  documentNumber: string | null;
  barcodeDigitableLine: string | null;
  confidence: number;
}

/** Extrai dados de boleto/conta genérica */
export function parseBoleto(text: string): BoletoExtractionResult {
  const result: BoletoExtractionResult = {
    supplierNameRaw: null,
    supplierCnpj: null,
    competenceDate: null,
    dueDate: null,
    totalAmount: null,
    documentNumber: null,
    barcodeDigitableLine: null,
    confidence: 0,
  };

  let hits = 0;
  const maxHits = 5;

  // ── Valor ──
  const valorMatch =
    text.match(/VALOR\s+(?:DO\s+)?(?:DOCUMENTO|BOLETO|COBRAN[CÇ]A)[:\s]*R?\$?\s*([\d.,]+)/i) ??
    text.match(/VALOR\s+(?:A\s+)?PAGAR[:\s]*R?\$?\s*([\d.,]+)/i) ??
    text.match(/TOTAL[:\s]*R?\$?\s*([\d.,]+)/i);
  if (valorMatch) {
    result.totalAmount = parseBrlAmount(valorMatch[1]!);
    if (result.totalAmount !== null) hits++;
  }

  // ── Vencimento ──
  const vencMatch =
    text.match(/VENCIMENTO[:\s]*(\d{2})[\/\-](\d{2})[\/\-](\d{4})/i) ??
    text.match(/DATA\s+(?:DE\s+)?VENCIMENTO[:\s]*(\d{2})[\/\-](\d{2})[\/\-](\d{4})/i);
  if (vencMatch) {
    result.dueDate = `${vencMatch[3]}-${vencMatch[2]}-${vencMatch[1]}`;
    hits++;
  }

  // ── Competência ──
  const compMatch =
    text.match(/(?:REFER[EÊ]NCIA|COMPETÊNCIA|PER[IÍ]ODO)[:\s]*(\d{2})[\/\-](\d{4})/i) ??
    text.match(/(?:REFER[EÊ]NCIA|COMPETÊNCIA)[:\s]*(\d{2})[\/\-](\d{2})[\/\-](\d{4})/i);
  if (compMatch) {
    if (compMatch[3]) {
      // DD/MM/YYYY
      result.competenceDate = `${compMatch[3]}-${compMatch[2]}-${compMatch[1]}`;
    } else {
      // MM/YYYY
      result.competenceDate = `${compMatch[2]}-${compMatch[1]}-01`;
    }
    hits++;
  }

  // ── CNPJ ──
  const cnpjMatch = text.match(/(?:CNPJ)[:\s]*(\d{2}\.?\d{3}\.?\d{3}\/?\d{4}\-?\d{2})/i);
  if (cnpjMatch) {
    result.supplierCnpj = cnpjMatch[1]!.replace(/[.\-\/]/g, "");
    hits++;
  }

  // ── Nome do cedente/beneficiário ──
  const cedenteMatch =
    text.match(/(?:CEDENTE|BENEFICI[AÁ]RIO)[:\s]*([A-ZÁÀÂÃÉÈÊÍÏÓÔÕÚÜÇ][^\n]{3,60})/i);
  if (cedenteMatch) {
    result.supplierNameRaw = cedenteMatch[1]!.trim();
  }

  // ── Número do documento ──
  const docMatch =
    text.match(/(?:N[ÚU]MERO\s+(?:DO\s+)?DOCUMENTO)[:\s]*(\S+)/i) ??
    text.match(/(?:NOSSO\s+N[ÚU]MERO)[:\s]*(\S+)/i);
  if (docMatch) {
    result.documentNumber = docMatch[1]!;
    hits++;
  }

  // ── Linha digitável ──
  const linhaMatch = text.match(/(\d{5}\.?\d{5}\s*\d{5}\.?\d{6}\s*\d{5}\.?\d{6}\s*\d\s*\d{14})/);
  if (linhaMatch) {
    result.barcodeDigitableLine = linhaMatch[1]!.replace(/\s+/g, "");
  }

  result.confidence = Math.min(hits / maxHits, 1);

  return result;
}

function parseBrlAmount(raw: string): number | null {
  const normalized = raw.replace(/\./g, "").replace(",", ".");
  const val = parseFloat(normalized);
  return isNaN(val) ? null : val;
}
