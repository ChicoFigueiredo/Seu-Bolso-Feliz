"use server";

/**
 * Os números das telas de Projetos & Centros.
 *
 * Só agregação — todo CRUD de centro e todo rateio já existe em
 * `financial-centers.ts` e é reaproveitado de lá sem reimplementação.
 *
 * A pergunta que estas telas respondem é uma só: **quanto deste mês foi para
 * cada projeto, e quanto ainda não foi para projeto nenhum**. Daí os três
 * cuidados que valem mais que a estética:
 *
 *   1. **Não alocado é informação, não erro.** A soma dos rateios de uma
 *      movimentação pode ser menor que ela; o resto aparece somado no topo da
 *      lista porque é o trabalho que falta fazer, não um defeito.
 *
 *   2. **Pagamento de fatura fica fora da conta.** `statement_payment` não
 *      aceita rateio (as compras dentro da fatura já foram rateadas uma a
 *      uma), então incluí-lo no "não alocado" mostraria como pendência algo
 *      que nunca vai poder ser resolvido.
 *
 *   3. **Excluir um centro é diferente de arquivar.** A tela precisa saber
 *      quantos rateios cada centro tem para oferecer "excluir" só quando é
 *      verdade — a FK é `on delete restrict` e o botão mentiria.
 *
 * Segurança: não há RLS (o Neon não tem). Toda consulta filtra `user_id` por
 * `ownerId` — o dono dos dados, nunca `id`, que é quem está agindo.
 */

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq, gte, inArray, lte, notInArray, sql } from "drizzle-orm";
import {
  INFLOW_TRANSACTION_TYPES,
  NON_ALLOCATABLE_TRANSACTION_TYPES,
  canAllocateTransactionType,
  getCurrentPeriod,
} from "@sbf/domain";
import {
  getFinancialCenter,
  listFinancialCenters,
  listFinancialCentersWithBalance,
} from "@/app/actions/financial-centers";
// Fica em `components/centros/` e não aqui porque este arquivo é "use server":
// nele só função async pode ser exportada, e um helper puro exportado daqui
// não compilaria nem daria para testar isoladamente.
import { rotuloCurtoDoPeriodo } from "@/components/centros/periodo";
import { paraIso } from "@/lib/datas";

// ══════════════════════════════════════════════════════════════
// Tipos da interface
// ══════════════════════════════════════════════════════════════

export interface PeriodoResumo {
  inicio: string;
  fim: string;
  /** Rótulo completo do domínio — `01/08/2026 a 31/08/2026`. */
  rotulo: string;
  /**
   * Rótulo enxuto para título e rótulo de métrica — `agosto`, `20/03 a 19/04`.
   * Calculado aqui, no servidor, e não na tela: ele depende do ano corrente, e
   * decidir isso no cliente abriria divergência de hidratação na virada do ano.
   */
  rotuloCurto: string;
}

export interface CentroNoPainel {
  id: string;
  nome: string;
  tipo: string;
  status: string;
  descricao: string | null;
  icone: string | null;
  cor: string | null;
  /** Entradas menos saídas desde sempre — a conta corrente virtual do centro. */
  saldo: number;
  entrouNoPeriodo: number;
  saiuNoPeriodo: number;
  resultadoNoPeriodo: number;
  /** Quantos rateios apontam para este centro. Zero é o que libera excluir. */
  rateios: number;
}

export interface MovimentacaoPendente {
  id: string;
  descricao: string;
  data: string;
  tipo: string;
  /** Valor cheio da movimentação, sempre positivo. */
  valor: number;
  alocado: number;
  naoAlocado: number;
  produto: string | null;
}

export interface PainelCentros {
  periodo: PeriodoResumo;
  centrosAtivos: CentroNoPainel[];
  centrosArquivados: CentroNoPainel[];
  /** As saídas do período que ainda não foram atribuídas a nenhum centro. */
  naoAlocado: {
    /** Quanto do período ainda não tem dono. */
    total: number;
    /** Total de saídas rateáveis do período — a base da comparação. */
    saidasDoPeriodo: number;
    /** Quantas movimentações têm alguma parte sem centro. */
    movimentacoes: number;
  };
  /** As primeiras pendências, para dar o que fazer sem sair da tela. */
  pendentes: MovimentacaoPendente[];
}

export interface MovimentacaoDoCentro {
  id: string;
  descricao: string;
  data: string;
  tipo: string;
  direcao: "entrada" | "saida";
  /** Valor cheio da movimentação — o contexto, não o número principal. */
  valorMovimentacao: number;
  /** O que foi de fato atribuído a este centro. É este que soma. */
  valorAlocado: number;
  produto: string | null;
  nota: string | null;
}

