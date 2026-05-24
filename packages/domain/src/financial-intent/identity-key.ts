/**
 * Financial Identity Key — chave semântica para deduplicar obrigações financeiras.
 *
 * Dois documentos diferentes (ex: lembrete de fatura + boleto) sobre a mesma
 * obrigação devem produzir a mesma chave para serem agrupados como evidências
 * do mesmo item financeiro, não como duas despesas independentes.
 */

import type { FinancialIntent } from "./types";
import { createHash } from "node:crypto";

export interface FinancialIdentityInput {
  userId: string;
  intent: FinancialIntent;
  supplierName?: string | null;
  supplierCnpj?: string | null;
  institutionName?: string | null;
  financialProductHint?: string | null;
  amount?: number | null;
  dueDate?: string | null;
  competenceDate?: string | null;
  cycleStartDate?: string | null;
  cycleEndDate?: string | null;
  documentNumber?: string | null;
  barcodeDigitableLine?: string | null;
  cardLast4?: string | null;
}

function normalizeName(name: string): string {
  return name
    .toLowerCase()
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_|_$/g, "");
}

function normalizeCnpj(cnpj: string): string {
  return cnpj.replace(/\D/g, "");
}

function normalizeBarcode(barcode: string): string {
  return barcode.replace(/\D/g, "");
}

function amountToCents(amount: number): number {
  return Math.round(amount * 100);
}

function hashKey(parts: string[]): string {
  return createHash("sha256").update(parts.join("|")).digest("hex").slice(0, 32);
}

/**
 * Gera uma chave de identidade financeira semântica.
 * Retorna null se não há campos mínimos para identificar a obrigação.
 */
export function buildFinancialIdentityKey(input: FinancialIdentityInput): string | null {
  const { userId, intent } = input;

  // Sinal forte 1: linha digitável/código de barras (identifica boleto unicamente)
  if (input.barcodeDigitableLine) {
    const barcode = normalizeBarcode(input.barcodeDigitableLine);
    if (barcode.length >= 44) {
      return hashKey([userId, "barcode", barcode]);
    }
  }

  // Sinal forte 2: número de documento + fornecedor + vencimento
  if (input.documentNumber && (input.supplierName || input.supplierCnpj) && input.dueDate) {
    const supplier = input.supplierCnpj
      ? normalizeCnpj(input.supplierCnpj)
      : normalizeName(input.supplierName!);
    return hashKey([userId, "docnum", input.documentNumber, supplier, input.dueDate]);
  }

  // Fatura de cartão: instituição/produto + vencimento + valor + ciclo ou card_last4
  if (intent === "invoice_statement") {
    const institution = input.institutionName
      ? normalizeName(input.institutionName)
      : input.supplierName
        ? normalizeName(input.supplierName)
        : null;
    const product = input.financialProductHint ? normalizeName(input.financialProductHint) : null;
    const dueDate = input.dueDate ?? input.competenceDate;
    const card = input.cardLast4 ?? null;
    const cycle =
      input.cycleStartDate && input.cycleEndDate
        ? `${input.cycleStartDate}_${input.cycleEndDate}`
        : null;

    if (institution && dueDate && (card || cycle)) {
      const amountPart = input.amount != null ? String(amountToCents(input.amount)) : "?";
      const parts = [
        userId,
        "invoice",
        institution,
        product ?? "",
        dueDate,
        amountPart,
        card ?? cycle ?? "",
      ];
      return hashKey(parts);
    }
  }

  // Lembrete de fatura: instituição + valor + vencimento + intent
  if (intent === "bill_reminder") {
    const institution = input.institutionName
      ? normalizeName(input.institutionName)
      : input.supplierName
        ? normalizeName(input.supplierName)
        : null;
    if (institution && input.dueDate && input.amount != null) {
      return hashKey([
        userId,
        "reminder",
        institution,
        input.dueDate,
        String(amountToCents(input.amount)),
      ]);
    }
  }

  // Genérico: fornecedor + vencimento + valor + intent
  const supplier = input.supplierCnpj
    ? normalizeCnpj(input.supplierCnpj)
    : input.supplierName
      ? normalizeName(input.supplierName)
      : null;
  const dueDate = input.dueDate ?? input.competenceDate;
  if (supplier && dueDate && input.amount != null) {
    return hashKey([userId, intent, supplier, dueDate, String(amountToCents(input.amount))]);
  }

  return null;
}
