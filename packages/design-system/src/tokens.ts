/**
 * Nomes dos temas do Seu Bolso Feliz.
 *
 * Cinco claros e dois escuros. `salvia` e o tema canonico — e o que o
 * `:root` de tokens.css aplica quando nenhum tema e declarado.
 */
export const SBF_THEMES = ["salvia", "ocre", "azul", "linho", "menta", "noite", "grafite"] as const;

export type SbfThemeName = (typeof SBF_THEMES)[number];

/** Temas escuros — útil para decidir o `color-scheme` do host. */
export const SBF_DARK_THEMES: readonly SbfThemeName[] = ["noite", "grafite"];

/**
 * Papel semântico de uma cor de estado. Nenhum componente recebe cor crua:
 * recebe o papel, e o tema decide o pigmento.
 *
 * - `neutral` — informação sem carga. O padrão.
 * - `primary` — direcao, progresso, confirmação.
 * - `secondary` — informação e IA (azul ardosia).
 * - `accent` — conquista e meta (dourado fosco), em doses pequenas.
 * - `success` — resultado positivo consolidado.
 * - `warn` — compromisso próximo. Ambar, nunca vermelho.
 * - `danger` — risco real: atraso, falha, ação urgente.
 */
export type SbfTone =
  "neutral" | "primary" | "secondary" | "accent" | "success" | "warn" | "danger";

/** Fundo, texto e borda de cada tom, em tokens. */
export const toneStyles: Record<SbfTone, { bg: string; fg: string; line: string }> = {
  neutral: { bg: "var(--muted)", fg: "var(--text-3)", line: "var(--line)" },
  primary: { bg: "var(--primary-soft)", fg: "var(--primary-ink)", line: "var(--primary-line)" },
  secondary: {
    bg: "var(--secondary-soft)",
    fg: "var(--secondary-ink)",
    line: "var(--secondary-line)",
  },
  accent: { bg: "var(--accent-soft)", fg: "var(--accent-ink)", line: "var(--accent-soft)" },
  success: { bg: "var(--primary-soft)", fg: "var(--success)", line: "var(--primary-line)" },
  warn: { bg: "var(--warn-soft)", fg: "var(--warn)", line: "var(--warn-line)" },
  danger: { bg: "var(--warn-soft)", fg: "var(--danger)", line: "var(--warn-line)" },
};

/** Tipografia base aplicada na raiz de todo componente exportado. */
export const baseType = {
  fontFamily: "var(--sbf-font-sans)",
  color: "var(--text)",
} as const;

/** Numeros monetarios sempre alinham: mesma largura por digito. */
export const tabular = { fontVariantNumeric: "tabular-nums" } as const;

/**
 * Formata um valor em Real brasileiro.
 *
 * Passar `cents: false` (padrão) arredonda para o real inteiro — a leitura
 * de painel raramente precisa de centavos e eles poluem a varredura rapida.
 */
export function formatBRL(value: number, opts: { cents?: boolean } = {}): string {
  const digits = opts.cents ? 2 : 0;
  return new Intl.NumberFormat("pt-BR", {
    style: "currency",
    currency: "BRL",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

/** Data curta pt-BR — `22/08/2026`. */
export function formatDate(value: Date | string): string {
  const d = typeof value === "string" ? new Date(value) : value;
  return d.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit", year: "numeric" });
}
