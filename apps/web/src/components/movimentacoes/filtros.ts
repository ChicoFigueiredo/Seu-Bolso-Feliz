/**
 * Os filtros da tela Movimentações — lógica pura, sem React, sem banco.
 *
 * Fica num módulo próprio por dois motivos. O primeiro é teste: decidir qual
 * recorte a tela está mostrando é a parte da tela que mais erra, e é a que
 * dá para verificar sem subir Postgres nem navegador. O segundo é peso: este
 * arquivo é importado pelo componente cliente, então ele não importa
 * `@sbf/domain` nem nada de servidor — o cálculo do intervalo de datas, que
 * precisa do ciclo financeiro do usuário, mora em `./periodos.ts` e só é
 * usado pela server action.
 *
 * Vocabulário: **movimentação** (nunca "transação"/"lançamento"),
 * **categoria**, **tag**, **centro**, **não alocado** — ver CONTEXT.md.
 */

export const CHAVES_PERIODO = [
  "este-periodo",
  "periodo-anterior",
  "90-dias",
  "este-ano",
  "tudo",
] as const;
export type ChavePeriodo = (typeof CHAVES_PERIODO)[number];

export const CHAVES_TIPO = ["todos", "entradas", "saidas", "cartao"] as const;
export type ChaveTipo = (typeof CHAVES_TIPO)[number];

export const CHAVES_SITUACAO = [
  "todas",
  "nao-confirmadas",
  "vencidas",
  "sem-categoria",
  "nao-alocadas",
] as const;
export type ChaveSituacao = (typeof CHAVES_SITUACAO)[number];

export interface OpcaoFiltro {
  value: string;
  label: string;
}

export const OPCOES_PERIODO: readonly OpcaoFiltro[] = [
  { value: "este-periodo", label: "Este período" },
  { value: "periodo-anterior", label: "Período anterior" },
  { value: "90-dias", label: "90 dias" },
  { value: "este-ano", label: "Este ano" },
  { value: "tudo", label: "Tudo" },
];

export const OPCOES_TIPO: readonly OpcaoFiltro[] = [
  { value: "todos", label: "Tudo" },
  { value: "entradas", label: "Entradas" },
  { value: "saidas", label: "Saídas" },
  { value: "cartao", label: "No cartão" },
];

/**
 * "Saídas sem categoria" e "Saídas não alocadas" limitam-se a saídas de
 * propósito: é exatamente o recorte que a tela Hoje conta nas pendências, e
 * dois números com o mesmo nome e valores diferentes destroem a confiança
 * mais rápido do que um número ausente.
 */
export const OPCOES_SITUACAO: readonly OpcaoFiltro[] = [
  { value: "todas", label: "Todas as situações" },
  { value: "nao-confirmadas", label: "Não confirmadas" },
  { value: "vencidas", label: "Vencidas" },
  { value: "sem-categoria", label: "Saídas sem categoria" },
  { value: "nao-alocadas", label: "Saídas não alocadas" },
];

/** Quantas movimentações cada pedido traz. O "Carregar mais" soma outra. */
export const TAMANHO_PAGINA = 30;

export interface FiltrosMovimentacoes {
  periodo: ChavePeriodo;
  tipo: ChaveTipo;
  situacao: ChaveSituacao;
  /** Texto livre casado contra descrição e nome do fornecedor. */
  busca: string;
  categoriaId: string | null;
  tagId: string | null;
  centroId: string | null;
  /** Conta ou cartão (`financial_products.id`). */
  produtoId: string | null;
  /**
   * Quantas linhas carregar a partir da mais recente. Cresce de
   * `TAMANHO_PAGINA` em `TAMANHO_PAGINA`; a consulta é sempre do topo, então
   * nenhuma linha aparece duas vezes nem some se o dado mudar no meio.
   */
  limite: number;
}

export const FILTROS_PADRAO: FiltrosMovimentacoes = {
  periodo: "este-periodo",
  tipo: "todos",
  situacao: "todas",
  busca: "",
  categoriaId: null,
  tagId: null,
  centroId: null,
  produtoId: null,
  limite: TAMANHO_PAGINA,
};

