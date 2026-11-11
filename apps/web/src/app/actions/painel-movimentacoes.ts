"use server";

/**
 * Os dados da tela Movimentações.
 *
 * A tela responde a uma pergunta simples — "o que entrou e o que saiu?" — mas
 * o resumo no topo esbarra em duas armadilhas que, se ignoradas, produzem um
 * número errado com cara de certo:
 *
 *   1. **Compra no cartão não é saída de caixa.** Quando você passa o cartão,
 *      o dinheiro ainda está na conta; ele sai no pagamento da fatura. Somar a
 *      compra e depois o `statement_payment` conta o mesmo dinheiro duas
 *      vezes. Aqui "Saiu" conta só o que deixou uma **conta**, e o que foi
 *      comprado no cartão aparece num número próprio, rotulado como
 *      compromisso futuro.
 *
 *   2. **Previsto não é realizado.** Movimentação não confirmada é intenção,
 *      não fato. "Entrou", "Saiu" e "Resultado" somam apenas movimentação
 *      confirmada; o previsto vai para um campo separado e a interface o
 *      apresenta como estimativa, nunca embutido no número principal.
 *
 * `adjustment` fica de fora dos dois lados: ajuste de saldo é correção de
 * registro, não fluxo de dinheiro.
 *
 * Segurança: não há RLS no Neon. Toda consulta filtra `user_id` pelo
 * `ownerId` (o dono dos dados, não o ator) — ver apps/web/src/lib/auth/index.ts.
 */

import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, ilike, inArray, isNull, lte, or, sql, type SQL } from "drizzle-orm";
import { requireUser } from "@/lib/auth";
import { getCategories } from "./categories";
import { getTags } from "./tags";
import { listFinancialCenters } from "./financial-centers";
import { getFinancialProducts } from "./financial-products";
import {
  TAMANHO_PAGINA,
  type DirecaoMovimentacao,
  type FiltrosMovimentacoes,
} from "@/components/movimentacoes/filtros";
import { intervaloDoPeriodo } from "@/components/movimentacoes/periodos";
import { decidirPaginacao, limiteDaConsulta } from "@/components/movimentacoes/paginacao";

const ENTRADAS = ["income", "refund"] as const;
const SAIDAS = [
  "expense",
  "fee",
  "interest_charge",
  "statement_payment",
  "liability_payment",
] as const;

const T = schema.transactions;
const P = schema.financial_products;
const C = schema.categories;
const F = schema.suppliers;

export interface MovimentacaoItem {
  id: string;
  descricao: string;
  /** Sempre positivo; o sinal vem de `direcao`. */
  valor: number;
  /** `YYYY-MM-DD`. */
  data: string;
  direcao: DirecaoMovimentacao;
  /** Conta ou cartão de onde a movimentação saiu/entrou. */
  produto: string | null;
  /** Saiu do limite do cartão, não do saldo de uma conta. */
  noCartao: boolean;
  categoria: string | null;
  fornecedor: string | null;
  /** Centros que receberam rateio desta movimentação. */
  centros: string[];
  /** Ainda não confirmada: é previsão, não fato. */
  prevista: boolean;
  /** Há documento de origem — dá para explicar o gasto depois. */
  temDocumento: boolean;
}

export interface ResumoMovimentacoes {
  /** Entradas confirmadas em conta. */
  entrou: number;
  /** Saídas confirmadas de conta, incluindo pagamento de fatura e de parcela. */
  saiu: number;
  /** `entrou - saiu`. Fluxo do recorte, não posição da conta. */
  resultado: number;
  /** Comprado no cartão e ainda não pago: compromisso, não saída de caixa. */
  noCartao: number;
  /** Entradas ainda não confirmadas dentro do recorte. */
  previstoEntrada: number;
  /** Saídas ainda não confirmadas dentro do recorte. */
  previstoSaida: number;
  /** Quantas movimentações casam com o filtro (não só as carregadas). */
  total: number;
  /** Quantas movimentações do recorte ainda não foram confirmadas. */
  quantidadePrevista: number;
}

