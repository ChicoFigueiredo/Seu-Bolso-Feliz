/**
 * Boleto de energia, anonimizado.
 *
 * O bloco `expected` é a semente do conjunto de goldens (§15.5): a saída
 * esperada fica versionada junto do documento, para que uma regressão de
 * parser seja detectada em vez de percebida meses depois num relatório errado.
 */
import { buildSimplePdf } from "../builders/pdf";

/** Linha digitável de 47 dígitos, anonimizada. */
export const CEMIG_BARCODE = "83640000001 2 34560000000 3 45670000000 4 56780000000 5";

export const boletoCemig = {
  filename: "boleto-cemig-03-2026.pdf",
  mimeType: "application/pdf",

  lines: [
    "CEMIG DISTRIBUICAO S.A.",
    "CNPJ: 06.981.180/0001-16",
    "CONTA DE ENERGIA ELETRICA",
    "",
    "Instalacao: 3001234567",
    "Referencia: MARCO/2026",
    "Vencimento: 15/04/2026",
    "Valor a pagar: R$ 245,50",
    "",
    "Consumo: 320 kWh",
    "Periodo: 01/03/2026 a 31/03/2026",
    "",
    `Linha digitavel: ${CEMIG_BARCODE}`,
  ],

  bytes(): Buffer {
    return buildSimplePdf(this.lines);
  },

  /** Também disponível como texto, para exercitar o caminho CSV/texto. */
  asText(): string {
    return this.lines.join("\n");
  },

  expected: {
    supplierName: "CEMIG",
    amountCents: 24550,
    dueDate: "2026-04-15",
    competenceMonth: "2026-03",
    barcodeDigits: CEMIG_BARCODE.replace(/\D/g, ""),
    consumptionKwh: 320,
  },
} as const;
