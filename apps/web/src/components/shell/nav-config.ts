/**
 * Configuração da navegação principal do Seu Bolso Feliz.
 *
 * A ordem e o agrupamento aqui são decisão de produto (replicam o que já está
 * documentado nos previews de `@sbf/design-system`, .design-sync/previews/AppSidebar.tsx
 * e BottomNav.tsx): Hoje, Movimentações, Contas & Cartões, Dívidas, Recorrentes,
 * Projetos & Centros, Documentos, Inteligência — e, abaixo, Conexões,
 * Configurações, Avançado. Rotas técnicas de ingestão não ocupam navegação
 * principal.
 *
 * Este módulo não importa React de propósito: é só dado e funções puras, para
 * poder ser testado e reutilizado tanto no shell desktop quanto no mobile.
 */

export interface NavEntry {
  /** Chave estável, comparada com `active` nos componentes do design system. */
  id: string;
  label: string;
  /** Glifo Material Symbols Rounded. */
  icon: string;
  /** Para onde navegar. Vazio quando o item abre uma folha em vez de rota. */
  href: string;
}

/** Navegação principal do desktop (`AppSidebar`), na ordem da arquitetura de informação. */
export const PRIMARY_NAV: NavEntry[] = [
  { id: "hoje", label: "Hoje", icon: "today", href: "/dashboard/hoje" },
  {
    id: "movimentacoes",
    label: "Movimentações",
    icon: "receipt_long",
    href: "/dashboard/movimentacoes",
  },
  {
    id: "contas",
    label: "Contas & Cartões",
    icon: "account_balance",
    href: "/dashboard/contas",
  },
  { id: "dividas", label: "Dívidas", icon: "trending_down", href: "/dashboard/dividas" },
  { id: "recorrentes", label: "Recorrentes", icon: "autorenew", href: "/dashboard/recorrentes" },
  {
    id: "centros",
    label: "Projetos & Centros",
    icon: "workspaces",
    href: "/dashboard/centros",
  },
  { id: "documentos", label: "Documentos", icon: "folder_open", href: "/dashboard/documentos" },
  {
    id: "inteligencia",
    label: "Inteligência",
    icon: "auto_awesome",
    href: "/dashboard/inteligencia",
  },
];

/** Rodapé do desktop — itens de segunda ordem, fora da navegação financeira. */
export const SECONDARY_NAV: NavEntry[] = [
  { id: "conexoes", label: "Conexões", icon: "sync_alt", href: "/dashboard/conexoes" },
  { id: "config", label: "Configurações", icon: "settings", href: "/dashboard/settings" },
  { id: "avancado", label: "Avançado", icon: "tune", href: "/dashboard/settings#avancado" },
];

/** Sair não é item de arquitetura de informação — é a única ação que precisa
 * sobreviver à troca de shell, então ganha tratamento especial no `onNavigate`
 * de quem consome este módulo (ver AppShell). */
export const LOGOUT_ID = "logout";
export const LOGOUT_ENTRY: NavEntry = { id: LOGOUT_ID, label: "Sair", icon: "logout", href: "" };

/**
 * Abas do mobile (`BottomNav`). Quatro abas + ação central: o `+` é inserido
 * pelo próprio componente entre a segunda e a terceira aba.
 */
export const MOBILE_TABS: NavEntry[] = [
  { id: "hoje", label: "Hoje", icon: "today", href: "/dashboard/hoje" },
  {
    id: "movimentacoes",
    label: "Movimentos",
    icon: "receipt_long",
    href: "/dashboard/movimentacoes",
  },
  { id: "centros", label: "Projetos", icon: "workspaces", href: "/dashboard/centros" },
  { id: "mais", label: "Mais", icon: "more_horiz", href: "" },
];

/** Itens que ficam de fora dos quatro tabs mobile, mas continuam alcançáveis
 * pela folha "Mais" — o resto da navegação principal. */
export const MORE_SHEET_PRIMARY: NavEntry[] = PRIMARY_NAV.filter(
  (item) => !MOBILE_TABS.some((tab) => tab.id === item.id),
);

/** Destino do botão de ação rápida (TopBar "Registrar" e o `+` do mobile).
 * Aponta para a tela de criação que já existe hoje; quando a fase de
 * Movimentações ganhar sua própria rota de criação, só este valor muda. */
export const QUICK_ADD_HREF = "/dashboard/transactions/new";

