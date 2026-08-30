"use server";

/**
 * Os números da tela Hoje.
 *
 * Esta tela existe para responder, em cinco segundos: quanto eu tenho, quanto
 * disso é realmente meu, o que vence agora, e como está o mês. O trabalho duro
 * aqui não é buscar dado — é **não contar a mesma coisa duas vezes**.
 *
 * As três armadilhas de dupla contagem, e como cada uma é evitada:
 *
 *   1. **Compra no cartão vs. fatura.** Cada compra é uma linha em
 *      `transactions` com `statement_cycle_id`, e a fatura inteira é uma linha
 *      em `statement_cycles`. Somar as duas dobra a dívida do cartão. Aqui o
 *      compromisso do cartão vem da **fatura**, e as transações com
 *      `statement_cycle_id` são excluídas dos compromissos avulsos.
 *
 *   2. **Recorrente vs. movimentação gerada por ele.** Quando uma recorrência
 *      vira lançamento, `recurring_instances.transaction_id` aponta para ele.
 *      Só entram as instâncias pendentes **sem** transação associada.
 *
 *   3. **Parcela de dívida vs. movimentação de pagamento.** A parcela tem
 *      `liability_installment_id` na transação. O compromisso mensal de dívidas
 *      é informativo e **não** entra no "livre após compromissos" — senão a
 *      parcela deste mês seria contada como transação futura e como dívida.
 *
 * "Disponível agora", "Livre após compromissos" e "Resultado do mês" são três
 * coisas diferentes e estão separadas de propósito. Ver CONTEXT.md.
 */
import { getDb, schema } from "@sbf/db";
import { and, eq, gte, inArray, isNull, isNotNull, lte, ne, sql, asc, type SQL } from "drizzle-orm";
import { getCurrentPeriod } from "@sbf/domain";
import { requireUser } from "@/lib/auth";

const CONTAS = ["checking_account", "savings_account"] as const;
const SAIDAS = [
  "expense",
  "fee",
  "interest_charge",
  "statement_payment",
  "liability_payment",
] as const;

export interface ContaResumo {
  id: string;
  nome: string;
  instituicao: string | null;
  saldo: number;
}

export interface CartaoResumo {
  id: string;
  nome: string;
  instituicao: string | null;
  faturaAtual: number;
  limiteTotal: number | null;
  limiteLivre: number | null;
  fechamento: string | null;
  vencimento: string | null;
}

export interface CompromissoResumo {
  id: string;
  descricao: string;
  valor: number;
  data: string;
  origem: "movimentacao" | "recorrente" | "fatura";
  produto: string | null;
  vencido: boolean;
  valorEstimado: boolean;
}

export interface CentroResumo {
  id: string;
  nome: string;
  icone: string | null;
  gastoNoMes: number;
  saldo: number;
}

export interface PainelHoje {
  periodo: { inicio: string; fim: string; rotulo: string };
  disponivelAgora: number;
  livreAposCompromissos: number;
  /** Há compromisso sem valor confirmado: o "livre" é estimativa, não fato. */
  estimativaIncompleta: boolean;
  contas: ContaResumo[];
  cartoes: CartaoResumo[];
  dividaTotal: number;
  compromissoMensalDividas: number;
  proximos: CompromissoResumo[];
  faltaAteFimDoPeriodo: number;
  mes: { entrou: number; saiu: number; aindaVaiSair: number; livreProjetado: number };
  pendencias: { rotulo: string; quantidade: number; rota: string }[];
  centros: CentroResumo[];
}

const iso = (d: Date) => d.toISOString().slice(0, 10);
const num = (v: unknown) => (v == null ? 0 : Number(v));

