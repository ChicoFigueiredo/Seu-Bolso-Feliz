/**
 * Financial Intent Classifier — classificador determinístico de intenção financeira.
 * Roda antes de qualquer chamada de IA. Retorna intent, confidence, reasons e missingFields.
 */

import type { FinancialIntent } from "./types";

export interface ClassifierInput {
  text: string;
  subject?: string;
  from?: string;
  hasAttachment?: boolean;
  mimeType?: string;
}

export interface ClassificationResult {
  intent: FinancialIntent;
  confidence: number;
  reasons: string[];
  missingFields: string[];
}

type SignalSet = {
  patterns: RegExp[];
  weight: number;
  label: string;
};

const SIGNALS: Record<FinancialIntent, SignalSet[]> = {
  bill_to_pay: [
    {
      patterns: [/linha digit[aá]vel/i, /c[oó]digo de barras/i, /\d{47,48}/],
      weight: 0.4,
      label: "linha_digitavel",
    },
    { patterns: [/boleto/i], weight: 0.25, label: "boleto" },
    {
      patterns: [/vencimento|pague at[eé]|data de vencimento/i],
      weight: 0.15,
      label: "vencimento",
    },
    { patterns: [/valor total|total a pagar|valor do boleto/i], weight: 0.1, label: "valor_total" },
    { patterns: [/boleto dispon[ií]vel|segunda via/i], weight: 0.1, label: "boleto_disponivel" },
  ],
  bill_reminder: [
    {
      patterns: [/sua fatura vence|vence amanh[aã]|lembrete de pagamento/i],
      weight: 0.4,
      label: "lembrete_fatura",
    },
    {
      patterns: [/n[aã]o esqueça|n[aã]o perca o prazo|evite juros/i],
      weight: 0.2,
      label: "aviso_prazo",
    },
    { patterns: [/fatura|pagamento/i], weight: 0.15, label: "fatura_pagamento" },
    { patterns: [/vencimento/i], weight: 0.15, label: "vencimento" },
    {
      patterns: [/nubank|c6bank|itau|bradesco|santander|caixa|bb|inter/i],
      weight: 0.1,
      label: "instituicao",
    },
  ],
  invoice_statement: [
    {
      patterns: [/fatura do cart[aã]o|total da fatura|fechamento da fatura/i],
      weight: 0.35,
      label: "fatura_cartao",
    },
    { patterns: [/melhor dia de compra|limite dispon[ií]vel/i], weight: 0.2, label: "cartao_info" },
    { patterns: [/ciclo|per[ií]odo da fatura/i], weight: 0.15, label: "ciclo" },
    { patterns: [/cart[aã]o|cr[eé]dito/i], weight: 0.15, label: "cartao" },
    { patterns: [/vencimento da fatura/i], weight: 0.15, label: "vencimento_fatura" },
  ],
  bank_statement: [
    {
      patterns: [/extrato|saldo anterior|saldo atual|saldo dispon[ií]vel/i],
      weight: 0.4,
      label: "extrato",
    },
    {
      patterns: [/\.ofx$|\.csv$/i, /lista de lan[cç]amentos|m[úu]ltiplas transa[cç][oõ]es/i],
      weight: 0.3,
      label: "multiplas_transacoes",
    },
    { patterns: [/saldo/i], weight: 0.15, label: "saldo" },
    { patterns: [/d[eé]bito|cr[eé]dito|lan[cç]amento/i], weight: 0.15, label: "lancamentos" },
  ],
  payment_receipt: [
    {
      patterns: [/pagamento realizado|pagamento confirmado|comprovante de pagamento/i],
      weight: 0.4,
      label: "pagamento_realizado",
    },
    { patterns: [/comprovante|recibo/i], weight: 0.2, label: "comprovante" },
    { patterns: [/pix|transfer[eê]ncia|ted|doc/i], weight: 0.2, label: "pix_ted" },
    { patterns: [/data do pagamento|valor pago/i], weight: 0.1, label: "data_pagamento" },
    {
      patterns: [/autentia[cç][aã]o|c[oó]digo de autentica[cç][aã]o/i],
      weight: 0.1,
      label: "autenticacao",
    },
  ],
  payment_confirmation: [
    {
      patterns: [/compra aprovada|transa[cç][aã]o aprovada|autorizada/i],
      weight: 0.4,
      label: "compra_aprovada",
    },
    {
      patterns: [/d[eé]bito autom[aá]tico agendado|d[eé]bito autom[aá]tico/i],
      weight: 0.3,
      label: "debito_automatico",
    },
    { patterns: [/confirmamos|recebemos seu pagamento/i], weight: 0.2, label: "confirmacao" },
    { patterns: [/compra|transa[cç][aã]o/i], weight: 0.1, label: "compra" },
  ],
  transaction_history: [
    {
      patterns: [/hist[oó]rico de transa[cç][oõ]es|extrato de transa[cç][oõ]es/i],
      weight: 0.5,
      label: "historico",
    },
    {
      patterns: [/movimenta[cç][oõ]es|lan[cç]amentos do per[ií]odo/i],
      weight: 0.3,
      label: "movimentacoes",
    },
    { patterns: [/per[ií]odo|de.*at[eé]/i], weight: 0.2, label: "periodo" },
  ],
  contract_or_debt: [
    { patterns: [/contrato|financiamento|empr[eé]stimo/i], weight: 0.35, label: "contrato" },
    { patterns: [/parcela|amortiza[cç][aã]o|juros/i], weight: 0.25, label: "parcela_juros" },
    { patterns: [/CET|custo efetivo total|saldo devedor/i], weight: 0.25, label: "cet_devedor" },
    { patterns: [/prazo|vencimento final/i], weight: 0.15, label: "prazo" },
  ],
  recurring_charge: [
    {
      patterns: [/cobran[cç]a recorrente|mensalidade|assinatura/i],
      weight: 0.4,
      label: "recorrente",
    },
    { patterns: [/plano|anuidade|renovação autom[aá]tica/i], weight: 0.3, label: "plano" },
    { patterns: [/todo m[eê]s|mensal/i], weight: 0.3, label: "mensal" },
  ],
  unknown: [],
};

