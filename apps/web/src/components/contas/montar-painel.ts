/**
 * Os números da tela Contas & Cartões — a parte que não toca banco.
 *
 * Vive fora de `app/actions/` porque um módulo `"use server"` só pode exportar
 * função assíncrona: `montarPainelContas` é síncrona e, no arquivo de action,
 * derrubava a build inteira com uma mensagem que não aponta para a causa.
 * Separado aqui, ele também fica testável sem subir Postgres — mesmo motivo
 * documentado em `components/movimentacoes/filtros.ts`.
 *
 * ⚠️ Armadilha do domínio: uma fatura (`statement_cycles.card_id`) referencia
 * `cards.id` — não `financial_products.id`. São tabelas diferentes, com UUIDs
 * independentes; cruzar pela chave errada faz TODA fatura aparecer como
 * R$ 0,00, sem erro nenhum. Aqui o cruzamento parte de `cards`.
 */
import { getFinancialProducts } from "@/app/actions/financial-products";
import type { CardComProduto } from "@/app/actions/cards";
import { getInstitutions } from "@/app/actions/institutions";
import { getStatementCycles } from "@/app/actions/statement-cycles";

/** Os dois tipos que este painel entende como "conta" — mesmo recorte de
 * `CONTAS` em `painel-hoje.ts`, para que o total daqui bata com o
 * "Disponível agora" da tela Hoje. */
const TIPOS_CONTA = ["checking_account", "savings_account"] as const;

const num = (v: unknown): number => (v == null ? 0 : Number(v));

export type ProdutoParaPainel = Pick<
  Awaited<ReturnType<typeof getFinancialProducts>>[number],
  "id" | "institution_id" | "name" | "type" | "current_balance" | "credit_limit" | "is_active"
>;

export type CartaoParaPainel = Pick<
  CardComProduto,
  | "id"
  | "financial_product_id"
  | "last_four_digits"
  | "card_brand"
  | "holder_name"
  | "is_primary"
  | "is_active"
  | "closing_day"
  | "due_day"
  | "financial_products"
>;

export type InstituicaoParaPainel = Pick<
  Awaited<ReturnType<typeof getInstitutions>>[number],
  "id" | "name" | "type" | "is_active"
>;

export type CicloParaPainel = Pick<
  Awaited<ReturnType<typeof getStatementCycles>>[number],
  | "id"
  | "card_id"
  | "reference_month"
  | "cycle_start_date"
  | "cycle_end_date"
  | "due_date"
  | "total_amount"
  | "paid_amount"
  | "status"
>;

export interface InstituicaoResumo {
  id: string;
  nome: string;
  tipo: string;
  ativa: boolean;
}

export interface ContaDetalhe {
  id: string;
  nome: string;
  /** `checking_account` | `savings_account`. */
  tipo: string;
  instituicaoId: string;
  instituicao: string | null;
  saldo: number;
  ativa: boolean;
}

export interface FaturaResumo {
  id: string;
  cartaoId: string;
  referenceMonth: string;
  cycleStart: string;
  cycleEnd: string;
  dueDate: string;
  total: number;
  paid: number;
  /** `open` | `closed` | `paid` | `partial` | `overdue`. */
  status: string;
}

export interface CartaoDetalhe {
  id: string;
  financialProductId: string;
  nome: string;
  instituicao: string | null;
  brand: string | null;
  last4: string | null;
  holderName: string | null;
  isPrimary: boolean;
  ativo: boolean;
  /** Dia do mês do fechamento contratual do cartão (`cards.closing_day`) —
   * um fato recorrente, existe mesmo sem nenhuma fatura ainda gerada. */
  closingDay: number | null;
  /** Idem, para o vencimento (`cards.due_day`). */
  dueDay: number | null;
  /** Vem de `financial_products.credit_limit` — não de `cards.credit_limit`,
   * que é uma coluna paralela e hoje desconectada (ver relatório final). */
  limiteTotal: number | null;
  /** Total da fatura atual (ver `cicloAtual`), ou 0 quando não há nenhuma em
   * aberto. */
  faturaAtual: number;
  limiteLivre: number | null;
  /** A fatura não paga mais antiga — a próxima que pede atenção. `null`
   * quando todas as faturas estão pagas (ou não há nenhuma ainda). */
  cicloAtual: FaturaResumo | null;
  /** Todas as faturas do cartão, mais recente primeiro. */
  ciclos: FaturaResumo[];
}