/**
 * A tela Hoje já publica links do tipo `?filtro=vencidas` nas pendências
 * (ver `contarPendencias` em app/actions/painel-hoje.ts). Este mapa honra
 * esses links; quebrar um deles transformaria a pendência num beco sem saída.
 * `sem-centro` vira `nao-alocadas` porque "sem centro" é vocabulário
 * desaconselhado pelo CONTEXT.md.
 */
const SITUACAO_VINDA_DE_HOJE: Readonly<Record<string, ChaveSituacao>> = {
  vencidas: "vencidas",
  "sem-categoria": "sem-categoria",
  "sem-centro": "nao-alocadas",
  "nao-confirmadas": "nao-confirmadas",
};

function primeiroTexto(valor: string | string[] | undefined): string | null {
  const bruto = Array.isArray(valor) ? valor[0] : valor;
  const limpo = bruto?.trim();
  return limpo ? limpo : null;
}

function chaveValida<T extends string>(chaves: readonly T[], valor: string | null): T | null {
  return valor !== null && (chaves as readonly string[]).includes(valor) ? (valor as T) : null;
}

/**
 * Traduz a query string da URL em filtros. Qualquer valor desconhecido cai no
 * padrão em vez de virar erro: um link velho ou torto deve mostrar a tela,
 * não uma página de falha.
 */
export function lerFiltrosDaBusca(
  params: Readonly<Record<string, string | string[] | undefined>>,
): FiltrosMovimentacoes {
  const campo = (chave: string) => primeiroTexto(params[chave]);

  const situacaoDeHoje = SITUACAO_VINDA_DE_HOJE[campo("filtro") ?? ""] ?? null;
  const situacao = chaveValida(CHAVES_SITUACAO, campo("situacao")) ?? situacaoDeHoje;

  // Quem chega de uma pendência da tela Hoje quer ver as mesmas linhas que
  // foram contadas lá — e aquela contagem não tem recorte de período. Limitar
  // ao período atual mostraria menos linhas do que o número prometido.
  const periodoPadrao: ChavePeriodo = situacaoDeHoje ? "tudo" : FILTROS_PADRAO.periodo;

  return {
    periodo: chaveValida(CHAVES_PERIODO, campo("periodo")) ?? periodoPadrao,
    tipo: chaveValida(CHAVES_TIPO, campo("tipo")) ?? FILTROS_PADRAO.tipo,
    situacao: situacao ?? FILTROS_PADRAO.situacao,
    busca: campo("busca") ?? "",
    categoriaId: campo("categoria"),
    tagId: campo("tag"),
    centroId: campo("centro"),
    produtoId: campo("conta"),
    limite: TAMANHO_PAGINA,
  };
}

/**
 * Quantos filtros "de dentro da folha" estão ligados. Vira o contador do
 * botão Filtros: sem ele, um filtro escondido numa folha fechada explica
 * silenciosamente por que a lista está vazia.
 */
export function contarFiltrosDetalhados(filtros: FiltrosMovimentacoes): number {
  return [
    filtros.situacao !== "todas",
    filtros.categoriaId !== null,
    filtros.tagId !== null,
    filtros.centroId !== null,
    filtros.produtoId !== null,
  ].filter(Boolean).length;
}

/** Algum filtro difere do padrão? Decide o texto do estado vazio. */
export function temFiltroAtivo(filtros: FiltrosMovimentacoes): boolean {
  return (
    contarFiltrosDetalhados(filtros) > 0 ||
    filtros.tipo !== FILTROS_PADRAO.tipo ||
    filtros.periodo !== FILTROS_PADRAO.periodo ||
    filtros.busca.trim() !== ""
  );
}

const DIAS_SEMANA = ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"] as const;

/**
 * Cabeçalho de um dia da lista: "Hoje", "Ontem", "Sex, 29/08". O ano só
 * aparece quando é outro — repetir "/2026" em toda linha do ano corrente é
 * ruído.
 *
 * As datas chegam como `YYYY-MM-DD` e são lidas ao meio-dia local para que
 * fuso nenhum empurre o dia para trás.
 */
