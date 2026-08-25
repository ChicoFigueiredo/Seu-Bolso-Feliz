/**
 * Financial Identity Key — chave semântica para deduplicar obrigações financeiras.
 *
 * Dois documentos diferentes sobre a mesma obrigação (lembrete de fatura,
 * PDF da fatura, boleto, comprovante) devem convergir para a MESMA obrigação
 * canônica, em vez de virarem despesas independentes.
 *
 * ── Por que uma chave só não funciona ───────────────────────────────────────
 *
 * Um boleto tem código de barras e valor. Um lembrete não tem nem um nem
 * outro. Uma fatura tem ciclo e cartão. Um comprovante tem data de pagamento.
 * Nenhuma tupla única é computável a partir dos quatro.
 *
 * A versão anterior respondeu a isso com cinco prefixos de aridades
 * diferentes (`barcode`, `docnum`, `invoice`, `reminder` e o intent cru),
 * o que garantia que jamais colidissem — o oposto exato do objetivo declarado
 * no próprio cabeçalho do arquivo.
 *
 * ── Design: emitir um CONJUNTO de chaves, casar por qualquer membro ─────────
 *
 * Toda chave usa a mesma aridade e o mesmo prefixo de versão; só o slot `kind`
 * varia. A obrigação guarda todas as chaves computáveis, e uma evidência nova
 * casa se qualquer uma delas bater.
 *
 * Regras que corrigem defeitos específicos da versão anterior:
 *
 *   - `intent` NUNCA entra em chave nenhuma. Era o maior defeito: o fallback
 *     genérico usava o intent cru como prefixo, então fatura e lembrete sobre
 *     a mesma conta eram matematicamente incapazes de colidir.
 *   - `dueDate` sai da chave `docnum`. Uma 2ª via com vencimento renegociado
 *     partia a obrigação em duas.
 *   - O período é bucketizado em `YYYY-MM`. É isso que faz um lembrete
 *     ("vence 10/04") e uma fatura ("fechamento 28/03") caírem no mesmo balde.
 *   - Chaves fracas nunca são `primary` e só devem casar contra obrigação de
 *     valor desconhecido ou dentro da tolerância — senão duas contas distintas
 *     do mesmo fornecedor no mesmo mês se fundiriam.
 */

import { createHash } from "node:crypto";