export interface PainelMovimentacoes {
  periodo: { inicio: string | null; fim: string | null; rotulo: string };
  /** Data de hoje no relógio do servidor — evita "Hoje" divergir na hidratação. */
  hoje: string;
  resumo: ResumoMovimentacoes;
  itens: MovimentacaoItem[];
  /** Há mais linhas além das carregadas, e dá para carregá-las. */
  temMais: boolean;
  /** Há mais linhas, mas o teto por requisição foi atingido: filtre. */
  atingiuTeto: boolean;
  /**
   * Quantas movimentações o usuário tem AO TODO, ignorando todo filtro.
   *
   * Existe por um motivo concreto: com o período padrão num mês recém-começado
   * e 12 meses de extrato importados, a tela dizia "Nenhuma movimentação
   * ainda — assim que uma conta for conectada…" para quem tinha 2858. Um app
   * financeiro que parece ter perdido o histórico perde o usuário; comparar o
   * recorte com este número é o que permite dizer a verdade.
   */
  totalGeral: number;
}

export interface OpcoesMovimentacoes {
  categorias: { id: string; nome: string }[];
  tags: { id: string; nome: string }[];
  centros: { id: string; nome: string }[];
  /** Contas e cartões, já rotulados para o seletor. */
  contas: { id: string; nome: string }[];
}

const num = (valor: unknown) => (valor == null ? 0 : Number(valor));

function iso(data: Date): string {
  const mes = String(data.getMonth() + 1).padStart(2, "0");
  const dia = String(data.getDate()).padStart(2, "0");
  return `${data.getFullYear()}-${mes}-${dia}`;
}

function direcaoDoTipo(tipo: string): DirecaoMovimentacao {
  if ((ENTRADAS as readonly string[]).includes(tipo)) return "entrada";
  if ((SAIDAS as readonly string[]).includes(tipo)) return "saida";
  return "neutra";
}

/** `%` e `_` digitados pelo usuário são texto, não curinga. */
function textoParaLike(termo: string): string {
  return `%${termo.replace(/[\\%_]/g, (caractere) => `\\${caractere}`)}%`;
}

const ehEntrada = inArray(T.type, [...ENTRADAS]);
const ehSaida = inArray(T.type, [...SAIDAS]);
const confirmada = eq(T.is_confirmed, true);
const naoConfirmada = eq(T.is_confirmed, false);
const ehCartao = eq(P.type, "credit_card");
// `coalesce` porque o join é `left`: sem produto casado, a movimentação conta
// como de conta, que é o comportamento conservador (ela aparece no fluxo).
const ehConta = sql`coalesce(${P.type}, '') <> 'credit_card'`;

/**
 * As condições do recorte atual. Vale para a lista e para os totais — se as
 * duas divergirem, o resumo descreve um conjunto que a lista não mostra.
 */
function condicoesDoRecorte(
  filtros: FiltrosMovimentacoes,
  ownerId: string,
  intervalo: { inicio: string | null; fim: string | null },
  hoje: string,
): SQL[] {
  const condicoes: SQL[] = [eq(T.user_id, ownerId)];

  if (intervalo.inicio) condicoes.push(gte(T.event_date, intervalo.inicio));
  if (intervalo.fim) condicoes.push(lte(T.event_date, intervalo.fim));

  if (filtros.tipo === "entradas") condicoes.push(ehEntrada);
  if (filtros.tipo === "saidas") condicoes.push(ehSaida);
  if (filtros.tipo === "cartao") condicoes.push(ehCartao);

  // Os mesmos recortes que a tela Hoje conta em "Precisam da sua atenção".
  if (filtros.situacao === "nao-confirmadas") condicoes.push(naoConfirmada);
  if (filtros.situacao === "vencidas") {
    condicoes.push(naoConfirmada, lte(T.event_date, hoje), ehSaida);
  }
  if (filtros.situacao === "sem-categoria") condicoes.push(isNull(T.category_id), ehSaida);
  if (filtros.situacao === "nao-alocadas") {
    condicoes.push(
      ehSaida,
      sql`not exists (select 1 from public.transaction_allocations a where a.transaction_id = ${T.id})`,
    );
  }

  if (filtros.categoriaId) condicoes.push(eq(T.category_id, filtros.categoriaId));
  if (filtros.produtoId) condicoes.push(eq(T.financial_product_id, filtros.produtoId));

  // Tag e centro filtram por `exists` em vez de join para não multiplicar
  // linhas quando a movimentação tem mais de uma — o que estragaria os totais.
  // Um id de outro usuário aqui não vaza nada: a consulta externa já está
  // presa ao `ownerId`, então ele simplesmente não casa com nada.
  if (filtros.tagId) {
    condicoes.push(
      sql`exists (select 1 from public.transaction_tags tt where tt.transaction_id = ${T.id} and tt.tag_id = ${filtros.tagId})`,
    );
  }
  if (filtros.centroId) {
    condicoes.push(
      sql`exists (select 1 from public.transaction_allocations a where a.transaction_id = ${T.id} and a.financial_center_id = ${filtros.centroId} and a.user_id = ${ownerId})`,
    );
  }

  const busca = filtros.busca.trim();
  if (busca) {
    const padrao = textoParaLike(busca);
    const casa = or(ilike(T.description, padrao), ilike(F.name, padrao));
    if (casa) condicoes.push(casa);
  }

  return condicoes;
}