export interface PontoEvolucao {
  dia: string;
  saiu: number;
  /** Quanto o centro já tinha consumido até o fim deste dia. */
  acumulado: number;
}

export interface DetalheCentro {
  centro: {
    id: string;
    nome: string;
    tipo: string;
    status: string;
    descricao: string | null;
    icone: string | null;
    cor: string | null;
  };
  periodo: PeriodoResumo;
  saldo: number;
  entrouSempre: number;
  saiuSempre: number;
  entrouNoPeriodo: number;
  saiuNoPeriodo: number;
  resultadoNoPeriodo: number;
  rateios: number;
  /** Quantas movimentações do período tocam este centro (antes do corte). */
  totalMovimentacoes: number;
  movimentacoes: MovimentacaoDoCentro[];
  evolucao: PontoEvolucao[];
}

export interface CentroEscolhivel {
  id: string;
  nome: string;
  cor: string | null;
  icone: string | null;
}

export interface RateioDaMovimentacao {
  transacaoId: string;
  descricao: string;
  data: string;
  tipo: string;
  /** Valor cheio da movimentação, sempre positivo. */
  valor: number;
  produto: string | null;
  /** `false` para pagamento de fatura — o banco recusa o rateio. */
  podeRatear: boolean;
  /** Por que não dá, em português, para a tela explicar antes de o usuário tentar. */
  motivoNaoPodeRatear: string | null;
  alocado: number;
  naoAlocado: number;
  rateios: Array<{ centroId: string; valor: number; nota: string | null }>;
  /** Centros ativos — centro arquivado não recebe rateio novo. */
  centros: CentroEscolhivel[];
}

// ══════════════════════════════════════════════════════════════
// Infraestrutura interna (arquivo "use server": só função async é exportável)
// ══════════════════════════════════════════════════════════════

const iso = paraIso;
const num = (valor: unknown) => (valor == null ? 0 : Number(valor));

/** Quantas movimentações a lista de pendências mostra antes de mandar para a busca. */
const LIMITE_PENDENTES = 12;
/** Quantas movimentações a tela de detalhe lista. A evolução usa agregado, não esta lista. */
const LIMITE_MOVIMENTACOES = 100;

/**
 * O período financeiro do usuário. Pode não começar no dia 1º — sem
 * preferência gravada, o mês-calendário é o padrão honesto (mesma regra da
 * tela Hoje).
 */
async function periodoDoUsuario(ownerId: string): Promise<PeriodoResumo> {
  const db = getDb();
  const [preferencia] = await db
    .select({ dia: schema.user_financial_preferences.financial_cycle_start_day })
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, ownerId))
    .limit(1);

  const periodo = getCurrentPeriod({ startDay: preferencia?.dia ?? 1 }, new Date());
  const inicio = iso(periodo.startDate);
  const fim = iso(periodo.endDate);
  return {
    inicio,
    fim,
    rotulo: periodo.label,
    rotuloCurto: rotuloCurtoDoPeriodo(inicio, fim),
  };
}

/**
 * Subconsulta "quanto já foi rateado de cada movimentação".
 *
 * Sai como subconsulta e não como `join` direto porque a agregação precisa
 * acontecer **antes** do join: agrupar depois multiplicaria o valor da
 * movimentação pelo número de rateios dela.
 */
function somaRateiosPorMovimentacao(db: ReturnType<typeof getDb>, ownerId: string) {
  return db
    .select({
      transactionId: schema.transaction_allocations.transaction_id,
      alocado: sql<string>`sum(${schema.transaction_allocations.amount})`.as("alocado"),
    })
    .from(schema.transaction_allocations)
    .where(eq(schema.transaction_allocations.user_id, ownerId))
    .groupBy(schema.transaction_allocations.transaction_id)
    .as("soma_rateios");
}

/** Contagem de rateios por centro — é o que decide se "excluir" pode aparecer. */
async function contarRateiosPorCentro(ownerId: string): Promise<Map<string, number>> {
  const db = getDb();
  const linhas = await db
    .select({
      centroId: schema.transaction_allocations.financial_center_id,
      quantidade: sql<string>`count(*)`,
    })
    .from(schema.transaction_allocations)
    .where(eq(schema.transaction_allocations.user_id, ownerId))
    .groupBy(schema.transaction_allocations.financial_center_id);

  return new Map(linhas.map((linha) => [linha.centroId, num(linha.quantidade)]));
}

// ══════════════════════════════════════════════════════════════
// Lista — /dashboard/centros
// ══════════════════════════════════════════════════════════════

