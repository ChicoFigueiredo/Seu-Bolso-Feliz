/**
 * Identidade visual de um Centro: cor e ícone.
 *
 * A cor de um centro não é decoração — é o que faz "Edulingo" ser reconhecido
 * no cartão da lista, na barra de rateio e no gráfico sem ler o nome. Por isso
 * ela vem de uma paleta fechada, e não de um seletor livre: nove pigmentos que
 * convivem no tema salvia e que continuam distinguíveis entre si.
 *
 * Nada aqui é obrigatório no banco (`color` e `icon` são nulos por padrão);
 * `corDoCentro`/`iconeDoCentro` cuidam do centro antigo que ainda não escolheu.
 */

export interface CorDeCentro {
  /** Valor gravado em `financial_centers.color` — hex, no máximo 20 caracteres. */
  valor: string;
  nome: string;
}

export const CORES_DE_CENTRO: readonly CorDeCentro[] = [
  { valor: "#245C4A", nome: "Sálvia" },
  { valor: "#2E7A76", nome: "Petróleo" },
  { valor: "#496A7A", nome: "Ardósia" },
  { valor: "#5C6BA8", nome: "Anil" },
  { valor: "#7A5A86", nome: "Ameixa" },
  { valor: "#A85C6E", nome: "Rosé" },
  { valor: "#B4623F", nome: "Terracota" },
  { valor: "#97701C", nome: "Ocre" },
  { valor: "#6B7A3A", nome: "Musgo" },
];

export interface IconeDeCentro {
  /** Glifo Material Symbols Rounded, gravado em `financial_centers.icon`. */
  valor: string;
  nome: string;
}

export const ICONES_DE_CENTRO: readonly IconeDeCentro[] = [
  { valor: "workspaces", nome: "Projeto" },
  { valor: "school", nome: "Estudo" },
  { valor: "home", nome: "Casa" },
  { valor: "person", nome: "Pessoa" },
  { valor: "business_center", nome: "Negócio" },
  { valor: "rocket_launch", nome: "Lançamento" },
  { valor: "code", nome: "Software" },
  { valor: "storefront", nome: "Loja" },
  { valor: "directions_car", nome: "Carro" },
  { valor: "pets", nome: "Pet" },
  { valor: "favorite", nome: "Saúde" },
  { valor: "flight", nome: "Viagem" },
];

/** Cor padrão de um centro que nunca escolheu uma. */
export const COR_PADRAO = "#245C4A";
/** Ícone padrão — o mesmo da seção na navegação. */
export const ICONE_PADRAO = "workspaces";

export function corDoCentro(cor: string | null | undefined): string {
  const escolhida = cor?.trim();
  return escolhida ? escolhida : COR_PADRAO;
}

export function iconeDoCentro(icone: string | null | undefined): string {
  const escolhido = icone?.trim();
  return escolhido ? escolhido : ICONE_PADRAO;
}

/** Rótulo humano do `type` do centro, para não vazar o valor cru do banco. */
export function rotuloDoTipo(tipo: string): string {
  if (tipo === "pessoa") return "Pessoa";
  if (tipo === "negocio") return "Negócio";
  return "Projeto";
}

export const TIPOS_DE_CENTRO = [
  { valor: "projeto", rotulo: "Projeto" },
  { valor: "pessoa", rotulo: "Pessoa" },
  { valor: "negocio", rotulo: "Negócio" },
] as const;