const CRITICAL_FIELDS_BY_INTENT: Record<FinancialIntent, string[]> = {
  bill_to_pay: ["amount", "due_date", "supplier_name"],
  bill_reminder: ["amount", "due_date", "institution"],
  invoice_statement: ["amount", "due_date", "card_info"],
  bank_statement: ["date_range"],
  payment_receipt: ["amount", "payment_date"],
  payment_confirmation: ["amount"],
  transaction_history: ["date_range"],
  contract_or_debt: ["amount", "due_date", "supplier_name"],
  recurring_charge: ["amount", "supplier_name"],
  unknown: [],
};

function scoreIntent(combined: string, signals: SignalSet[]): { score: number; reasons: string[] } {
  let score = 0;
  const reasons: string[] = [];
  for (const signal of signals) {
    const matched = signal.patterns.some((p) => p.test(combined));
    if (matched) {
      score += signal.weight;
      reasons.push(signal.label);
    }
  }
  return { score: Math.min(score, 1), reasons };
}

/**
 * Classifica a intenção financeira de um texto/assunto de forma determinística.
 */
export function classifyFinancialIntent(input: ClassifierInput): ClassificationResult {
  const combined = [input.subject ?? "", input.text].join(" ");

  let bestIntent: FinancialIntent = "unknown";
  let bestScore = 0;
  let bestReasons: string[] = [];

  const candidates: FinancialIntent[] = [
    "bill_to_pay",
    "bill_reminder",
    "invoice_statement",
    "bank_statement",
    "payment_receipt",
    "payment_confirmation",
    "transaction_history",
    "contract_or_debt",
    "recurring_charge",
  ];

  for (const intent of candidates) {
    const { score, reasons } = scoreIntent(combined, SIGNALS[intent] ?? []);
    if (score > bestScore) {
      bestScore = score;
      bestIntent = intent;
      bestReasons = reasons;
    }
  }

  // Threshold mínimo de confiança para não retornar "unknown"
  if (bestScore < 0.2) {
    return {
      intent: "unknown",
      confidence: bestScore,
      reasons: [],
      missingFields: CRITICAL_FIELDS_BY_INTENT.unknown ?? [],
    };
  }

  const missingFields = CRITICAL_FIELDS_BY_INTENT[bestIntent] ?? [];

  return {
    intent: bestIntent,
    confidence: Math.round(bestScore * 100) / 100,
    reasons: bestReasons,
    missingFields,
  };
}
