export interface SupplierTemplateResult {
  templateId: string;
  supplierNameRaw: string | null;
  supplierCnpj: string | null;
  totalAmount: number | null;
  dueDate: string | null;
  competenceDate: string | null;
  documentNumber: string | null;
  confidence: number;
  matchedRules: string[];
}

interface TemplateDef {
  id: string;
  supplierName: string;
  supplierHints: RegExp[];
  fieldRegex: {
    totalAmount?: RegExp[];
    dueDate?: RegExp[];
    competenceDate?: RegExp[];
    documentNumber?: RegExp[];
    supplierCnpj?: RegExp[];
  };
}

const TEMPLATE_DEFS: TemplateDef[] = [
  {
    id: "tpl-cemig-energia-v1",
    supplierName: "CEMIG DISTRIBUICAO S.A.",
    supplierHints: [/\bCEMIG\b/i, /COMPANHIA\s+ENERG[ÉE]TICA\s+DE\s+MINAS\s+GERAIS/i],
    fieldRegex: {
      totalAmount: [/VALOR\s+(?:A\s+)?PAGAR[:\s]*R?\$?\s*([\d.,]+)/i],
      dueDate: [/VENCIMENTO[:\s]*(\d{2})[/-](\d{2})[/-](\d{4})/i],
      competenceDate: [
        /(?:M[EÊ]S\s+REFER[EÊ]NCIA|REFER[EÊ]NCIA)[:\s]*([A-ZÀ-ÖØ-öø-ÿ]{3,})\/?(\d{4})/i,
      ],
      documentNumber: [/(?:N[ÚU]MERO\s+DA\s+NOTA|NF)[:\s]*(\S+)/i],
      supplierCnpj: [/CNPJ[:\s]*(\d{2}\.\d{3}\.\d{3}\/\d{4}-\d{2})/i],
    },
  },
  {
    id: "tpl-vivo-telecom-v1",
    supplierName: "VIVO S.A.",
    supplierHints: [/\bVIVO\b/i, /TELEFONICA\s+BRASIL/i],
    fieldRegex: {
      totalAmount: [
        /TOTAL\s+A\s+PAGAR[:\s]*R?\$?\s*([\d.,]+)/i,
        /VALOR\s+DO\s+DOCUMENTO[:\s]*R?\$?\s*([\d.,]+)/i,
      ],
      dueDate: [/VENCIMENTO[:\s]*(\d{2})[/-](\d{2})[/-](\d{4})/i],
      competenceDate: [/(?:REFER[EÊ]NCIA|PER[IÍ]ODO)[:\s]*(\d{2})[/-](\d{4})/i],
      documentNumber: [/(?:N[ÚU]MERO\s+DO\s+DOCUMENTO|FATURA)[:\s]*(\S+)/i],
      supplierCnpj: [/CNPJ[:\s]*(\d{2}\.\d{3}\.\d{3}\/\d{4}-\d{2})/i],
    },
  },
];

const PT_MONTHS: Record<string, string> = {
  JAN: "01",
  FEV: "02",
  MAR: "03",
  ABR: "04",
  MAI: "05",
  JUN: "06",
  JUL: "07",
  AGO: "08",
  SET: "09",
  OUT: "10",
  NOV: "11",
  DEZ: "12",
  JANEIRO: "01",
  FEVEREIRO: "02",
  MARCO: "03",
  MARÇO: "03",
  ABRIL: "04",
  MAIO: "05",
  JUNHO: "06",
  JULHO: "07",
  AGOSTO: "08",
  SETEMBRO: "09",
  OUTUBRO: "10",
  NOVEMBRO: "11",
  DEZEMBRO: "12",
};

function parseBrlAmount(value: string): number | null {
  const normalized = value.replace(/\./g, "").replace(/,/g, ".");
  const parsed = Number.parseFloat(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

function normalizeCnpj(value: string): string {
  return value.replace(/[^\d]/g, "");
}

function extractFirst(regexes: RegExp[] | undefined, text: string): RegExpExecArray | null {
  if (!regexes) return null;
  for (const regex of regexes) {
    const match = regex.exec(text);
    if (match) return match;
  }
  return null;
}

function parseCompetenceFromMatch(match: RegExpExecArray): string | null {
  if (!match[1]) return null;

  // MM/YYYY
  if (match[1].length === 2 && match[2]?.length === 4) {
    return `${match[2]}-${match[1]}-01`;
  }

  const monthRaw = match[1].toUpperCase();
  const month = PT_MONTHS[monthRaw];
  const year = match[2];
  if (!month || !year) return null;
  return `${year}-${month}-01`;
}

export function applySupplierTemplate(text: string): SupplierTemplateResult | null {
  for (const tpl of TEMPLATE_DEFS) {
    const isSupplierMatch = tpl.supplierHints.some((hint) => hint.test(text));
    if (!isSupplierMatch) continue;

    const matchedRules: string[] = ["supplier_hint"];

    const amountMatch = extractFirst(tpl.fieldRegex.totalAmount, text);
    const dueMatch = extractFirst(tpl.fieldRegex.dueDate, text);
    const competenceMatch = extractFirst(tpl.fieldRegex.competenceDate, text);
    const documentMatch = extractFirst(tpl.fieldRegex.documentNumber, text);
    const cnpjMatch = extractFirst(tpl.fieldRegex.supplierCnpj, text);

    const totalAmount = amountMatch ? parseBrlAmount(amountMatch[1] ?? "") : null;
    if (totalAmount !== null) matchedRules.push("total_amount");

    const dueDate = dueMatch ? `${dueMatch[3]}-${dueMatch[2]}-${dueMatch[1]}` : null;
    if (dueDate) matchedRules.push("due_date");

    const competenceDate = competenceMatch ? parseCompetenceFromMatch(competenceMatch) : null;
    if (competenceDate) matchedRules.push("competence_date");

    const documentNumber = documentMatch?.[1]?.trim() ?? null;
    if (documentNumber) matchedRules.push("document_number");

    const supplierCnpj = cnpjMatch?.[1] ? normalizeCnpj(cnpjMatch[1]) : null;
    if (supplierCnpj) matchedRules.push("supplier_cnpj");

    const confidence = Math.min(0.55 + matchedRules.length * 0.07, 0.93);

    return {
      templateId: tpl.id,
      supplierNameRaw: tpl.supplierName,
      supplierCnpj,
      totalAmount,
      dueDate,
      competenceDate,
      documentNumber,
      confidence,
      matchedRules,
    };
  }

  return null;
}