export async function carregarPainelCentros(): Promise<PainelCentros> {
  const { ownerId } = await requireUser();
  const db = getDb();
  const periodo = await periodoDoUsuario(ownerId);

  // Saídas do período que aceitam rateio. `income`/`refund` ficam de fora
  // porque a pergunta da tela é sobre gasto; `statement_payment` fica de fora
  // porque nunca vai poder ser rateado.
  const saidasRateaveis = and(
    eq(schema.transactions.user_id, ownerId),
    gte(schema.transactions.event_date, periodo.inicio),
    lte(schema.transactions.event_date, periodo.fim),
    notInArray(schema.transactions.type, [
      ...INFLOW_TRANSACTION_TYPES,
      ...NON_ALLOCATABLE_TRANSACTION_TYPES,
    ]),
  );

  const soma = somaRateiosPorMovimentacao(db, ownerId);
  const valorCheio = sql`abs(${schema.transactions.amount})`;
  const jaAlocado = sql`coalesce(${soma.alocado}, 0)`;

  const [centros, rateiosPorCentro, totais, pendentes] = await Promise.all([
    listFinancialCentersWithBalance({ startDate: periodo.inicio, endDate: periodo.fim }),

    contarRateiosPorCentro(ownerId),

    db
      .select({
        // `::text` em vez de `::float8`: `sum(numeric)` é decimal exato no
        // Postgres e o texto chega sem uma rodada extra de arredondamento.
        saidas: sql<string>`coalesce(sum(${valorCheio}), 0)::text`,
        // `least` é rede de segurança: o trigger do banco já garante que o
        // alocado nunca passa do valor da movimentação.
        alocado: sql<string>`coalesce(sum(least(${jaAlocado}, ${valorCheio})), 0)::text`,
        movimentacoes: sql<string>`count(*) filter (where ${jaAlocado} < ${valorCheio})`,
      })
      .from(schema.transactions)
      .leftJoin(soma, eq(soma.transactionId, schema.transactions.id))
      .where(saidasRateaveis),

    db
      .select({
        id: schema.transactions.id,
        descricao: schema.transactions.description,
        data: schema.transactions.event_date,
        tipo: schema.transactions.type,
        valor: sql<string>`${valorCheio}::text`,
        alocado: sql<string>`${jaAlocado}::text`,
        produto: schema.financial_products.name,
      })
      .from(schema.transactions)
      .leftJoin(soma, eq(soma.transactionId, schema.transactions.id))
      .leftJoin(
        schema.financial_products,
        and(
          eq(schema.financial_products.id, schema.transactions.financial_product_id),
          eq(schema.financial_products.user_id, ownerId),
        ),
      )
      .where(and(saidasRateaveis, sql`${jaAlocado} < ${valorCheio}`))
      .orderBy(desc(schema.transactions.event_date))
      .limit(LIMITE_PENDENTES),
  ]);

  const paraPainel = (centro: (typeof centros)[number]): CentroNoPainel => ({
    id: centro.id,
    nome: centro.name,
    tipo: centro.type,
    status: centro.status,
    descricao: centro.description,
    icone: centro.icon,
    cor: centro.color,
    saldo: centro.balance,
    entrouNoPeriodo: centro.monthInflow,
    saiuNoPeriodo: centro.monthOutflow,
    resultadoNoPeriodo: centro.monthResult,
    rateios: rateiosPorCentro.get(centro.id) ?? 0,
  });

  const saidasDoPeriodo = num(totais[0]?.saidas);
  const alocadoDoPeriodo = num(totais[0]?.alocado);

  return {
    periodo,
    centrosAtivos: centros.filter((centro) => centro.status === "active").map(paraPainel),
    centrosArquivados: centros.filter((centro) => centro.status === "archived").map(paraPainel),
    naoAlocado: {
      total: Math.max(0, saidasDoPeriodo - alocadoDoPeriodo),
      saidasDoPeriodo,
      movimentacoes: num(totais[0]?.movimentacoes),
    },
    pendentes: pendentes.map((linha) => ({
      id: linha.id,
      descricao: linha.descricao ?? "Sem descrição",
      data: linha.data,
      tipo: linha.tipo,
      valor: num(linha.valor),
      alocado: num(linha.alocado),
      naoAlocado: num(linha.valor) - num(linha.alocado),
      produto: linha.produto,
    })),
  };
}

// ══════════════════════════════════════════════════════════════
// Detalhe — /dashboard/centros/[id]
// ══════════════════════════════════════════════════════════════

