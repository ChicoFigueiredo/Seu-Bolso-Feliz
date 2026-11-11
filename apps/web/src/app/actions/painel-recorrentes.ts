"use server";

/**
 * Os números da tela Recorrentes: assinaturas, contas de consumo e
 * parcelamentos que se repetem todo mês.
 *
 * Esta tela existe para responder uma pergunta: **quanto do mês já está
 * prometido antes de eu gastar qualquer coisa?** O trabalho aqui não é criar
 * lógica de negócio nova — `apps/web/src/app/actions/recurring.ts` já resolve
 * create/update/delete/generate — é **compor** essas leituras num modelo de
 * tela e normalizar frequências diferentes num único "por mês".
 *
 * A mesma armadilha de dupla contagem da tela Hoje vale aqui
 * (ver painel-hoje.ts): quando uma recorrência vira lançamento,
 * `recurring_instances.transaction_id` aponta para ele. Uma instância
 * "conciliada" (com `transaction_id`) nunca é escolhida como "próxima
 * instância" do card do template — senão o compromisso apareceria como
 * pendente numa tela e já pago noutra.
 *
 * A outra regra que este arquivo protege: **nada estimado vira fato**. Um
 * template de valor variável (conta de luz) ou sem valor cadastrado ainda
 * entra no total mensal comprometido pelo melhor valor disponível — mas o
 * total inteiro sai marcado como estimativa (`totalMensalEstimado`), nunca
 * como número fechado.
 */
import { getDb, schema } from "@sbf/db";
import { eq } from "drizzle-orm";
import { getCurrentPeriod } from "@sbf/domain";
import { requireUser } from "@/lib/auth";
import {
  generateRecurringInstances,
  getRecurringInstances,
  getRecurringTemplate,
  getRecurringTemplates,
} from "./recurring";
import { getCategories } from "./categories";
import { getSuppliers } from "./suppliers";
import { getFinancialProducts } from "./financial-products";
import { paraIso } from "@/lib/datas";

export type TipoRecorrente = "income" | "expense" | "liability_payment" | "statement_payment";
export type FrequenciaRecorrencia =
  "monthly" | "weekly" | "biweekly" | "quarterly" | "annual" | "custom";
export type PrioridadeRecorrencia = "essential" | "high" | "medium" | "low" | "optional";
export type StatusInstanciaRecorrente =
  "pending" | "paid" | "partial" | "skipped" | "overdue" | "cancelled";

export interface ProximaInstanciaResumo {
  id: string;
  data: string;
  valor: number | null;
  /** Já passou da data esperada e ainda está pendente. */
  vencida: boolean;
}

export interface RecorrenteResumo {
  id: string;
  nome: string;
  tipo: TipoRecorrente;
  ativo: boolean;
  /** Valor do template — fixo ou, se `variavel`, a referência/estimativa. */
  valor: number | null;
  /** Conta de consumo: o valor é estimativa, nunca fato. */
  variavel: boolean;
  frequencia: FrequenciaRecorrencia;
  /** Rótulo pronto pra tela — já resolve "todo dia N" / "a cada N dias". */
  frequenciaLabel: string;
  diaDoMes: number | null;
  intervaloDiasCustom: number | null;
  prioridade: PrioridadeRecorrencia | null;
  categoria: string | null;
  fornecedor: string | null;
  produtoFinanceiro: string | null;
  notas: string | null;
  iniciaEm: string | null;
  terminaEm: string | null;
  /** Instância pendente mais próxima, sem transação associada. `null` quando
   *  não há nenhuma gerada ainda — ver `precisaGerarInstancias`. */
  proximaInstancia: ProximaInstanciaResumo | null;
  /** Ativo e sem nenhuma instância pendente futura: a tela precisa oferecer
   *  "gerar instâncias" em vez de fingir que sabe a próxima data. */
  precisaGerarInstancias: boolean;
}

export interface InstanciaPeriodoResumo {
  id: string;
  templateId: string;
  templateNome: string;
  tipo: TipoRecorrente;
  dataEsperada: string;
  valorEsperado: number | null;
  valorReal: number | null;
  status: StatusInstanciaRecorrente;
  dataPagamento: string | null;
  /** Tem `transaction_id`: já virou movimentação de verdade. A defesa contra
   *  dupla contagem começa em mostrar isto, nunca em esconder a instância. */
  conciliada: boolean;
}