export async function carregarMovimentacoes(
  filtros: FiltrosMovimentacoes,
): Promise<PainelMovimentacoes> {
  const { ownerId } = await requireUser();
  const db = getDb();
  const agora = new Date();
  const hoje = iso(agora);

  // Sem preferência gravada, o mês-calendário é o padrão honesto — mesma
  // regra da tela Hoje.
  const [preferencia] = await db
    .select({ dia: schema.user_financial_preferences.financial_cycle_start_day })
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, ownerId))
    .limit(1);

  const intervalo = intervaloDoPeriodo(filtros.periodo, agora, preferencia?.dia ?? 1);
  const onde = and(...condicoesDoRecorte(filtros, ownerId, intervalo, hoje));
  const limite = limiteDaConsulta(filtros.limite, TAMANHO_PAGINA);

  // Todo `leftJoin` repete o filtro por dono de propósito (defesa em
  // profundidade): sem RLS, um join sem `user_id` é a porta de um IDOR.
  const [linhas, [totais], [geral]] = await Promise.all([
    db
      .select({
        id: T.id,
        descricao: T.description,
        valor: T.amount,
        data: T.event_date,
        tipo: T.type,
        confirmada: T.is_confirmed,
        documentoId: T.source_document_id,
        produto: P.name,
        produtoTipo: P.type,
        categoria: C.name,
        fornecedor: F.name,
      })
      .from(T)
      .leftJoin(P, and(eq(P.id, T.financial_product_id), eq(P.user_id, ownerId)))
      .leftJoin(C, and(eq(C.id, T.category_id), eq(C.user_id, ownerId)))
      .leftJoin(F, and(eq(F.id, T.supplier_id), eq(F.user_id, ownerId)))
      .where(onde)
      // `created_at` desempata dentro do dia: sem ele a ordem de duas linhas
      // da mesma data muda entre requisições e a paginação repete/pula linhas.
      .orderBy(desc(T.event_date), desc(T.created_at))
      .limit(limite),

    db
      .select({
        entrou: sql<number>`coalesce(sum(case when ${confirmada} and ${ehEntrada} and ${ehConta} then ${T.amount} else 0 end), 0)`,
        saiu: sql<number>`coalesce(sum(case when ${confirmada} and ${ehSaida} and ${ehConta} then ${T.amount} else 0 end), 0)`,
        // Estorno no cartão abate a fatura, por isso entra com sinal trocado.
        noCartao: sql<number>`coalesce(sum(case when ${confirmada} and ${ehCartao} then (case when ${ehEntrada} then -${T.amount} else ${T.amount} end) else 0 end), 0)`,
        previstoEntrada: sql<number>`coalesce(sum(case when ${naoConfirmada} and ${ehEntrada} then ${T.amount} else 0 end), 0)`,
        previstoSaida: sql<number>`coalesce(sum(case when ${naoConfirmada} and ${ehSaida} then ${T.amount} else 0 end), 0)`,
        quantidadePrevista: sql<number>`count(*) filter (where ${naoConfirmada})`,
        total: sql<number>`count(*)`,
      })
      .from(T)
      .leftJoin(P, and(eq(P.id, T.financial_product_id), eq(P.user_id, ownerId)))
      .leftJoin(F, and(eq(F.id, T.supplier_id), eq(F.user_id, ownerId)))
      .where(onde),

    // Sem NENHUM filtro, nem de período: é a régua contra a qual o recorte se
    // compara. Barata — `idx_transactions_user_id` cobre exatamente isto.
    db
      .select({ total: sql<number>`count(*)` })
      .from(T)
      .where(eq(T.user_id, ownerId)),
  ]);

  const ids = linhas.map((linha) => linha.id);
  const rateios =
    ids.length === 0
      ? []
      : await db
          .select({
            movimentacaoId: schema.transaction_allocations.transaction_id,
            centro: schema.financial_centers.name,
          })
          .from(schema.transaction_allocations)
          .innerJoin(
            schema.financial_centers,
            and(
              eq(schema.financial_centers.id, schema.transaction_allocations.financial_center_id),
              eq(schema.financial_centers.user_id, ownerId),
            ),
          )
          .where(
            and(
              eq(schema.transaction_allocations.user_id, ownerId),
              inArray(schema.transaction_allocations.transaction_id, ids),
            ),
          );

  const centrosPorMovimentacao = new Map<string, string[]>();
  for (const rateio of rateios) {
    const atuais = centrosPorMovimentacao.get(rateio.movimentacaoId);
    if (atuais) atuais.push(rateio.centro);
    else centrosPorMovimentacao.set(rateio.movimentacaoId, [rateio.centro]);
  }

  const itens: MovimentacaoItem[] = linhas.map((linha) => ({
    id: linha.id,
    // Sem descrição, o fornecedor é a melhor identificação que existe.
    descricao: linha.descricao?.trim() || linha.fornecedor || "Sem descrição",
    valor: Math.abs(num(linha.valor)),
    data: linha.data,
    direcao: direcaoDoTipo(linha.tipo),
    produto: linha.produto,
    noCartao: linha.produtoTipo === "credit_card",
    categoria: linha.categoria,
    fornecedor: linha.fornecedor,
    centros: centrosPorMovimentacao.get(linha.id) ?? [],
    prevista: !linha.confirmada,
    temDocumento: linha.documentoId != null,
  }));

  const entrou = num(totais?.entrou);
  const saiu = num(totais?.saiu);
  const total = num(totais?.total);

  return {
    periodo: intervalo,
    hoje,
    resumo: {
      entrou,
      saiu,
      resultado: entrou - saiu,
      noCartao: num(totais?.noCartao),
      previstoEntrada: num(totais?.previstoEntrada),
      previstoSaida: num(totais?.previstoSaida),
      quantidadePrevista: num(totais?.quantidadePrevista),
      total,
    },
    itens,
    ...decidirPaginacao(total, itens.length),
    totalGeral: num(geral?.total),
  };
}

/**
 * As listas que alimentam os seletores da folha de filtros.
 *
 * Reaproveita as actions que já existem em vez de repetir consulta: cada uma
 * já filtra por `ownerId` e já é testada.
 */
export async function carregarOpcoesMovimentacoes(): Promise<OpcoesMovimentacoes> {
  const [categorias, tags, centros, produtos] = await Promise.all([
    getCategories(),
    getTags(),
    listFinancialCenters({ status: "active" }),
    getFinancialProducts(),
  ]);

  return {
    categorias: categorias
      .filter((categoria) => categoria.is_active)
      .map((categoria) => ({ id: categoria.id, nome: categoria.name })),
    tags: tags.map((tag) => ({ id: tag.id, nome: tag.name })),
    centros: centros.map((centro) => ({ id: centro.id, nome: centro.name })),
    contas: produtos
      .filter((produto) => produto.is_active)
      .map((produto) => ({
        id: produto.id,
        nome: produto.type === "credit_card" ? `Cartão ${produto.name}` : produto.name,
      })),
  };
}