/** `null` quando o centro não existe ou não é do usuário — a página faz `notFound()`. */
export async function carregarDetalheCentro(centroId: string): Promise<DetalheCentro | null> {
  const { ownerId } = await requireUser();
  const centro = await getFinancialCenter(centroId);
  if (!centro) return null;

  const db = getDb();
  const periodo = await periodoDoUsuario(ownerId);

  const ehEntrada = inArray(schema.transactions.type, [...INFLOW_TRANSACTION_TYPES]);
  const doCentroNoPeriodo = and(
    eq(schema.transaction_allocations.user_id, ownerId),
    eq(schema.transaction_allocations.financial_center_id, centroId),
    gte(schema.transactions.event_date, periodo.inicio),
    lte(schema.transactions.event_date, periodo.fim),
  );

  const [saldos, rateiosPorCentro, totalLinhas, movimentacoes, porDia] = await Promise.all([
    listFinancialCentersWithBalance({ startDate: periodo.inicio, endDate: periodo.fim }),

    contarRateiosPorCentro(ownerId),

    db
      .select({ quantidade: sql<string>`count(*)` })
      .from(schema.transaction_allocations)
      .innerJoin(
        schema.transactions,
        and(
          eq(schema.transactions.id, schema.transaction_allocations.transaction_id),
          eq(schema.transactions.user_id, ownerId),
        ),
      )
      .where(doCentroNoPeriodo),

    db
      .select({
        id: schema.transactions.id,
        descricao: schema.transactions.description,
        data: schema.transactions.event_date,
        tipo: schema.transactions.type,
        valorMovimentacao: schema.transactions.amount,
        valorAlocado: schema.transaction_allocations.amount,
        nota: schema.transaction_allocations.note,
        produto: schema.financial_products.name,
      })
      .from(schema.transaction_allocations)
      .innerJoin(
        schema.transactions,
        and(
          eq(schema.transactions.id, schema.transaction_allocations.transaction_id),
          eq(schema.transactions.user_id, ownerId),
        ),
      )
      .leftJoin(
        schema.financial_products,
        and(
          eq(schema.financial_products.id, schema.transactions.financial_product_id),
          eq(schema.financial_products.user_id, ownerId),
        ),
      )
      .where(doCentroNoPeriodo)
      .orderBy(desc(schema.transactions.event_date), desc(schema.transactions.created_at))
      .limit(LIMITE_MOVIMENTACOES),

    // A evolução vem agregada por dia, não da lista acima: a lista tem corte
    // e a curva ficaria mentindo assim que o centro passasse de 100 rateios.
    db
      .select({
        dia: schema.transactions.event_date,
        saiu: sql<string>`coalesce(sum(${schema.transaction_allocations.amount}) filter (where not (${ehEntrada})), 0)::text`,
      })
      .from(schema.transaction_allocations)
      .innerJoin(
        schema.transactions,
        and(
          eq(schema.transactions.id, schema.transaction_allocations.transaction_id),
          eq(schema.transactions.user_id, ownerId),
        ),
      )
      .where(doCentroNoPeriodo)
      .groupBy(schema.transactions.event_date)
      .orderBy(asc(schema.transactions.event_date)),
  ]);

  const saldo = saldos.find((linha) => linha.id === centroId);

  return {
    centro: {
      id: centro.id,
      nome: centro.name,
      tipo: centro.type,
      status: centro.status,
      descricao: centro.description,
      icone: centro.icon,
      cor: centro.color,
    },
    periodo,
    saldo: saldo?.balance ?? 0,
    entrouSempre: saldo?.inflow ?? 0,
    saiuSempre: saldo?.outflow ?? 0,
    entrouNoPeriodo: saldo?.monthInflow ?? 0,
    saiuNoPeriodo: saldo?.monthOutflow ?? 0,
    resultadoNoPeriodo: saldo?.monthResult ?? 0,
    rateios: rateiosPorCentro.get(centroId) ?? 0,
    totalMovimentacoes: num(totalLinhas[0]?.quantidade),
    movimentacoes: movimentacoes.map((linha) => ({
      id: linha.id,
      descricao: linha.descricao ?? "Sem descrição",
      data: linha.data,
      tipo: linha.tipo,
      direcao: (INFLOW_TRANSACTION_TYPES as readonly string[]).includes(linha.tipo)
        ? "entrada"
        : "saida",
      valorMovimentacao: Math.abs(num(linha.valorMovimentacao)),
      valorAlocado: num(linha.valorAlocado),
      produto: linha.produto,
      nota: linha.nota,
    })),
    evolucao: serieDiaria(periodo, new Map(porDia.map((linha) => [linha.dia, num(linha.saiu)]))),
  };
}

