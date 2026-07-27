/**
 * Composição da query do Gmail.
 *
 * Puro e testável de propósito: `--query`, `--from-date` e `--to-date` eram
 * aceitos pela CLI e não chegavam a lugar nenhum — `options.query` era
 * atribuído e nunca mais lido, e as datas nem sequer tinham `case` no parser.
 * Aqui a composição vira uma função com testes, em vez de uma esperança.
 */

export interface QueryParts {
  /** Query livre do usuário, ex.: `from:nubank has:attachment`. */
  query?: string | null;
  /** Início do período, `YYYY-MM-DD` (inclusivo). */
  fromDate?: string | null;
  /** Fim do período, `YYYY-MM-DD` (inclusivo). */
  toDate?: string | null;
}

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * Converte `YYYY-MM-DD` para o formato `YYYY/MM/DD` que o Gmail exige.
 * Devolve null para entrada inválida, para que uma data malformada não vire
 * silenciosamente um filtro inesperado.
 */
export function toGmailDate(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const m = ISO_DATE.exec(iso.trim());
  if (!m) return null;
  return `${m[1]}/${m[2]}/${m[3]}`;
}

/**
 * Soma um dia a uma data ISO.
 *
 * O operador `before:` do Gmail é EXCLUSIVO. Sem este ajuste, `--to-date
 * 2026-04-30` perderia todas as mensagens do próprio dia 30 — um erro que só
 * apareceria como "faltam alguns e-mails" muito depois.
 */
export function addOneDay(iso: string): string | null {
  const m = ISO_DATE.exec(iso.trim());
  if (!m) return null;
  const d = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])));
  d.setUTCDate(d.getUTCDate() + 1);
  return d.toISOString().slice(0, 10);
}

/**
 * Compõe a query final. Devolve string vazia quando não há nenhum filtro,
 * caso em que o chamador deve buscar apenas por label.
 */
export function buildGmailQuery(parts: QueryParts): string {
  const terms: string[] = [];

  const user = parts.query?.trim();
  if (user) terms.push(user);

  const after = toGmailDate(parts.fromDate);
  if (after) terms.push(`after:${after}`);

  if (parts.toDate) {
    const exclusiveEnd = addOneDay(parts.toDate);
    const before = toGmailDate(exclusiveEnd);
    if (before) terms.push(`before:${before}`);
  }

  return terms.join(" ");
}
