/**
 * Que dia é hoje, para quem está usando o app.
 *
 * `new Date().toISOString().slice(0, 10)` parece a forma óbvia de escrever a
 * data de hoje em `yyyy-mm-dd`, e está errada em dois sentidos ao mesmo tempo:
 *
 *  - **No navegador do usuário** (UTC−3), das 21h à meia-noite o UTC já virou o
 *    dia seguinte. Nessas três horas, todo dia, "Hoje" vira "Ontem", o
 *    compromisso de amanhã vira "Hoje" e conta que vence hoje aparece como
 *    vencida. Foi assim que este arquivo nasceu: às 22h37 o código dizia 31/08
 *    enquanto o relógio de São Paulo dizia 30/08.
 *  - **No servidor** (Vercel roda em UTC), o relógio local *é* UTC — então
 *    trocar por "componentes locais da data" conserta o cliente e mantém o
 *    servidor errado, e ainda faz os dois discordarem, o que quebra a
 *    hidratação do React.
 *
 * A única resposta que vale nos dois lados é **fixar o fuso**. O app é de uma
 * pessoa só, no Brasil, e o dinheiro dela vence em horário de Brasília; o dia
 * financeiro é o de São Paulo, não o de onde a função foi executada.
 */

/** O fuso em que o dinheiro deste app acontece. */
export const FUSO = "America/Sao_Paulo";

// `en-CA` formata como `yyyy-mm-dd`, que é exatamente o formato que o Postgres
// usa em `date` — evita remontar a string a partir das partes.
const formatador = new Intl.DateTimeFormat("en-CA", {
  timeZone: FUSO,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
});

/** Um instante → a data `yyyy-mm-dd` que ele representa em São Paulo. */
export function paraIso(instante: Date | number | string): string {
  const d = instante instanceof Date ? instante : new Date(instante);
  return formatador.format(d);
}

/** A data de hoje em São Paulo, `yyyy-mm-dd`. */
export function hojeIso(): string {
  return formatador.format(new Date());
}

/**
 * Soma dias a uma data `yyyy-mm-dd` e devolve outra data `yyyy-mm-dd`.
 *
 * O meio-dia no meio da conta não é enfeite: somar dias a partir da
 * meia-noite atravessa a fronteira do dia em qualquer mudança de horário, e o
 * resultado erra por um. A partir do meio-dia sobram doze horas de folga dos
 * dois lados.
 */
export function somaDias(iso: string, dias: number): string {
  const base = new Date(`${iso}T12:00:00`);
  base.setDate(base.getDate() + dias);
  const a = base.getFullYear();
  const m = String(base.getMonth() + 1).padStart(2, "0");
  const d = String(base.getDate()).padStart(2, "0");
  return `${a}-${m}-${d}`;
}
