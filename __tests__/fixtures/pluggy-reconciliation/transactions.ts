/**
 * Dataset sintético de transações bancárias — §37 dos critérios de aceite
 * de reconciliação (docs/prompts/2026-08-24-prompt-integracao-pluggy.md).
 *
 * 100 transações no total: um núcleo de cenários com resultado esperado
 * conhecido (recorrência, parcelamento, conflito, ambíguo — as tags do §37,
 * que cruzam o pool em vez de serem uma categoria à parte) mais ruído
 * genérico representando volume realista sem match algum. Determinístico
 * (sem Date.now()/Math.random()) para o teste de integração ser reprodutível.
 */

/** Soma `days` a uma data ISO (YYYY-MM-DD) sem depender do relógio real. */
export function addDays(iso: string, days: number): string {
  const d = new Date(`${iso}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().split("T")[0]!;
}

export type TransactionScenario = "recorrencia" | "parcelamento" | "conflito" | "ambiguo" | null;

export interface SyntheticTransaction {
  /** Rótulo estável para referência nos asserts — não vai pro banco. */
  label: string;
  supplierName: string;
  amountCents: number;
  eventDate: string;
  scenario: TransactionScenario;
  /** Agrupa transações do mesmo cenário (ex.: as 5 parcelas de uma compra). */
  scenarioGroup?: string;
}

const NOISE_SUPPLIERS = [
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
];

/** LCG determinístico — mesma saída sempre, sem depender de Math.random(). */
function pseudoRandom(seed: number): number {
  return ((seed * 9301 + 49297) % 233280) / 233280;
}

function noiseTransaction(index: number): SyntheticTransaction {
  const supplier = NOISE_SUPPLIERS[index % NOISE_SUPPLIERS.length]!;
  const amountCents = 1500 + Math.floor(pseudoRandom(index + 1) * 48500);
  const eventDate = addDays("2026-01-05", index * 3);
  return {
    label: `noise-${index}`,
    supplierName: supplier,
    amountCents,
    eventDate,
    scenario: null,
  };
}

/**
 * 15 recorrências: mesmo fornecedor/valor todo mês, ao longo de vários
 * meses — o formato que a Regra 3 (match_recurrence) espera encontrar.
 */
function recurrenceTransactions(): SyntheticTransaction[] {
  const recurringSuppliers = [
    { name: "Cemig Distribuicao S.A.", amountCents: 24550 },
    { name: "Sabesp Agua e Esgoto", amountCents: 11200 },
    { name: "Condominio Edificio Aurora", amountCents: 65000 },
  ];

  const items: SyntheticTransaction[] = [];
  for (const supplier of recurringSuppliers) {
    for (let month = 0; month < 5 && items.length < 15; month++) {
      items.push({
        label: `recorrencia-${supplier.name}-m${month}`,
        supplierName: supplier.name,
        amountCents: supplier.amountCents,
        eventDate: addDays("2026-03-10", month * 30),
        scenario: "recorrencia",
        scenarioGroup: supplier.name,
      });
    }
  }
  return items.slice(0, 15);
}

/**
 * 10 parcelamentos: uma compra dividida em N parcelas do mesmo valor,
 * ~30 dias entre si — cada parcela deve casar com sua própria transação,
 * nunca com a parcela vizinha (mesmo valor, data diferente o bastante pra
 * sair da janela de 7 dias do match_exact, mas perto o bastante pra tentar
 * confundir o matcher — é o teste de prevenção de falso positivo).
 */
function installmentTransactions(): SyntheticTransaction[] {
  const purchases = [
    { supplier: "Magazine Luiza Parcelado", installments: 5, amountCents: 18000 },
    { supplier: "Casas Bahia Parcelado", installments: 5, amountCents: 24000 },
  ];

  const items: SyntheticTransaction[] = [];
  for (const purchase of purchases) {
    for (let n = 0; n < purchase.installments; n++) {
      items.push({
        label: `parcelamento-${purchase.supplier}-${n + 1}-de-${purchase.installments}`,
        supplierName: purchase.supplier,
        amountCents: purchase.amountCents,
        eventDate: addDays("2026-02-15", n * 30),
        scenario: "parcelamento",
        scenarioGroup: purchase.supplier,
      });
    }
  }
  return items;
}

/**
 * 10 conflitos: mesmo valor e mesma data, fornecedor diferente — a Regra 2
 * filtra por supplier_id, então um draft do fornecedor A nunca pode casar
 * com a transação do fornecedor B só porque valor/data coincidem.
 */
function conflictTransactions(): SyntheticTransaction[] {
  const pairs = [
    { a: "Claro Telecomunicacoes", b: "Vivo Telefonica Brasil", amountCents: 8990 },
    { a: "Sky Brasil Servicos", b: "Net Servicos de Comunicacao", amountCents: 12990 },
    { a: "Enel Distribuicao SP", b: "Light Servicos de Eletricidade", amountCents: 31000 },
    { a: "Farmacia Pague Menos", b: "Drogasil S.A.", amountCents: 4550 },
    { a: "Padaria Pao Dourado", b: "Confeitaria Doce Sabor", amountCents: 2300 },
  ];

  const items: SyntheticTransaction[] = [];
  pairs.forEach((pair, idx) => {
    const eventDate = addDays("2026-05-01", idx * 4);
    items.push({
      label: `conflito-${idx}-a`,
      supplierName: pair.a,
      amountCents: pair.amountCents,
      eventDate,
      scenario: "conflito",
      scenarioGroup: `conflito-${idx}`,
    });
    items.push({
      label: `conflito-${idx}-b`,
      supplierName: pair.b,
      amountCents: pair.amountCents,
      eventDate,
      scenario: "conflito",
      scenarioGroup: `conflito-${idx}`,
    });
  });
  return items;
}

/**
 * 10 ambíguos: duas transações do MESMO fornecedor, valores próximos
 * (dentro dos 5% de tolerância) e datas dentro da janela de 7 dias — os
 * dois viram candidato de match_exact pro mesmo draft, então
 * `candidates.length > 1` com scores próximos é o sinal de ambiguidade que
 * o motor não resolve sozinho (por decisão explícita: fica pro humano).
 */
function ambiguousTransactions(): SyntheticTransaction[] {
  const suppliers = [
    "Assistencia Tecnica Rapida",
    "Oficina Mecanica Silva",
    "Consultoria Financeira Prime",
    "Clinica Odontologica Sorriso",
    "Pet Shop Amigo Fiel",
  ];

  const items: SyntheticTransaction[] = [];
  suppliers.forEach((supplier, idx) => {
    const baseDate = addDays("2026-06-01", idx * 6);
    const baseAmountCents = 10000 + idx * 500;
    items.push({
      label: `ambiguo-${idx}-primeira`,
      supplierName: supplier,
      amountCents: baseAmountCents,
      eventDate: baseDate,
      scenario: "ambiguo",
      scenarioGroup: `ambiguo-${idx}`,
    });
    items.push({
      label: `ambiguo-${idx}-segunda`,
      supplierName: supplier,
      amountCents: baseAmountCents + 50,
      eventDate: addDays(baseDate, 2),
      scenario: "ambiguo",
      scenarioGroup: `ambiguo-${idx}`,
    });
  });
  return items;
}

export const RECURRENCE_TRANSACTIONS = recurrenceTransactions();
export const INSTALLMENT_TRANSACTIONS = installmentTransactions();
export const CONFLICT_TRANSACTIONS = conflictTransactions();
export const AMBIGUOUS_TRANSACTIONS = ambiguousTransactions();

const SCENARIO_COUNT =
  RECURRENCE_TRANSACTIONS.length +
  INSTALLMENT_TRANSACTIONS.length +
  CONFLICT_TRANSACTIONS.length +
  AMBIGUOUS_TRANSACTIONS.length;

const NOISE_COUNT = 100 - SCENARIO_COUNT;

export const NOISE_TRANSACTIONS = Array.from({ length: NOISE_COUNT }, (_, i) =>
  noiseTransaction(i),
);

/** As 100 transações bancárias sintéticas do §37, em ordem estável. */
export const SYNTHETIC_TRANSACTIONS: SyntheticTransaction[] = [
  ...RECURRENCE_TRANSACTIONS,
  ...INSTALLMENT_TRANSACTIONS,
  ...CONFLICT_TRANSACTIONS,
  ...AMBIGUOUS_TRANSACTIONS,
  ...NOISE_TRANSACTIONS,
];