export interface PainelRecorrentes {
  periodo: { inicio: string; fim: string; rotulo: string };
  totalMensalComprometido: number;
  /** Há valor variável ou sem valor cadastrado somado ao total: ele é
   *  estimativa, não fato. */
  totalMensalEstimado: boolean;
  contagem: { ativos: number; pausados: number };
  recorrentes: RecorrenteResumo[];
  instancias: InstanciaPeriodoResumo[];
  resumoInstancias: { previstas: number; pagas: number; conciliadas: number };
}

export interface ResultadoGeracao {
  criadas: number;
  templatesProcessados: number;
  falhas: { nome: string; erro: string }[];
}

const num = (v: unknown): number | null => (v == null ? null : Number(v));
const iso = paraIso;

const FREQUENCIA_LABEL: Record<FrequenciaRecorrencia, string> = {
  monthly: "Mensal",
  weekly: "Semanal",
  biweekly: "Quinzenal",
  quarterly: "Trimestral",
  annual: "Anual",
  custom: "Personalizada",
};

/** Rótulo pronto pra tela: prefere ser específico ("todo dia 10") a genérico. */
function labelFrequencia(
  freq: FrequenciaRecorrencia,
  diaDoMes: number | null,
  intervaloDias: number | null,
): string {
  if (freq === "monthly" && diaDoMes) return `Mensal · todo dia ${diaDoMes}`;
  if (freq === "custom" && intervaloDias) {
    return `A cada ${intervaloDias} dia${intervaloDias === 1 ? "" : "s"}`;
  }
  return FREQUENCIA_LABEL[freq] ?? freq;
}

// Média de dias por mês (365.25/12) — só para normalizar frequência
// "personalizada" (intervalo em dias) num equivalente mensal comparável.
const DIAS_POR_MES = 30.4368;

/**
 * Converte um valor recorrente para o equivalente mensal, pra poder somar
 * frequências diferentes (semanal, trimestral, anual...) num único "por mês".
 *
 * Assíncrona só porque este arquivo é "use server": o Next exige que todo
 * export daqui seja função async, mesmo a conta em si sendo síncrona. Fica
 * exportada de propósito para poder testar cada fração isoladamente — ver
 * painel-recorrentes.test.ts.
 */
export async function equivalenteMensal(
  valor: number,
  frequencia: FrequenciaRecorrencia,
  intervaloDiasCustom: number | null,
): Promise<number | null> {
  switch (frequencia) {
    case "monthly":
      return valor;
    case "weekly":
      return (valor * 52) / 12;
    case "biweekly":
      return (valor * 26) / 12;
    case "quarterly":
      return valor / 3;
    case "annual":
      return valor / 12;
    case "custom":
      // Sem o intervalo não dá pra saber quantas vezes por mês isso se
      // repete — melhor admitir que não sabemos do que inventar um número.
      if (!intervaloDiasCustom || intervaloDiasCustom <= 0) return null;
      return (valor * DIAS_POR_MES) / intervaloDiasCustom;
    default:
      return null;
  }
}

async function requireOwnerId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo
  // ator. Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

