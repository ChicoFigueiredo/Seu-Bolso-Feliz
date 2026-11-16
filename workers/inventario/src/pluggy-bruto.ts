/**
 * Leitura do objeto bruto de uma transação da Pluggy (API v2). O app guarda só
 * descrição, valor e estabelecimento; o bruto traz quem pagou e quem recebeu
 * (paymentData), a parcela e a data original da compra no cartão
 * (creditCardMetadata) e o CNPJ do estabelecimento (merchant).
 *
 * Puro: sem banco nem Bun, para o vitest. Campos ausentes viram null.
 */

interface Parte {
  name?: string | null;
  documentNumber?: { type?: string | null; value?: string | null } | null;
}

export interface TransacaoPluggyBruta {
  id: string;
  type?: "DEBIT" | "CREDIT" | string;
  amount?: number;
  date?: string;
  description?: string | null;
  paymentData?: {
    payer?: Parte | null;
    receiver?: Parte | null;
    paymentMethod?: string | null;
  } | null;
  creditCardMetadata?: {
    installmentNumber?: number | null;
    totalInstallments?: number | null;
    totalAmount?: number | null;
    purchaseDate?: string | null;
    billId?: string | null;
  } | null;
  merchant?: { name?: string | null; businessName?: string | null; cnpj?: string | null } | null;
}

export interface Enriquecimento {
  /** Quem recebeu (saída) ou quem pagou (entrada). */
  contraparte: string | null;
  contraparte_doc: string | null;
  meio: string | null;
  merchant_cnpj: string | null;
  data_compra: string | null;
  parcela_numero: number | null;
  parcela_total: number | null;
  valor_total_compra_centavos: number | null;
  fatura_id: string | null;
}

function limpo(texto: string | null | undefined): string | null {
  const t = texto?.replace(/\s+/g, " ").trim();
  return t ? t : null;
}

function centavos(valor: number | null | undefined): number | null {
  return typeof valor === "number" && Number.isFinite(valor)
    ? Math.round(Math.abs(valor) * 100)
    : null;
}

export function enriquecer(tx: TransacaoPluggyBruta): Enriquecimento {
  const saiu = tx.type === "DEBIT" || (tx.type == null && (tx.amount ?? 0) < 0);
  const outra = saiu ? tx.paymentData?.receiver : tx.paymentData?.payer;
  const cc = tx.creditCardMetadata;
  const parcelada = (cc?.totalInstallments ?? 0) > 1;
  return {
    contraparte: limpo(outra?.name),
    contraparte_doc: limpo(outra?.documentNumber?.value),
    meio: limpo(tx.paymentData?.paymentMethod),
    merchant_cnpj: limpo(tx.merchant?.cnpj),
    data_compra: cc?.purchaseDate ? cc.purchaseDate.slice(0, 10) : null,
    parcela_numero: parcelada ? (cc?.installmentNumber ?? null) : null,
    parcela_total: parcelada ? (cc?.totalInstallments ?? null) : null,
    valor_total_compra_centavos: parcelada ? centavos(cc?.totalAmount) : null,
    fatura_id: limpo(cc?.billId),
  };
}

/** Fatura de cartão da API /bills (campos que usamos). */
export interface FaturaPluggyBruta {
  id: string;
  dueDate?: string | null;
  totalAmount?: number | null;
  minimumPaymentAmount?: number | null;
  closeDate?: string | null;
}

export function registroDaFatura(f: FaturaPluggyBruta): {
  external_ref: string;
  data: string;
  valor_centavos: number;
  descricao: string;
} | null {
  if (!f.dueDate || typeof f.totalAmount !== "number") return null;
  const minimo = centavos(f.minimumPaymentAmount);
  return {
    external_ref: `pluggy:bill_${f.id}`,
    data: f.dueDate.slice(0, 10),
    valor_centavos: -Math.round(Math.abs(f.totalAmount) * 100),
    descricao: `Fatura do cartão${minimo != null ? ` (mínimo ${(minimo / 100).toFixed(2)})` : ""}`,
  };
}