export interface FinancialIdentityInput {
  userId: string;
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

export type KeyKind =
  "barcode" | "docnum" | "card_cycle" | "supplier_period_amount" | "supplier_period";

export type KeyStrength = "strong" | "medium" | "weak";

export interface IdentityKeyEntry {
  key: string;
  kind: KeyKind;
  strength: KeyStrength;
}

export interface FinancialIdentityKeySet {
  /** Chave mais forte computável. É a que a obrigação grava como canônica. */
  primary: string | null;
  /** Demais chaves computáveis, usadas como aliases de casamento. */
  alternates: string[];
  entries: IdentityKeyEntry[];
}

const KEY_VERSION = "fik.v2";

const STRENGTH_BY_KIND: Record<KeyKind, KeyStrength> = {
  barcode: "strong",
  docnum: "strong",
  card_cycle: "strong",
  supplier_period_amount: "medium",
  supplier_period: "weak",
};

/** Ordem de precedência ao escolher a `primary`. */
const KIND_ORDER: KeyKind[] = [
  "barcode",
  "docnum",
  "card_cycle",
  "supplier_period_amount",
  "supplier_period",
];

export function normalizeName(name: string): string {
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

export function amountToCents(amount: number): number {
  return Math.round(amount * 100);
}

/**
 * Bucketiza uma data em `YYYY-MM`.
 *
 * É o que permite que documentos sobre a mesma obrigação, mas com datas
 * diferentes (fechamento vs vencimento), coincidam.
 */
function toPeriod(date: string | null | undefined): string | null {
  if (!date) return null;
  const m = /^(\d{4})-(\d{2})/.exec(date);
  return m ? `${m[1]}-${m[2]}` : null;
}

/** Toda chave tem exatamente esta forma. Aridade fixa é o que torna colisão possível. */
function hashKey(parts: {
  userId: string;
  kind: KeyKind;
  scope: string;
  period: string;
  discriminator: string;
  amount: string;
}): string {
  return createHash("sha256")
    .update(
      [
        KEY_VERSION,
        parts.userId,
        parts.kind,
        parts.scope,
        parts.period,
        parts.discriminator,
        parts.amount,
      ].join("|"),
    )
    .digest("hex")
    .slice(0, 32);
}

/**
 * Escopos de fornecedor computáveis.
 *
 * Emite CNPJ e nome quando ambos são conhecidos, para que um documento que
 * traga só o nome case com outro que traga só o CNPJ.
 */
function supplierScopes(input: FinancialIdentityInput): string[] {
  const scopes: string[] = [];
  if (input.supplierCnpj) {
    const cnpj = normalizeCnpj(input.supplierCnpj);
    if (cnpj.length >= 8) {
      scopes.push(`cnpj:${cnpj}`);
      // Raiz de 8 dígitos: filiais da mesma empresa compartilham a raiz.
      if (cnpj.length === 14) scopes.push(`cnpjroot:${cnpj.slice(0, 8)}`);
    }
  }
  const name = input.institutionName ?? input.supplierName;
  if (name) scopes.push(`name:${normalizeName(name)}`);
  return scopes;
}

/**
 * Gera o conjunto de chaves de identidade financeira.
 *
 * Retorna `{primary: null, alternates: [], entries: []}` quando não há campos
 * mínimos — nunca lança.
 */
export function buildFinancialIdentityKeys(input: FinancialIdentityInput): FinancialIdentityKeySet {
  const { userId } = input;
  const entries: IdentityKeyEntry[] = [];

  const push = (
    kind: KeyKind,
    scope: string,
    period: string | null,
    discriminator: string | null,
    amountCents: number | null,
  ) => {
    entries.push({
      key: hashKey({
        userId,
        kind,
        scope,
        period: period ?? "-",
        discriminator: discriminator ?? "-",
        amount: amountCents === null ? "-" : String(amountCents),
      }),
      kind,
      strength: STRENGTH_BY_KIND[kind],
    });
  };

  const scopes = supplierScopes(input);
  const period = toPeriod(input.dueDate ?? input.competenceDate ?? input.cycleEndDate);
  const amountCents = input.amount != null ? amountToCents(input.amount) : null;

  // ── barcode: identifica um boleto unicamente. Sem valor, sem período:
  // uma 2ª via do mesmo boleto tem o mesmo código.
  if (input.barcodeDigitableLine) {
    const barcode = normalizeBarcode(input.barcodeDigitableLine);
    if (barcode.length >= 44) push("barcode", "-", null, barcode, null);
  }

  // ── docnum: número do documento + fornecedor. SEM vencimento, de propósito:
  // renegociar a data não cria uma obrigação nova.
  if (input.documentNumber && scopes.length > 0) {
    for (const scope of scopes) push("docnum", scope, null, input.documentNumber, null);
  }

  // ── card_cycle: fatura de cartão. O discriminador é o final do cartão ou o
  // intervalo do ciclo, o que estiver disponível.
  const cycle =
    input.cycleStartDate && input.cycleEndDate
      ? `${input.cycleStartDate}_${input.cycleEndDate}`
      : null;
  if (period && scopes.length > 0 && (input.cardLast4 || cycle)) {
    for (const scope of scopes) {
      if (input.cardLast4) push("card_cycle", scope, period, `card:${input.cardLast4}`, null);
      if (cycle) push("card_cycle", scope, period, `cycle:${cycle}`, null);
    }
  }

  // ── supplier_period_amount: fornecedor + mês + valor. É a chave que faz
  // lembrete e fatura da mesma conta convergirem.
  if (scopes.length > 0 && period && amountCents !== null) {
    for (const scope of scopes) push("supplier_period_amount", scope, period, null, amountCents);
  }

  // ── supplier_period: último recurso, quando o valor é desconhecido.
  if (scopes.length > 0 && period) {
    for (const scope of scopes) push("supplier_period", scope, period, null, null);
  }

  if (entries.length === 0) {
    return { primary: null, alternates: [], entries: [] };
  }

  const rank = (e: IdentityKeyEntry) => KIND_ORDER.indexOf(e.kind);
  const sorted = [...entries].sort((a, b) => rank(a) - rank(b));

  // Uma chave fraca nunca vira primary: ela identifica "alguma conta deste
  // fornecedor neste mês", o que é fraco demais para ser a identidade canônica.
  const primaryEntry = sorted.find((e) => e.strength !== "weak") ?? null;
  const primary = primaryEntry?.key ?? null;

  return {
    primary,
    alternates: sorted.filter((e) => e.key !== primary).map((e) => e.key),
    entries: sorted,
  };
}

/**
 * @deprecated Use `buildFinancialIdentityKeys`, que devolve o conjunto completo.
 * Mantida para não quebrar chamadores existentes; devolve apenas a chave primária.
 */
export function buildFinancialIdentityKey(input: FinancialIdentityInput): string | null {
  return buildFinancialIdentityKeys(input).primary;
}