export function rotuloDoDia(iso: string, hojeIso: string): string {
  if (iso === hojeIso) return "Hoje";
  const data = new Date(`${iso}T12:00:00`);
  const hoje = new Date(`${hojeIso}T12:00:00`);
  const distancia = Math.round((data.getTime() - hoje.getTime()) / 86_400_000);
  if (distancia === 1) return "Amanhã";
  if (distancia === -1) return "Ontem";

  const dia = String(data.getDate()).padStart(2, "0");
  const mes = String(data.getMonth() + 1).padStart(2, "0");
  const ano = data.getFullYear() === hoje.getFullYear() ? "" : `/${data.getFullYear()}`;
  return `${DIAS_SEMANA[data.getDay()] ?? ""}, ${dia}/${mes}${ano}`;
}

export interface GrupoDoDia<T> {
  data: string;
  rotulo: string;
  itens: T[];
}

/**
 * Agrupa a lista em dias, preservando a ordem recebida. Agrupa só corridas
 * consecutivas — a lista já vem ordenada por data decrescente do banco, e
 * reordenar aqui esconderia um defeito de ordenação em vez de mostrá-lo.
 */
export function agruparPorDia<T extends { data: string }>(
  itens: readonly T[],
  hojeIso: string,
): GrupoDoDia<T>[] {
  const grupos: GrupoDoDia<T>[] = [];
  for (const item of itens) {
    const ultimo = grupos[grupos.length - 1];
    if (ultimo && ultimo.data === item.data) {
      ultimo.itens.push(item);
      continue;
    }
    grupos.push({ data: item.data, rotulo: rotuloDoDia(item.data, hojeIso), itens: [item] });
  }
  return grupos;
}

/**
 * `neutra` existe para `adjustment`: ajuste de saldo não é entrada nem saída,
 * e dar um sinal a ele mentiria sobre o fluxo do período.
 */
export type DirecaoMovimentacao = "entrada" | "saida" | "neutra";

/** Valor com sinal explícito. O menos é U+2212, que alinha com o mais. */
export function valorComSinal(direcao: DirecaoMovimentacao, valorFormatado: string): string {
  if (direcao === "entrada") return `+ ${valorFormatado}`;
  if (direcao === "saida") return `− ${valorFormatado}`;
  return valorFormatado;
}

export interface ContextoDaLinha {
  /** Nome da conta ou do cartão. */
  produto: string | null;
  /** Saiu do limite do cartão, não do saldo de uma conta. */
  noCartao: boolean;
  categoria: string | null;
}

/**
 * A segunda linha da movimentação: onde o dinheiro passou e de que natureza
 * ele é. "No cartão" vem escrito, e não só sugerido por cor, porque a
 * diferença entre gastar do saldo e gastar do limite é a informação mais
 * fácil de confundir nesta tela.
 */
export function metaDaMovimentacao(linha: ContextoDaLinha): string {
  const onde = linha.produto
    ? linha.noCartao
      ? `No cartão ${linha.produto}`
      : linha.produto
    : linha.noCartao
      ? "No cartão"
      : null;
  return [onde, linha.categoria ?? "Sem categoria"].filter(Boolean).join(" · ");
}

/**
 * Serializa os filtros de volta para query string, para a URL acompanhar o
 * que a tela mostra — link compartilhável e botão voltar coerente.
 *
 * Só o que difere do padrão entra: uma URL cheia de valores default é ruído.
 * `limite` fica de fora porque quantas linhas já foram carregadas não
 * descreve o recorte, e `filtro` (o parâmetro legado da tela Hoje) também
 * não é reemitido — ao chegar por ele, a URL se normaliza para `situacao`.
 */
export function filtrosParaBusca(filtros: FiltrosMovimentacoes): string {
  const params = new URLSearchParams();
  if (filtros.periodo !== FILTROS_PADRAO.periodo) params.set("periodo", filtros.periodo);
  if (filtros.tipo !== FILTROS_PADRAO.tipo) params.set("tipo", filtros.tipo);
  if (filtros.situacao !== FILTROS_PADRAO.situacao) params.set("situacao", filtros.situacao);
  const busca = filtros.busca.trim();
  if (busca) params.set("busca", busca);
  if (filtros.categoriaId) params.set("categoria", filtros.categoriaId);
  if (filtros.tagId) params.set("tag", filtros.tagId);
  if (filtros.centroId) params.set("centro", filtros.centroId);
  if (filtros.produtoId) params.set("conta", filtros.produtoId);
  return params.toString();
}