export async function carregarPainelRecorrentes(): Promise<PainelRecorrentes> {
  const ownerId = await requireOwnerId();
  const db = getDb();
  const hojeIso = iso(new Date());

  // O ciclo financeiro do usuário pode não começar no dia 1 — mesmo cálculo
  // de painel-hoje.ts, pra "instâncias do período" falar do mesmo período.
  const [pref] = await db
    .select({ dia: schema.user_financial_preferences.financial_cycle_start_day })
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, ownerId))
    .limit(1);

  const periodoCalc = getCurrentPeriod({ startDay: pref?.dia ?? 1 }, new Date());
  const inicio = iso(periodoCalc.startDate);
  const fim = iso(periodoCalc.endDate);

  // `false`: também precisamos dos pausados, pro filtro "Todos"/"Pausados" —
  // não é só a lista dos ativos.
  const [templates, categorias, fornecedores, produtos] = await Promise.all([
    getRecurringTemplates(false),
    getCategories(),
    getSuppliers(false),
    getFinancialProducts(),
  ]);

  const categoriaPorId = new Map(categorias.map((c) => [c.id, c.name]));
  const fornecedorPorId = new Map(fornecedores.map((s) => [s.id, s.name]));
  const produtoPorId = new Map(produtos.map((p) => [p.id, p.name]));

  // Duas consultas por template: a mais próxima pendente (pra "próxima
  // data", sem limite de data — uma anual daqui a 4 meses ainda é relevante)
  // e tudo que existe dentro do período atual (pra "instâncias do período").
  const [proximasPorTemplate, periodoPorTemplate] = await Promise.all([
    Promise.all(templates.map((t) => getRecurringInstances(t.id, { status: "pending" }))),
    Promise.all(
      templates.map((t) => getRecurringInstances(t.id, { startDate: inicio, endDate: fim })),
    ),
  ]);

  const recorrentes: RecorrenteResumo[] = [];
  const instancias: InstanciaPeriodoResumo[] = [];
  let totalMensalComprometido = 0;
  let totalMensalEstimado = false;
  let ativos = 0;
  let pausados = 0;

  for (let idx = 0; idx < templates.length; idx++) {
    const t = templates[idx]!;
    if (t.is_active) ativos++;
    else pausados++;

    // A instância pendente mais próxima que AINDA NÃO virou transação. Uma
    // instância com transaction_id já é uma movimentação de verdade —
    // contá-la aqui de novo é a duplicidade que este arquivo existe pra
    // evitar (ver cabeçalho do arquivo).
    const pendentes = proximasPorTemplate[idx] ?? [];
    const proximaBruta = pendentes.find((i) => i.transaction_id == null) ?? null;
    const proximaInstancia: ProximaInstanciaResumo | null = proximaBruta
      ? {
          id: proximaBruta.id,
          data: proximaBruta.expected_date,
          valor: num(proximaBruta.expected_amount),
          vencida: proximaBruta.expected_date < hojeIso,
        }
      : null;

    const valor = num(t.amount);
    const frequencia = t.frequency as FrequenciaRecorrencia;

    // Total comprometido: só recorrentes ativos, e só o que sai do bolso —
    // receita recorrente (salário, cliente fixo) não é um compromisso de
    // gasto, então não entra na soma (ela continua visível na lista, só não
    // infla o número que a tela existe pra dar).
    if (t.is_active && t.type !== "income") {
      if (valor == null) {
        totalMensalEstimado = true;
      } else {
        const equivalente = await equivalenteMensal(valor, frequencia, t.custom_interval_days);
        if (equivalente == null) {
          // "Personalizada" sem intervalo cadastrado: não dá pra normalizar
          // com segurança, então nem soma — só marca a estimativa.
          totalMensalEstimado = true;
        } else {
          totalMensalComprometido += equivalente;
          if (t.is_variable_amount) totalMensalEstimado = true;
        }
      }
    }

    recorrentes.push({
      id: t.id,
      nome: t.name,
      tipo: t.type as TipoRecorrente,
      ativo: t.is_active,
      valor,
      variavel: t.is_variable_amount,
      frequencia,
      frequenciaLabel: labelFrequencia(frequencia, t.day_of_month, t.custom_interval_days),
      diaDoMes: t.day_of_month,
      intervaloDiasCustom: t.custom_interval_days,
      prioridade: t.priority as PrioridadeRecorrencia | null,
      categoria: t.category_id ? (categoriaPorId.get(t.category_id) ?? null) : null,
      fornecedor: t.supplier_id ? (fornecedorPorId.get(t.supplier_id) ?? null) : null,
      produtoFinanceiro: t.financial_product_id
        ? (produtoPorId.get(t.financial_product_id) ?? null)
        : null,
      notas: t.notes,
      iniciaEm: t.starts_at,
      terminaEm: t.ends_at,
      proximaInstancia,
      precisaGerarInstancias: t.is_active && proximaInstancia == null,
    });

    const doPeriodo = periodoPorTemplate[idx] ?? [];
    for (const i of doPeriodo) {
      instancias.push({
        id: i.id,
        templateId: t.id,
        templateNome: t.name,
        tipo: t.type as TipoRecorrente,
        dataEsperada: i.expected_date,
        valorEsperado: num(i.expected_amount),
        valorReal: num(i.actual_amount),
        status: i.status as StatusInstanciaRecorrente,
        dataPagamento: i.paid_date,
        conciliada: i.transaction_id != null,
      });
    }
  }

  // Mais urgente primeiro: quem já tem próxima data definida, do mais
  // próximo ao mais distante; quem ainda precisa gerar instâncias vai pro
  // final, ordenado por nome.
  recorrentes.sort((a, b) => {
    if (a.proximaInstancia && b.proximaInstancia) {
      return a.proximaInstancia.data.localeCompare(b.proximaInstancia.data);
    }
    if (a.proximaInstancia) return -1;
    if (b.proximaInstancia) return 1;
    return a.nome.localeCompare(b.nome, "pt-BR");
  });

  instancias.sort((a, b) => a.dataEsperada.localeCompare(b.dataEsperada));

  const resumoInstancias = {
    previstas: instancias.filter((i) => i.status === "pending").length,
    pagas: instancias.filter((i) => i.status === "paid").length,
    conciliadas: instancias.filter((i) => i.conciliada).length,
  };

  return {
    periodo: { inicio, fim, rotulo: periodoCalc.label },
    totalMensalComprometido,
    totalMensalEstimado,
    contagem: { ativos, pausados },
    recorrentes,
    instancias,
    resumoInstancias,
  };
}