/**
 * Rotas antigas que ainda estão no ar, não têm redirect e não entraram na
 * navegação nova — mas o TopBar/MobileHeader ainda precisam de um título
 * decente quando alguém está nelas.
 */
const LEGACY_TITLES: NavEntry[] = [
  {
    id: "legacy-documents",
    label: "Documentos",
    icon: "folder_open",
    href: "/dashboard/documents",
  },
  {
    id: "legacy-suppliers",
    label: "Fornecedores",
    icon: "storefront",
    href: "/dashboard/suppliers",
  },
  { id: "legacy-reports", label: "Relatórios", icon: "bar_chart", href: "/dashboard/reports" },
  { id: "legacy-import", label: "Importar", icon: "upload_file", href: "/dashboard/import" },
];

/** Título de fallback para qualquer rota fora dos mapas acima (ex.: a antiga
 * home em `/dashboard`, ainda não migrada). */
const FALLBACK_TITLE = "Seu Bolso Feliz";

/** Entradas usadas para casar pathname → seção. Ignora âncoras (`#...`): elas
 * não aparecem em `usePathname()`, então "Configurações" e "Avançado"
 * disputariam o mesmo caminho — o anchor não tem correspondência própria. */
const MATCH_ENTRIES: NavEntry[] = [...PRIMARY_NAV, ...SECONDARY_NAV, ...LEGACY_TITLES].filter(
  (entry) => entry.href && !entry.href.includes("#"),
);

function bestMatch(pathname: string): NavEntry | undefined {
  const candidates = MATCH_ENTRIES.filter(
    (entry) => pathname === entry.href || pathname.startsWith(`${entry.href}/`),
  );
  if (candidates.length === 0) return undefined;
  // Caminho mais longo = correspondência mais específica.
  return candidates.reduce((best, entry) => (entry.href.length > best.href.length ? entry : best));
}

/** `id` do item de navegação (sidebar ou tabs) ativo para este caminho, ou
 * `undefined` quando a rota não pertence a nenhuma seção mapeada. */
export function resolveActiveId(pathname: string): string | undefined {
  return bestMatch(pathname)?.id;
}

/** Título de tela para TopBar/MobileHeader. Nunca vazio. */
export function resolveTitle(pathname: string): string {
  return bestMatch(pathname)?.label ?? FALLBACK_TITLE;
}

/** "Bom dia" / "Boa tarde" / "Boa noite" a partir da hora local (0–23). */
export function periodOfDay(hour: number): string {
  if (hour < 12) return "Bom dia";
  if (hour < 18) return "Boa tarde";
  return "Boa noite";
}

function capitalize(word: string): string {
  return word ? word.charAt(0).toUpperCase() + word.slice(1) : word;
}

/** Primeiro nome, derivado de `name` quando existe ou do usuário do e-mail
 * (`fran.fig@gmail.com` → `Fran`). Nunca fabrica um nome — só formata o que
 * já existe. */
export function firstNameOf(name: string | null, email: string): string {
  const trimmed = name?.trim();
  if (trimmed) return trimmed.split(/\s+/)[0]!;
  const local = email.split("@")[0] ?? email;
  const first = local.split(/[._-]+/).filter(Boolean)[0];
  return first ? capitalize(first) : email;
}

/** Nome completo para as iniciais do `Avatar`. Mesma regra de honestidade de
 * `firstNameOf`: sem `name`, formata o usuário do e-mail inteiro. */
export function fullNameOf(name: string | null, email: string): string {
  const trimmed = name?.trim();
  if (trimmed) return trimmed;
  const local = email.split("@")[0] ?? email;
  const parts = local
    .split(/[._-]+/)
    .filter(Boolean)
    .map(capitalize);
  return parts.length > 0 ? parts.join(" ") : email;
}

/** Linha de saudação completa do MobileHeader — "Bom dia, Chico". */
export function greetingLine(hour: number, name: string | null, email: string): string {
  return `${periodOfDay(hour)}, ${firstNameOf(name, email)}`;
}

/** Data por extenso, pt-BR, com a primeira letra maiúscula — "Domingo, 30 de
 * agosto". Usada como `subtitle` do TopBar. */
export function formatLongDatePtBR(date: Date): string {
  const formatted = new Intl.DateTimeFormat("pt-BR", {
    weekday: "long",
    day: "numeric",
    month: "long",
  }).format(date);
  return capitalize(formatted);
}
