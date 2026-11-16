/**
 * Partes puras do coletor do Gmail: a busca financeira e a leitura do payload.
 * Sem Bun nem banco, para o teste rodar no vitest.
 */
import type { GmailPart, GmailPayload } from "@sbf/worker-gmail-scanner/src/gmail-client";

const TERMOS = [
  "fatura",
  "boleto",
  "comprovante",
  "extrato",
  "recibo",
  '"nota fiscal"',
  "nfe",
  '"nf-e"',
  "danfe",
  "pagamento",
  "pix",
  "cobrança",
  "vencimento",
  "assinatura",
  "renovação",
  '"seu pedido"',
  "compra",
  "parcela",
  "empréstimo",
  "consignado",
  "financiamento",
  "contracheque",
  "holerite",
  '"informe de rendimentos"',
  "irpf",
  "darf",
  '"débito automático"',
  "invoice",
  "receipt",
  "subscription",
  "payment",
  '"order confirmation"',
];

/** A busca financeira: termos no assunto/corpo, ou anexo de documento. */
/**
 * Termos financeiros fora de Promoções, Fóruns e Social (newsletter também diz
 * "assinatura" e "compra" no corpo), ou qualquer e-mail com anexo de documento,
 * em qualquer categoria.
 */
export function consultaFinanceira(): string {
  const anexos = "has:attachment filename:(pdf OR xml OR ofx OR csv OR xlsx OR xls)";
  const foraDeListas = "-category:promotions -category:forums -category:social";
  return `({${TERMOS.join(" ")}} ${foraDeListas}) OR (${anexos})`;
}

export interface Anexo {
  filename: string;
  mimeType: string;
  attachmentId?: string;
  /** Anexo pequeno pode vir embutido, em base64url, sem attachmentId. */
  data?: string;
}

/** Todas as partes com nome de arquivo, em qualquer profundidade. */
export function anexosDe(payload: GmailPayload): Anexo[] {
  const anexos: Anexo[] = [];
  const visitar = (partes: GmailPart[] | undefined) => {
    for (const p of partes ?? []) {
      if (p.filename && (p.body.attachmentId || p.body.data)) {
        anexos.push({
          filename: p.filename,
          mimeType: p.mimeType,
          attachmentId: p.body.attachmentId,
          data: p.body.attachmentId ? undefined : p.body.data,
        });
      }
      visitar(p.parts);
    }
  };
  visitar(payload.parts);
  return anexos;
}

export function cabecalho(payload: GmailPayload, nome: string): string | null {
  const alvo = nome.toLowerCase();
  return payload.headers.find((h) => h.name.toLowerCase() === alvo)?.value ?? null;
}

/** Cota estourada vem como 403 "Quota exceeded"/"rateLimitExceeded", não só 429. */
export function erroTransitorio(msg: string): boolean {
  return (
    // 401: o token de acesso venceu entre a checagem e o uso; a próxima tentativa
    // renova. "sem resposta": chamada que passou do tempo limite (rede travada).
    /\((401|429|500|502|503|504)\)|fetch failed|ECONNRESET|ETIMEDOUT|sem resposta/.test(msg) ||
    (/\(403\)/.test(msg) && /quota exceeded|ratelimitexceeded|userratelimitexceeded/i.test(msg))
  );
}