export interface PainelContas {
  instituicoes: InstituicaoResumo[];
  contas: ContaDetalhe[];
  /** Soma do saldo só das contas ativas — mesmo recorte de
   * `disponivelAgora` em `painel-hoje.ts`. */
  totalContas: number;
  cartoes: CartaoDetalhe[];
}

/**
 * Monta o modelo de leitura a partir dos quatro recortes já buscados.
 * Função pura — sem banco, sem `requireUser` — para caber num teste comum.
 */
export function montarPainelContas(input: {
  produtos: ProdutoParaPainel[];
  cards: CartaoParaPainel[];
  instituicoes: InstituicaoParaPainel[];
  ciclos: CicloParaPainel[];
}): PainelContas {
  const produtoPorId = new Map(input.produtos.map((p) => [p.id, p]));
  const instituicaoPorId = new Map(input.instituicoes.map((i) => [i.id, i]));

  const ciclosPorCartao = new Map<string, CicloParaPainel[]>();
  for (const ciclo of input.ciclos) {
    const lista = ciclosPorCartao.get(ciclo.card_id) ?? [];
    lista.push(ciclo);
    ciclosPorCartao.set(ciclo.card_id, lista);
  }
  // Mais recente primeiro dentro de cada cartão (vencimento decrescente).
  for (const lista of ciclosPorCartao.values()) {
    lista.sort((a, b) => b.due_date.localeCompare(a.due_date));
  }

  const paraFatura = (c: CicloParaPainel): FaturaResumo => ({
    id: c.id,
    cartaoId: c.card_id,
    referenceMonth: c.reference_month,
    cycleStart: c.cycle_start_date,
    cycleEnd: c.cycle_end_date,
    dueDate: c.due_date,
    total: num(c.total_amount),
    paid: num(c.paid_amount),
    status: c.status,
  });

  const contas: ContaDetalhe[] = input.produtos
    .filter((p) => (TIPOS_CONTA as readonly string[]).includes(p.type))
    .map((p) => ({
      id: p.id,
      nome: p.name,
      tipo: p.type,
      instituicaoId: p.institution_id,
      instituicao: instituicaoPorId.get(p.institution_id)?.name ?? null,
      saldo: num(p.current_balance),
      ativa: p.is_active,
    }));

  const totalContas = contas.filter((c) => c.ativa).reduce((soma, c) => soma + c.saldo, 0);

  const cartoes: CartaoDetalhe[] = input.cards
    // Um `cards` pode, em tese, estar ligado a um produto que não é cartão de
    // crédito (débito sobre conta corrente — ver comentário de
    // `getProdutosParaCartao` em cards.ts). Fatura e limite só existem para
    // crédito, então é esse o recorte desta tela. O `??` cai para o `type` já
    // resolvido pelo JOIN de `getCards()` quando o produto não aparece no mapa
    // (defesa em profundidade — mesmo espírito do fallback logo abaixo).
    .filter(
      (card) =>
        (produtoPorId.get(card.financial_product_id)?.type ?? card.financial_products?.type) ===
        "credit_card",
    )
    .map((card) => {
      const produto = produtoPorId.get(card.financial_product_id) ?? null;
      const limiteTotal = produto?.credit_limit == null ? null : num(produto.credit_limit);

      const ciclos = (ciclosPorCartao.get(card.id) ?? []).map(paraFatura);
      // "Atual" é a fatura não paga mais antiga — a próxima que pede
      // atenção. `ciclos` está em ordem decrescente de vencimento, então é a
      // última da lista filtrada.
      const naoPagas = ciclos.filter((f) => f.status !== "paid");
      const cicloAtual = naoPagas.length > 0 ? (naoPagas[naoPagas.length - 1] ?? null) : null;
      const faturaAtual = cicloAtual ? cicloAtual.total - cicloAtual.paid : 0;

      return {
        id: card.id,
        financialProductId: card.financial_product_id,
        nome: produto?.name ?? card.financial_products?.name ?? "Cartão",
        instituicao: card.financial_products?.institutions?.name ?? null,
        brand: card.card_brand,
        last4: card.last_four_digits,
        holderName: card.holder_name,
        isPrimary: card.is_primary,
        ativo: card.is_active,
        closingDay: card.closing_day,
        dueDay: card.due_day,
        limiteTotal,
        faturaAtual,
        limiteLivre: limiteTotal == null ? null : limiteTotal - faturaAtual,
        cicloAtual,
        ciclos,
      };
    });

  const instituicoes: InstituicaoResumo[] = input.instituicoes.map((i) => ({
    id: i.id,
    nome: i.name,
    tipo: i.type,
    ativa: i.is_active,
  }));

  return { instituicoes, contas, totalContas, cartoes };
}
