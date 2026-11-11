/**
 * Dataset sintético de emails — §37 dos critérios de aceite de
 * reconciliação. Emails de cobrança são a origem mais imprecisa das três:
 * frequentemente citam fornecedor e valor mas não uma data de vencimento
 * exata ("sua fatura já está disponível"), ou citam um fornecedor/valor que
 * não existe em lugar nenhum do extrato. Por isso o pool é o melhor
 * candidato pra demonstrar correspondência aproximada e não-correspondência
 * — exatidão já é coberto por transactions.ts.
 *
 * Determinístico (sem Date.now()/Math.random()), como os demais fixtures.
 */
import { addDays } from "./transactions";

export type EmailExpectation = "match_exact" | "match_fuzzy" | "no_match";

export interface SyntheticEmail {
  label: string;
  subject: string;
  supplierNameRaw: string;
  amountCents: number | null;
  /** Vencimento explícito no corpo do email — null quando o email não cita um. */
  dueDate: string | null;
  competenceDate: string | null;
  /** O que a Regra 2/3 deve produzir quando este email vira draft. */
  expected: EmailExpectation;
}

/**
 * 15 emails que citam fornecedor + valor + data com precisão suficiente
 * pra cair em match_exact contra as transações de recorrência de
 * transactions.ts (mesmo fornecedor/valor, dentro da janela de 7 dias).
 */
function preciseEmails(): SyntheticEmail[] {
  const targets = [
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, month: 0 },
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, month: 1 },
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, month: 2 },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, month: 0 },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, month: 1 },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, month: 2 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 0 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 1 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 2 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 3 },
  ];

  return targets.map((t, idx) => {
    const dueDate = addDays("2026-03-12", t.month * 30);
    return {
      label: `email-preciso-${idx}`,
      subject: `Sua fatura ${t.supplier} está disponível`,
      supplierNameRaw: t.supplier,
      amountCents: t.amountCents,
      dueDate,
      competenceDate: dueDate,
      expected: "match_exact",
    };
  });
}

/**
 * 15 emails que citam fornecedor + valor certos mas SEM data de vencimento
 * (o email só avisa "fatura disponível", data fica só no PDF anexo) — a
 * Regra 2 não roda sem due_date, então isso testa o caminho de
 * correspondência aproximada via outro sinal (aqui: nenhum devido a falta
 * de due_date, o que é o comportamento correto — "aproximado" no sentido
 * do §37 é coberto pelas datas *dentro* da janela mas fora do dia exato,
 * ver installmentAndOffsetEmails).
 */
function noDueDateEmails(): SyntheticEmail[] {
  const suppliers = [
    "Netflix Assinaturas",
    "Spotify Brasil",
    "Amazon Prime",
    "Uber do Brasil",
    "iFood.com Agencia",
    "Supermercado Extra",
    "Farmacia Sao Paulo",
    "Academia Smart Fit",
    "Posto Ipiranga Centro",
    "Seguradora Porto Ltda",
    "Livraria Cultura",
    "Cinemark Shopping",
    "Claro Telecomunicacoes",
    "Vivo Telefonica Brasil",
    "Sky Brasil Servicos",
  ];

  return suppliers.map((supplier, idx) => ({
    label: `email-sem-vencimento-${idx}`,
    subject: `${supplier}: fatura disponível`,
    supplierNameRaw: supplier,
    amountCents: 3000 + idx * 250,
    dueDate: null,
    competenceDate: null,
    expected: "no_match",
  }));
}

/**
 * 10 emails com fornecedor/valor corretos mas data fora da janela de 7
 * dias do match_exact (>7 e <60 dias, ainda dentro da busca) —
 * correspondência aproximada de verdade (match_fuzzy).
 */
function offsetDateEmails(): SyntheticEmail[] {
  const targets = [
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, dueDate: "2026-03-30" },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, dueDate: "2026-03-28" },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, dueDate: "2026-03-25" },
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, dueDate: "2026-04-25" },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, dueDate: "2026-04-22" },
  ];

  return targets.map((t, idx) => ({
    label: `email-data-deslocada-${idx}`,
    subject: `${t.supplier}: cobrança referente ao período`,
    supplierNameRaw: t.supplier,
    amountCents: t.amountCents,
    dueDate: t.dueDate,
    competenceDate: t.dueDate,
    expected: "match_fuzzy",
  }));
}

/**
 * Emails que citam fornecedor/valor que não existem em lugar nenhum do
 * extrato — não-correspondência de verdade (score insuficiente), não só
 * ausência de due_date.
 */
function unknownEmails(): SyntheticEmail[] {
  const targets = [
    { supplier: "Companhia Fantasma de Cobranca", amountCents: 9999 },
    { supplier: "Instituto Nao Cadastrado", amountCents: 15000 },
    { supplier: "Servico Desconhecido LTDA", amountCents: 4200 },
    { supplier: "Fornecedor Nunca Visto SA", amountCents: 7700 },
    { supplier: "Cobranca Sem Historico ME", amountCents: 12300 },
    { supplier: "Cemig Distribuicao S.A.", amountCents: 999999 },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 500000 },
    { supplier: "Condominio Edificio Aurora", amountCents: 1 },
    { supplier: "Loja Inexistente no Extrato", amountCents: 33000 },
    { supplier: "Assinatura Nunca Cobrada", amountCents: 5600 },
  ];

  return targets.map((t, idx) => ({
    label: `email-desconhecido-${idx}`,
    subject: `${t.supplier}: cobrança`,
    supplierNameRaw: t.supplier,
    amountCents: t.amountCents,
    dueDate: "2026-03-15",
    competenceDate: "2026-03-15",
    expected: "no_match",
  }));
}

export const PRECISE_EMAILS = preciseEmails();
export const NO_DUE_DATE_EMAILS = noDueDateEmails();
export const OFFSET_DATE_EMAILS = offsetDateEmails();
export const UNKNOWN_EMAILS = unknownEmails();

/** Os 40 emails sintéticos do §37, em ordem estável. */
export const SYNTHETIC_EMAILS: SyntheticEmail[] = [
  ...PRECISE_EMAILS,
  ...NO_DUE_DATE_EMAILS,
  ...OFFSET_DATE_EMAILS,
  ...UNKNOWN_EMAILS,
];
