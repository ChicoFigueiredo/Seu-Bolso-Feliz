/**
 * Rótulos em português para os valores das CHECK constraints de `liabilities`
 * e `liability_installments` (ver `packages/db/schema.sql`).
 *
 * As opções de cada `<select>` do formulário são geradas a partir destes
 * mapas (`Object.entries(...)`), nunca digitadas soltas num JSX — assim o
 * valor que o `<select>` envia nunca diverge do que o banco aceita. Já houve
 * bug neste projeto de dropdown oferecendo um valor que a constraint recusa.
 */

export const TIPO_DIVIDA_LABEL: Record<string, string> = {
  personal_loan: "Empréstimo pessoal",
  mortgage: "Financiamento imobiliário",
  overdraft: "Cheque especial",
  installment_plan: "Parcelamento",
  other: "Outro",
};

export const STATUS_DIVIDA_LABEL: Record<string, string> = {
  active: "Ativa",
  paid_off: "Quitada",
  renegotiated: "Renegociada",
  defaulted: "Inadimplente",
};

export const TIPO_TAXA_LABEL: Record<string, string> = {
  monthly: "Mensal",
  annual: "Anual",
};

export const SISTEMA_AMORTIZACAO_LABEL: Record<string, string> = {
  sac: "SAC (parcela decrescente)",
  price: "Price (parcela fixa)",
  mixed: "Misto",
  other: "Outro",
  none: "Nenhum",
};

export const STATUS_PARCELA_LABEL: Record<string, string> = {
  pending: "Pendente",
  paid: "Paga",
  partial: "Parcial",
  overdue: "Atrasada",
  waived: "Dispensada",
};

export const SITUACAO_PARCELA_LABEL: Record<string, string> = {
  paga: "Paga",
  parcial: "Parcial",
  dispensada: "Dispensada",
  atrasada: "Atrasada",
  a_vencer: "A vencer",
};

/** Tom (ver `SbfTone` de `@sbf/design-system`) de cada situação de parcela — a cor é sempre o segundo canal, nunca o único. */
export const SITUACAO_PARCELA_TOM: Record<
  string,
  "success" | "secondary" | "neutral" | "danger" | "warn"
> = {
  paga: "success",
  parcial: "secondary",
  dispensada: "neutral",
  atrasada: "danger",
  a_vencer: "neutral",
};

export const SITUACAO_PARCELA_ICONE: Record<string, string> = {
  paga: "check_circle",
  parcial: "adjust",
  dispensada: "block",
  atrasada: "error",
  a_vencer: "schedule",
};