/**
 * Série densa de um ponto por dia do período, com o acumulado.
 *
 * Densa de propósito: uma série só com os dias que tiveram gasto desenharia
 * uma curva que sobe em ritmo constante mesmo com duas semanas paradas no
 * meio — a leitura "o projeto consumiu tudo na primeira semana" sumiria.
 */
function serieDiaria(periodo: PeriodoResumo, porDia: Map<string, number>): PontoEvolucao[] {
  const pontos: PontoEvolucao[] = [];
  // `T12:00:00` evita que o passo de um dia caia no dia anterior por causa do
  // horário de verão.
  const cursor = new Date(`${periodo.inicio}T12:00:00`);
  const fim = new Date(`${periodo.fim}T12:00:00`);
  let acumulado = 0;

  // Teto de segurança: um período mal configurado não pode virar laço infinito.
  for (let passo = 0; passo < 400 && cursor <= fim; passo += 1) {
    const dia = [
      String(cursor.getFullYear()).padStart(4, "0"),
      String(cursor.getMonth() + 1).padStart(2, "0"),
      String(cursor.getDate()).padStart(2, "0"),
    ].join("-");
    const saiu = porDia.get(dia) ?? 0;
    acumulado += saiu;
    pontos.push({ dia, saiu, acumulado });
    cursor.setDate(cursor.getDate() + 1);
  }

  return pontos;
}

// ══════════════════════════════════════════════════════════════
// Rateio — o que a folha de rateio precisa saber de uma movimentação
// ══════════════════════════════════════════════════════════════

/**
 * Tudo que a folha de rateio precisa: a movimentação, os rateios que ela já
 * tem e os centros que podem receber.
 *
 * Existe separado de `getTransactionAllocations` por um motivo só, e é o que
 * importa aqui: o **tipo** da movimentação. Sem ele a tela não tem como
 * explicar que pagamento de fatura não aceita rateio, e o usuário só
 * descobriria depois de preencher tudo e receber o erro do banco.
 */
export async function carregarRateioDaMovimentacao(
  transacaoId: string,
): Promise<RateioDaMovimentacao | null> {
  const { ownerId } = await requireUser();
  const db = getDb();

  const [movimentacao] = await db
    .select({
      id: schema.transactions.id,
      descricao: schema.transactions.description,
      data: schema.transactions.event_date,
      tipo: schema.transactions.type,
      valor: schema.transactions.amount,
      produto: schema.financial_products.name,
    })
    .from(schema.transactions)
    .leftJoin(
      schema.financial_products,
      and(
        eq(schema.financial_products.id, schema.transactions.financial_product_id),
        eq(schema.financial_products.user_id, ownerId),
      ),
    )
    .where(and(eq(schema.transactions.id, transacaoId), eq(schema.transactions.user_id, ownerId)));

  if (!movimentacao) return null;

  const [rateios, centros] = await Promise.all([
    db
      .select({
        centroId: schema.transaction_allocations.financial_center_id,
        valor: schema.transaction_allocations.amount,
        nota: schema.transaction_allocations.note,
      })
      .from(schema.transaction_allocations)
      .where(
        and(
          eq(schema.transaction_allocations.transaction_id, transacaoId),
          eq(schema.transaction_allocations.user_id, ownerId),
        ),
      )
      .orderBy(asc(schema.transaction_allocations.created_at)),
    listFinancialCenters({ status: "active" }),
  ]);

  const valor = Math.abs(num(movimentacao.valor));
  const alocado = rateios.reduce((total, rateio) => total + num(rateio.valor), 0);
  const podeRatear = canAllocateTransactionType(movimentacao.tipo);

  return {
    transacaoId: movimentacao.id,
    descricao: movimentacao.descricao ?? "Sem descrição",
    data: movimentacao.data,
    tipo: movimentacao.tipo,
    valor,
    produto: movimentacao.produto,
    podeRatear,
    motivoNaoPodeRatear: podeRatear
      ? null
      : "Pagamento de fatura não aceita rateio: as compras dentro da fatura já foram rateadas uma a uma. Ratear o pagamento contaria o mesmo dinheiro duas vezes no centro.",
    alocado,
    naoAlocado: valor - alocado,
    rateios: rateios.map((rateio) => ({
      centroId: rateio.centroId,
      valor: num(rateio.valor),
      nota: rateio.nota,
    })),
    centros: centros.map((centro) => ({
      id: centro.id,
      nome: centro.name,
      cor: centro.color,
      icone: centro.icon,
    })),
  };
}