export async function carregarPainelHoje(): Promise<PainelHoje> {
  const { ownerId } = await requireUser();
  const db = getDb();
  const hoje = new Date();

  // O ciclo financeiro do usuário pode não começar no dia 1. Sem preferência
  // gravada, o mês-calendário é o padrão honesto.
  const [pref] = await db
    .select({ dia: schema.user_financial_preferences.financial_cycle_start_day })
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, ownerId))
    .limit(1);

  const periodo = getCurrentPeriod({ startDay: pref?.dia ?? 1 }, hoje);
  const inicio = iso(periodo.startDate);
  const fim = iso(periodo.endDate);
  const agora = iso(hoje);

  const [produtos, ciclosAbertos, dividas, movimentos, recorrentes, centros] = await Promise.all([
    db
      .select({
        id: schema.financial_products.id,
        nome: schema.financial_products.name,
        tipo: schema.financial_products.type,
        saldo: schema.financial_products.current_balance,
        limite: schema.financial_products.credit_limit,
        instituicao: schema.institutions.name,
      })
      .from(schema.financial_products)
      .leftJoin(
        schema.institutions,
        and(
          eq(schema.institutions.id, schema.financial_products.institution_id),
          eq(schema.institutions.user_id, ownerId),
        ),
      )
      .where(
        and(
          eq(schema.financial_products.user_id, ownerId),
          eq(schema.financial_products.is_active, true),
        ),
      )
      .orderBy(asc(schema.financial_products.display_order), asc(schema.financial_products.name)),

    db
      .select({
        id: schema.statement_cycles.id,
        cartaoId: schema.statement_cycles.card_id,
        total: schema.statement_cycles.total_amount,
        pago: schema.statement_cycles.paid_amount,
        fechamento: schema.statement_cycles.cycle_end_date,
        vencimento: schema.statement_cycles.due_date,
      })
      .from(schema.statement_cycles)
      .where(
        and(
          eq(schema.statement_cycles.user_id, ownerId),
          ne(schema.statement_cycles.status, "paid"),
        ),
      )
      .orderBy(asc(schema.statement_cycles.due_date)),

    db
      .select({
        saldoDevedor: schema.liabilities.outstanding_balance,
        parcelas: schema.liabilities.total_installments,
        pagas: schema.liabilities.paid_installments,
        original: schema.liabilities.original_amount,
      })
      .from(schema.liabilities)
      .where(and(eq(schema.liabilities.user_id, ownerId), eq(schema.liabilities.status, "active"))),

    // Movimentações do período + compromissos avulsos não confirmados.
    // `statement_cycle_id IS NULL` é a guarda contra contar a compra do cartão
    // junto com a fatura que já a contém.
    db
      .select({
        id: schema.transactions.id,
        descricao: schema.transactions.description,
        valor: schema.transactions.amount,
        data: schema.transactions.event_date,
        tipo: schema.transactions.type,
        confirmada: schema.transactions.is_confirmed,
        cicloId: schema.transactions.statement_cycle_id,
        parcelaId: schema.transactions.liability_installment_id,
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
      .where(
        and(
          eq(schema.transactions.user_id, ownerId),
          gte(schema.transactions.event_date, inicio),
          lte(schema.transactions.event_date, fim),
        ),
      )
      .orderBy(asc(schema.transactions.event_date)),

    db
      .select({
        id: schema.recurring_instances.id,
        valor: schema.recurring_instances.expected_amount,
        data: schema.recurring_instances.expected_date,
        nome: schema.recurring_templates.name,
      })
      .from(schema.recurring_instances)
      .innerJoin(
        schema.recurring_templates,
        and(
          eq(schema.recurring_templates.id, schema.recurring_instances.recurring_template_id),
          eq(schema.recurring_templates.user_id, ownerId),
        ),
      )
      .where(
        and(
          eq(schema.recurring_instances.user_id, ownerId),
          eq(schema.recurring_instances.status, "pending"),
          // Já virou lançamento? Então o lançamento é que conta.
          isNull(schema.recurring_instances.transaction_id),
          lte(schema.recurring_instances.expected_date, fim),
        ),
      )
      .orderBy(asc(schema.recurring_instances.expected_date)),

    db
      .select({
        id: schema.financial_centers.id,
        nome: schema.financial_centers.name,
        icone: schema.financial_centers.icon,
      })
      .from(schema.financial_centers)
      .where(
        and(
          eq(schema.financial_centers.user_id, ownerId),
          eq(schema.financial_centers.status, "active"),
        ),
      )
      .orderBy(asc(schema.financial_centers.name))
      .limit(6),
  ]);

  // ── Posição ────────────────────────────────────────────────────────────
  const contas: ContaResumo[] = produtos
    .filter((p) => (CONTAS as readonly string[]).includes(p.tipo))
    .map((p) => ({
      id: p.id,
      nome: p.nome,
      instituicao: p.instituicao,
      saldo: num(p.saldo),
    }));

  const disponivelAgora = contas.reduce((s, c) => s + c.saldo, 0);

  const cicloPorCartao = new Map(ciclosAbertos.map((c) => [c.cartaoId, c]));
  const cartoes: CartaoResumo[] = produtos
    .filter((p) => p.tipo === "credit_card")
    .map((p) => {
      const ciclo = cicloPorCartao.get(p.id);
      const fatura = ciclo ? num(ciclo.total) - num(ciclo.pago) : 0;
      const limite = p.limite == null ? null : num(p.limite);
      return {
        id: p.id,
        nome: p.nome,
        instituicao: p.instituicao,
        faturaAtual: fatura,
        limiteTotal: limite,
        limiteLivre: limite == null ? null : limite - fatura,
        fechamento: ciclo?.fechamento ?? null,
        vencimento: ciclo?.vencimento ?? null,
      };
    });

  const dividaTotal = dividas.reduce((s, d) => s + num(d.saldoDevedor), 0);
  const compromissoMensalDividas = dividas.reduce((s, d) => {
    const restantes = (d.parcelas ?? 0) - (d.pagas ?? 0);
    return restantes > 0 ? s + num(d.saldoDevedor) / restantes : s;
  }, 0);

  // ── Compromissos ───────────────────────────────────────────────────────
  const compromissos: CompromissoResumo[] = [];

  for (const m of movimentos) {
    if (m.confirmada) continue;
    if (!(SAIDAS as readonly string[]).includes(m.tipo)) continue;
    // A compra do cartão já está dentro da fatura; a parcela de dívida já está
    // no compromisso mensal de dívidas. Contar aqui de novo dobraria as duas.
    if (m.cicloId || m.parcelaId) continue;
    compromissos.push({
      id: m.id,
      descricao: m.descricao ?? "Sem descrição",
      valor: num(m.valor),
      data: m.data,
      origem: "movimentacao",
      produto: m.produto,
      vencido: m.data < agora,
      valorEstimado: false,
    });
  }

  for (const r of recorrentes) {
    compromissos.push({
      id: r.id,
      descricao: r.nome,
      valor: num(r.valor),
      data: r.data,
      origem: "recorrente",
      produto: null,
      vencido: r.data < agora,
      // Recorrência sem valor esperado é justamente o caso que torna o
      // "livre" uma estimativa em vez de um fato.
      valorEstimado: r.valor == null,
    });
  }

  for (const c of ciclosAbertos) {
    const aberto = num(c.total) - num(c.pago);
    if (aberto <= 0) continue;
    const cartao = cartoes.find((x) => x.id === c.cartaoId);
    compromissos.push({
      id: c.id,
      descricao: `Fatura ${cartao?.nome ?? "do cartão"}`,
      valor: aberto,
      data: c.vencimento,
      origem: "fatura",
      produto: cartao?.nome ?? null,
      vencido: c.vencimento < agora,
      valorEstimado: false,
    });
  }

  compromissos.sort((a, b) => a.data.localeCompare(b.data));

  const totalCompromissos = compromissos.reduce((s, c) => s + c.valor, 0);
  const estimativaIncompleta = compromissos.some((c) => c.valorEstimado);
  const livreAposCompromissos = disponivelAgora - totalCompromissos;

  const aindaVaiSair = compromissos
    .filter((c) => c.data >= agora && c.data <= fim)
    .reduce((s, c) => s + c.valor, 0);

  // "Até o fim do período você ainda precisa de R$ X": só o que falta de
  // dinheiro, nunca um número negativo dizendo que sobra.
  const faltaAteFimDoPeriodo = Math.max(0, aindaVaiSair - disponivelAgora);

  // ── Resultado do mês ───────────────────────────────────────────────────
  let entrou = 0;
  let saiu = 0;
  for (const m of movimentos) {
    if (!m.confirmada) continue;
    if (m.tipo === "income" || m.tipo === "refund") entrou += num(m.valor);
    else if ((SAIDAS as readonly string[]).includes(m.tipo)) saiu += num(m.valor);
  }

  // ── Pendências ─────────────────────────────────────────────────────────
  const [pendencias, gastoPorCentro] = await Promise.all([
    contarPendencias(ownerId, agora),
    db
      .select({
        centroId: schema.transaction_allocations.financial_center_id,
        gasto: sql<number>`coalesce(sum(${schema.transaction_allocations.amount}), 0)`,
      })
      .from(schema.transaction_allocations)
      .innerJoin(
        schema.transactions,
        eq(schema.transactions.id, schema.transaction_allocations.transaction_id),
      )
      .where(
        and(
          eq(schema.transaction_allocations.user_id, ownerId),
          inArray(schema.transactions.type, [...SAIDAS]),
          gte(schema.transactions.event_date, inicio),
          lte(schema.transactions.event_date, fim),
        ),
      )
      .groupBy(schema.transaction_allocations.financial_center_id),
  ]);

  const gastoMap = new Map(gastoPorCentro.map((g) => [g.centroId, num(g.gasto)]));

  // Saldo do Centro como conta corrente virtual: tudo que entrou menos tudo
  // que saiu, desde sempre — não só no período. É o número que responde
  // "quanto o Edulingo me deve ou me sobrou".
  const saldoPorCentro = await db
    .select({
      centroId: schema.transaction_allocations.financial_center_id,
      saldo: sql<number>`coalesce(sum(
        case when ${schema.transactions.type} in ('income', 'refund')
             then ${schema.transaction_allocations.amount}
             else -${schema.transaction_allocations.amount} end), 0)`,
    })
    .from(schema.transaction_allocations)
    .innerJoin(
      schema.transactions,
      eq(schema.transactions.id, schema.transaction_allocations.transaction_id),
    )
    .where(eq(schema.transaction_allocations.user_id, ownerId))
    .groupBy(schema.transaction_allocations.financial_center_id);

  const saldoMap = new Map(saldoPorCentro.map((s) => [s.centroId, num(s.saldo)]));

  return {
    periodo: { inicio, fim, rotulo: periodo.label },
    disponivelAgora,
    livreAposCompromissos,
    estimativaIncompleta,
    contas,
    cartoes,
    dividaTotal,
    compromissoMensalDividas,
    proximos: compromissos.slice(0, 8),
    faltaAteFimDoPeriodo,
    mes: {
      entrou,
      saiu,
      aindaVaiSair,
      // Livre projetado é a posição no fim do período: o que há hoje menos o
      // que ainda sai. Não soma receita futura — receita prevista que não
      // entrou não é dinheiro, e apresentá-la como tal é o erro que faz alguém
      // gastar o que não tem.
      livreProjetado: disponivelAgora - aindaVaiSair,
    },
    pendencias,
    centros: centros.map((c) => ({
      id: c.id,
      nome: c.nome,
      icone: c.icone,
      gastoNoMes: gastoMap.get(c.id) ?? 0,
      saldo: saldoMap.get(c.id) ?? 0,
    })),
  };
}

/**
 * Contagem das pendências da §7.4 do brief. Cada uma é uma consulta barata de
 * `count`; nenhuma traz linha para a memória.
 */
async function contarPendencias(
  ownerId: string,
  agora: string,
): Promise<{ rotulo: string; quantidade: number; rota: string }[]> {
  const db = getDb();
  const conta = async (condicao: SQL | undefined) => {
    const [linha] = await db
      .select({ n: sql<number>`count(*)` })
      .from(schema.transactions)
      .where(condicao);
    return num(linha?.n);
  };

  const [vencidas, semCategoria, semCentro, naoConfirmadas] = await Promise.all([
    conta(
      and(
        eq(schema.transactions.user_id, ownerId),
        eq(schema.transactions.is_confirmed, false),
        lte(schema.transactions.event_date, agora),
        inArray(schema.transactions.type, [...SAIDAS]),
      ),
    ),
    conta(
      and(
        eq(schema.transactions.user_id, ownerId),
        isNull(schema.transactions.category_id),
        inArray(schema.transactions.type, [...SAIDAS]),
      ),
    ),
    conta(
      and(
        eq(schema.transactions.user_id, ownerId),
        inArray(schema.transactions.type, [...SAIDAS]),
        sql`not exists (select 1 from public.transaction_allocations a where a.transaction_id = ${schema.transactions.id})`,
      ),
    ),
    conta(
      and(
        eq(schema.transactions.user_id, ownerId),
        eq(schema.transactions.is_confirmed, false),
        isNotNull(schema.transactions.id),
      ),
    ),
  ]);

  const [documentos] = await db
    .select({ n: sql<number>`count(*)` })
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.user_id, ownerId),
        inArray(schema.ingestion_jobs.status, ["pending_review", "drafted"]),
      ),
    );

  return [
    { rotulo: "Vencidas", quantidade: vencidas, rota: "/dashboard/movimentacoes?filtro=vencidas" },
    {
      rotulo: "Sem categoria",
      quantidade: semCategoria,
      rota: "/dashboard/movimentacoes?filtro=sem-categoria",
    },
    {
      rotulo: "Sem centro",
      quantidade: semCentro,
      rota: "/dashboard/movimentacoes?filtro=sem-centro",
    },
    {
      rotulo: "Não confirmadas",
      quantidade: naoConfirmadas,
      rota: "/dashboard/movimentacoes?filtro=nao-confirmadas",
    },
    {
      rotulo: "Precisam de revisão",
      quantidade: num(documentos?.n),
      rota: "/dashboard/documentos",
    },
  ].filter((p) => p.quantidade > 0);
}