// Passo em dias de cada frequência fixa — "custom" resolve pelo próprio
// intervalo cadastrado no template (ver horizonteGeracao).
const PASSO_DIAS: Record<Exclude<FrequenciaRecorrencia, "custom">, number> = {
  monthly: 31,
  weekly: 7,
  biweekly: 14,
  quarterly: 92,
  annual: 366,
};

/**
 * Horizonte de geração sob medida pra frequência do template: 1,5x o passo
 * garante pelo menos uma ocorrência futura mesmo que a última instância
 * existente já esteja perto do fim do passo, sem gerar uma quantidade
 * exagerada de linhas pra templates de alta frequência (semanal/quinzenal)
 * quando um horizonte fixo e distante for aplicado a todos por igual.
 */
function horizonteGeracao(
  frequencia: FrequenciaRecorrencia,
  intervaloDiasCustom: number | null,
  referencia: Date,
): string {
  const passo = frequencia === "custom" ? (intervaloDiasCustom ?? 30) : PASSO_DIAS[frequencia];
  return iso(new Date(referencia.getTime() + passo * 1.5 * 86400000));
}

/** Gera as próximas instâncias de UM template — usado pelo card que ainda
 *  não tem "próxima data" (`precisaGerarInstancias`). */
export async function gerarInstanciasDoTemplate(templateId: string): Promise<number> {
  const template = await getRecurringTemplate(templateId);
  if (!template) throw new Error("Recorrente não encontrado");
  const ate = horizonteGeracao(
    template.frequency as FrequenciaRecorrencia,
    template.custom_interval_days,
    new Date(),
  );
  return generateRecurringInstances(templateId, ate);
}

/**
 * Gera instâncias pra todos os recorrentes ativos, cada um até o horizonte
 * da própria frequência. Idempotente: `generateRecurringInstances` sempre
 * retoma da última instância existente, então clicar de novo sem nada novo
 * pra gerar só devolve zero — nunca duplica.
 */
export async function gerarInstanciasDoPeriodo(): Promise<ResultadoGeracao> {
  const templates = await getRecurringTemplates(true);
  const referencia = new Date();

  const resultados = await Promise.all(
    templates.map(async (t) => {
      try {
        const ate = horizonteGeracao(
          t.frequency as FrequenciaRecorrencia,
          t.custom_interval_days,
          referencia,
        );
        const criadas = await generateRecurringInstances(t.id, ate);
        return { nome: t.name, ok: true as const, criadas };
      } catch (erro) {
        return {
          nome: t.name,
          ok: false as const,
          erro: erro instanceof Error ? erro.message : String(erro),
        };
      }
    }),
  );

  let criadas = 0;
  const falhas: { nome: string; erro: string }[] = [];
  for (const r of resultados) {
    if (r.ok) criadas += r.criadas;
    else falhas.push({ nome: r.nome, erro: r.erro });
  }

  return { criadas, templatesProcessados: templates.length, falhas };
}
