/**
 * Dataset sintético de PDFs/comprovantes — §37 dos critérios de aceite de
 * reconciliação. É a origem que carrega bytes reais (via buildSimplePdf,
 * mesmo builder de __tests__/fixtures/documents/boleto-cemig.ts), então é
 * o veículo natural pra demonstrar a Regra 1 (match_duplicate por
 * content_hash) e preservação de evidência — as outras duas origens
 * (transações/emails) não têm bytes de arquivo, só campos estruturados.
 *
 * Determinístico: dois documentos com o mesmo texto produzem os mesmos
 * bytes e, portanto, o mesmo content_hash — sem precisar computar o hash
 * aqui (fica a cargo do teste, via computeContentHash de @sbf/operations).
 */
import { buildSimplePdf } from "../builders/pdf";
import { addDays } from "./transactions";

export interface SyntheticDocument {
  label: string;
  filename: string;
  supplierName: string;
  amountCents: number;
  dueDate: string;
  lines: string[];
  /** Rótulo do par — dois documentos com o mesmo valor aqui têm o MESMO conteúdo (duplicata real). */
  duplicateGroup?: string;
}

function comprovanteLines(args: {
  supplier: string;
  amountReais: string;
  dueDate: string;
  reference: string;
}): string[] {
  return [
    args.supplier.toUpperCase(),
    "COMPROVANTE DE COBRANCA",
    "",
    `Referencia: ${args.reference}`,
    `Vencimento: ${args.dueDate}`,
    `Valor a pagar: R$ ${args.amountReais}`,
  ];
}

function formatDateBR(iso: string): string {
  const [year, month, day] = iso.split("-");
  return `${day}/${month}/${year}`;
}

function formatAmount(cents: number): string {
  return (cents / 100).toFixed(2).replace(".", ",");
}

/**
 * 16 comprovantes únicos — metade repete fornecedor/valor das recorrências
 * de transactions.ts (pra também poderem casar via Regra 2, já que um
 * comprovante que vira draft passa pelo mesmo findReconciliationCandidates
 * que um email ou uma transação Pluggy), metade é ruído sem match.
 */
function uniqueDocuments(): SyntheticDocument[] {
  const matching = [
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, month: 0 },
    { supplier: "Cemig Distribuicao S.A.", amountCents: 24550, month: 1 },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, month: 0 },
    { supplier: "Sabesp Agua e Esgoto", amountCents: 11200, month: 1 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 0 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 1 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 2 },
    { supplier: "Condominio Edificio Aurora", amountCents: 65000, month: 3 },
  ];

  const noise = [
    "Clinica Veterinaria Bicho Feliz",
    "Escola de Idiomas Fluencia",
    "Estacionamento Rotativo Zona Azul",
    "Grafica Rapida Impressos",
    "Loja de Materiais de Construcao Forte",
    "Salao de Beleza Charme",
    "Restaurante Sabor Caseiro",
    "Locadora de Ferramentas Obra Facil",
  ];

  const items: SyntheticDocument[] = matching.map((m, idx) => {
    const dueDate = addDays("2026-03-12", m.month * 30);
    return {
      label: `documento-com-match-${idx}`,
      filename: `comprovante-${idx}.pdf`,
      supplierName: m.supplier,
      amountCents: m.amountCents,
      dueDate,
      lines: comprovanteLines({
        supplier: m.supplier,
        amountReais: formatAmount(m.amountCents),
        dueDate: formatDateBR(dueDate),
        reference: `REF-${idx}`,
      }),
    };
  });

  noise.forEach((supplier, idx) => {
    const dueDate = addDays("2026-07-01", idx * 5);
    const amountCents = 5000 + idx * 700;
    items.push({
      label: `documento-ruido-${idx}`,
      filename: `comprovante-ruido-${idx}.pdf`,
      supplierName: supplier,
      amountCents,
      dueDate,
      lines: comprovanteLines({
        supplier,
        amountReais: formatAmount(amountCents),
        dueDate: formatDateBR(dueDate),
        reference: `REF-RUIDO-${idx}`,
      }),
    });
  });

  return items;
}

/**
 * 2 pares (4 documentos): o mesmo comprovante "enviado" duas vezes — texto
 * idêntico, portanto bytes e content_hash idênticos. É o cenário real da
 * Regra 1 (match_duplicate) e o que a preservação de evidência protege:
 * as DUAS entradas em source_documents continuam existindo depois da
 * detecção, nenhuma é descartada — só a segunda inserção de draft é que
 * não deveria duplicar a obrigação.
 */
function duplicatePairs(): SyntheticDocument[] {
  const pairs = [
    { supplier: "Internet Fibra Ligacao Total", amountCents: 9990, dueDate: "2026-08-10" },
    { supplier: "Plano de Saude Bem Estar", amountCents: 45000, dueDate: "2026-08-05" },
  ];

  const items: SyntheticDocument[] = [];
  pairs.forEach((p, idx) => {
    const lines = comprovanteLines({
      supplier: p.supplier,
      amountReais: formatAmount(p.amountCents),
      dueDate: formatDateBR(p.dueDate),
      reference: `REF-DUP-${idx}`,
    });
    for (let copy = 0; copy < 2; copy++) {
      items.push({
        label: `documento-duplicado-${idx}-copia-${copy}`,
        filename: `comprovante-duplicado-${idx}-${copy}.pdf`,
        supplierName: p.supplier,
        amountCents: p.amountCents,
        dueDate: p.dueDate,
        lines,
        duplicateGroup: `dup-${idx}`,
      });
    }
  });
  return items;
}

export const UNIQUE_DOCUMENTS = uniqueDocuments();
export const DUPLICATE_DOCUMENTS = duplicatePairs();

/** Os 20 PDFs/comprovantes sintéticos do §37, em ordem estável. */
export const SYNTHETIC_DOCUMENTS: SyntheticDocument[] = [
  ...UNIQUE_DOCUMENTS,
  ...DUPLICATE_DOCUMENTS,
];

export function documentBytes(doc: SyntheticDocument): Buffer {
  return buildSimplePdf(doc.lines);
}
