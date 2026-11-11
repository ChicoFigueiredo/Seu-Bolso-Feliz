"use server";

/**
 * Os dados da tela Dívidas: "quanto falta, e quando acaba".
 *
 * Este arquivo só existe porque a tela precisa de leituras agregadas que
 * nenhuma action existente entrega pronta (o mesmo motivo de
 * `painel-hoje.ts`) e de duas escritas que vão além do CRUD de uma linha só
 * que `liabilities.ts` já cobre:
 *
 *   - `gerarCronogramaAutomatico` — usa a matemática de amortização de
 *     `@sbf/domain` (SAC/Price/Misto) para popular de uma vez as parcelas de
 *     uma dívida que só tem os termos (principal, taxa, sistema, nº de
 *     parcelas, data de início), em vez de o usuário criar uma a uma.
 *   - `marcarParcelaComoPaga` — pagar uma parcela também abate o saldo
 *     devedor e soma em `paid_installments`; sem isso o "quanto falta" do
 *     topo nunca se moveria conforme o usuário fosse pagando.
 *
 * Fato vs. estimativa (regra do produto, não decoração): toda vez que este
 * arquivo não tem um dado concreto para "próxima parcela" ou "quando
 * termina", ele projeta a partir do que existe (parcelas restantes, cadência
 * mensal) e marca `estimativa: true` com a premissa em `premissa`. A tela
 * nunca deve apresentar essa projeção como se fosse a data contratual.
 */

import { getDb, schema } from "@sbf/db";
import { and, asc, eq, inArray } from "drizzle-orm";
import { generateSchedule, type AmortizationConfig, type AmortizationSystem } from "@sbf/domain";
import { requireUser } from "@/lib/auth";
import { hojeIso } from "@/lib/datas";

type Db = ReturnType<typeof getDb>;

const num = (v: unknown): number => (v == null ? 0 : Number(v));
const isoHoje = hojeIso;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo
  // ator. Ver apps/web/src/lib/auth/index.ts e o mesmo padrão em
  // apps/web/src/app/actions/liabilities.ts::requireUserId.
  const { ownerId } = await requireUser();
  return ownerId;
}

// ══════════════════════════════════════════════════════════════
// Tipos
// ══════════════════════════════════════════════════════════════

export type SituacaoParcela = "paga" | "parcial" | "dispensada" | "atrasada" | "a_vencer";

export interface PrevisaoFim {
  data: string;
  /** `true` quando a data depende de premissa (parcelas restantes, cadência) em vez de um dado gravado. */
  estimativa: boolean;
  /** Obrigatório quando `estimativa` é `true` — a regra do produto é nunca projetar em silêncio. */
  premissa?: string;
}

export interface DividaResumo {
  id: string;
  nome: string;
  /** Fornecedor da dívida quando existe; senão a instituição do produto financeiro; senão "—". */
  credor: string;
  tipo: string;
  status: string;
  saldoDevedor: number;
  valorOriginal: number;
  parcelasPagas: number;
  parcelasTotais: number | null;
  /** Fração quitada, 0 a 1 — por contagem de parcelas quando há total definido, senão por valor. */
  progresso: number;
  valorParcela: number | null;
  /** `true` quando não há uma próxima parcela lançada e o valor veio de saldo/parcelas restantes. */
  valorParcelaEstimado: boolean;
  proximoVencimento: string | null;
  proximoVencido: boolean;
  previsaoFim: PrevisaoFim | null;
  taxa: { valor: number; tipo: string } | null;
  sistemaAmortizacao: string | null;
  inicio: string | null;
  /** `end_date` gravado, se houver — distinto de `previsaoFim.data`, que pode ser projeção. Para pré-preencher o formulário de edição. */
  fimGravado: string | null;
}

export interface ParcelaDetalhe {
  id: string;
  numero: number;
  vencimento: string;
  valor: number;
  valorPago: number;
  dataPagamento: string | null;
  status: string;
  situacao: SituacaoParcela;
}

export interface PreviaCronograma {
  parcelas: number;
  primeiraParcela: number;
  premissa: string;
}

export interface DetalheDivida extends DividaResumo {
  produtoNome: string | null;
  instituicaoNome: string | null;
  fornecedorNome: string | null;
  financialProductId: string;
  parcelas: ParcelaDetalhe[];
  /** Preenchido só quando a dívida ainda não tem nenhuma parcela lançada e tem dados suficientes para projetar. */
  previaCronograma: PreviaCronograma | null;
}

export interface PainelDividas {
  /** Soma do saldo devedor das dívidas ativas — renegociadas/inadimplentes ficam fora, como em `painel-hoje.ts`. */
  dividaTotal: number;
  compromissoMensal: number;
  compromissoMensalEstimado: boolean;
  /** Há dívida ativa sem valor de parcela algum (nem lançada, nem projetável) — o total do topo subestima a realidade. */
  compromissoIncompleto: boolean;
  fimDoUltimoCompromisso: PrevisaoFim | null;
  /** Há dívida ativa sem previsão de fim alguma. */
  fimIncompleto: boolean;
  ativas: DividaResumo[];
  /** Renegociadas e inadimplentes — ainda pesam no bolso, por isso aparecem, mas fora dos totais do topo. */
  atencao: DividaResumo[];
  quitadas: DividaResumo[];
  temAlgumaDivida: boolean;
}

interface DividaBruta {
  id: string;
  nome: string;
  tipo: string;
  status: string;
  original: number;
  saldo: number;
  taxa: number | null;
  tipoTaxa: string | null;
  sistema: string | null;
  parcelasTotais: number | null;
  parcelasPagas: number;
  inicio: string | null;
  fim: string | null;
  instituicao: string | null;
  produto: string | null;
  fornecedor: string | null;
}

interface ParcelaBruta {
  id: string;
  liabilityId: string;
  numero: number;
  vencimento: string;
  valor: number;
  valorPago: number;
  dataPagamento: string | null;
  status: string;
}

// ══════════════════════════════════════════════════════════════
// Cálculo puro (sem I/O) — o que dá pra testar sem banco
// ══════════════════════════════════════════════════════════════

function situacaoParcela(status: string, vencimento: string, hojeIso: string): SituacaoParcela {
  if (status === "paid") return "paga";
  if (status === "waived") return "dispensada";
  if (status === "partial") return "parcial";
  if (vencimento < hojeIso) return "atrasada";
  return "a_vencer";
}

/** Soma meses a uma data ISO (`YYYY-MM-DD`) sem passar por UTC — mesma cautela de fuso horário de `financial-centers.ts::mesCivilDe`. */
function somaMesesISO(dataISO: string, meses: number): string {
  const [ano, mes, dia] = dataISO.split("-").map(Number) as [number, number, number];
  const indiceMesAlvo = mes - 1 + meses;
  const data = new Date(ano, indiceMesAlvo, dia);
  // Dia que não existe no mês alvo (ex.: 31 de fevereiro) estoura pro mês
  // seguinte em vez de ser recusado — a correção é usar o último dia do mês
  // alvo, não deixar a parcela pular um mês inteiro.
  const mesNormalizado = ((indiceMesAlvo % 12) + 12) % 12;
  if (data.getMonth() !== mesNormalizado) {
    return isoDeData(new Date(ano, indiceMesAlvo + 1, 0));
  }
  return isoDeData(data);
}

function isoDeData(d: Date): string {
  const ano = String(d.getFullYear()).padStart(4, "0");
  const mes = String(d.getMonth() + 1).padStart(2, "0");
  const dia = String(d.getDate()).padStart(2, "0");
  return `${ano}-${mes}-${dia}`;
}

function ordenarPorUrgencia(a: DividaResumo, b: DividaResumo): number {
  if (a.proximoVencido !== b.proximoVencido) return a.proximoVencido ? -1 : 1;
  if (a.proximoVencimento && b.proximoVencimento) {
    return a.proximoVencimento.localeCompare(b.proximoVencimento);
  }
  if (a.proximoVencimento) return -1;
  if (b.proximoVencimento) return 1;
  return a.nome.localeCompare(b.nome);
}

/**
 * Projeta quando a dívida termina. Ordem de confiança: data gravada > último
 * vencimento de um cronograma completo > projeção por parcelas restantes.
 * Só os dois primeiros são fato; o terceiro sempre volta com `estimativa: true`.
 */
function projetarFimDaDivida(
  l: DividaBruta,
  parcelas: readonly ParcelaBruta[],
  hojeIso: string,
): PrevisaoFim | null {
  if (l.fim) return { data: l.fim, estimativa: false };

  const total = l.parcelasTotais;
  if (!total || total <= 0) return null;

  if (parcelas.length >= total) {
    const ultima = parcelas.reduce((max, p) => (p.vencimento > max ? p.vencimento : max), "");
    if (ultima) return { data: ultima, estimativa: false };
  }

  const restantes = Math.max(0, total - (l.parcelasPagas ?? 0));
  if (restantes <= 0) return null;

  return {
    data: somaMesesISO(hojeIso, restantes),
    estimativa: true,
    premissa: `estimativa: ${restantes} parcela${restantes === 1 ? "" : "s"} restante${restantes === 1 ? "" : "s"}, uma por mês a partir de hoje, no ritmo atual`,
  };
}

function montarResumo(
  l: DividaBruta,
  parcelas: readonly ParcelaBruta[],
  hojeIso: string,
): DividaResumo {
  const emAberto = parcelas.filter((p) => p.status !== "paid" && p.status !== "waived");
  const proxima = [...emAberto].sort((a, b) => a.vencimento.localeCompare(b.vencimento))[0] ?? null;

  const totalParcelas = l.parcelasTotais ?? null;
  const parcelasPagas = l.parcelasPagas ?? 0;
  const restantes = totalParcelas != null ? Math.max(0, totalParcelas - parcelasPagas) : null;

  const saldo = num(l.saldo);
  const original = num(l.original);

  const valorParcelaEstimado = proxima == null;
  const valorParcela = proxima
    ? num(proxima.valor)
    : restantes && restantes > 0
      ? saldo / restantes
      : null;

  const progresso =
    totalParcelas != null && totalParcelas > 0
      ? Math.max(0, Math.min(1, parcelasPagas / totalParcelas))
      : original > 0
        ? Math.max(0, Math.min(1, 1 - saldo / original))
        : 0;

  return {
    id: l.id,
    nome: l.nome,
    credor: l.fornecedor ?? l.instituicao ?? "—",
    tipo: l.tipo,
    status: l.status,
    saldoDevedor: saldo,
    valorOriginal: original,
    parcelasPagas,
    parcelasTotais: totalParcelas,
    progresso,
    valorParcela,
    valorParcelaEstimado,
    proximoVencimento: proxima?.vencimento ?? null,
    proximoVencido: proxima ? proxima.vencimento < hojeIso : false,
    previsaoFim: projetarFimDaDivida(l, parcelas, hojeIso),
    taxa: l.taxa != null ? { valor: num(l.taxa), tipo: l.tipoTaxa ?? "monthly" } : null,
    sistemaAmortizacao: l.sistema,
    inicio: l.inicio,
    fimGravado: l.fim,
  };
}

const SISTEMAS_COM_TAXA = new Set<string>(["sac", "price", "mixed"]);

/**
 * Resolve os parâmetros para `generateSchedule` (`@sbf/domain`) a partir dos
 * termos gravados na dívida. `null` quando falta o mínimo para projetar
 * (total de parcelas e data de início) — a chamada não inventa nenhum dos
 * dois.
 */
function resolverConfigAmortizacao(l: {
  original: number;
  taxa: number | null;
  tipoTaxa: string | null;
  sistema: string | null;
  parcelasTotais: number | null;
  inicio: string | null;
}): { config: AmortizationConfig; premissa: string } | null {
  const total = l.parcelasTotais;
  if (!total || total <= 0) return null;
  if (!l.inicio) return null;

  if (l.sistema === "none") {
    // "Nenhum sistema de amortização" é uma escolha explícita do usuário, não
    // ausência de dado — tratamos como parcelas iguais e ignoramos qualquer
    // taxa perdida no cadastro, porque "nenhum sistema" não define como ela
    // se aplicaria.
    return {
      config: {
        system: "price",
        principalAmount: l.original,
        monthlyRate: 0,
        totalInstallments: total,
      },
      premissa: "sem sistema de amortização definido para esta dívida: parcelas iguais, sem juros",
    };
  }

  const system: AmortizationSystem = SISTEMAS_COM_TAXA.has(l.sistema ?? "")
    ? (l.sistema as AmortizationSystem)
    : "price"; // "other"/não definido: Price com taxa 0 (se houver) é a divisão neutra.

  const taxaBruta = l.taxa != null ? l.taxa : 0;
  const monthlyRate = l.tipoTaxa === "annual" ? Math.pow(1 + taxaBruta, 1 / 12) - 1 : taxaBruta;

  return {
    config: { system, principalAmount: l.original, monthlyRate, totalInstallments: total },
    premissa:
      monthlyRate > 0
        ? `sistema ${system.toUpperCase()}, taxa de ${(monthlyRate * 100).toFixed(2)}% a.m.`
        : "sem taxa de juros informada: parcelas iguais",
  };
}

// ══════════════════════════════════════════════════════════════
// Leitura
// ══════════════════════════════════════════════════════════════

const SELECAO_DIVIDA = {
  id: schema.liabilities.id,
  nome: schema.liabilities.name,
  tipo: schema.liabilities.type,
  status: schema.liabilities.status,
  original: schema.liabilities.original_amount,
  saldo: schema.liabilities.outstanding_balance,
  taxa: schema.liabilities.interest_rate,
  tipoTaxa: schema.liabilities.rate_type,
  sistema: schema.liabilities.amortization_system,
  parcelasTotais: schema.liabilities.total_installments,
  parcelasPagas: schema.liabilities.paid_installments,
  inicio: schema.liabilities.start_date,
  fim: schema.liabilities.end_date,
  financialProductId: schema.liabilities.financial_product_id,
  instituicao: schema.institutions.name,
  produto: schema.financial_products.name,
  fornecedor: schema.suppliers.name,
} as const;

function baseDividasQuery(db: Db) {
  return db
    .select(SELECAO_DIVIDA)
    .from(schema.liabilities)
    .leftJoin(
      schema.financial_products,
      and(
        eq(schema.financial_products.id, schema.liabilities.financial_product_id),
        eq(schema.financial_products.user_id, schema.liabilities.user_id),
      ),
    )
    .leftJoin(
      schema.institutions,
      and(
        eq(schema.institutions.id, schema.financial_products.institution_id),
        eq(schema.institutions.user_id, schema.liabilities.user_id),
      ),
    )
    .leftJoin(
      schema.suppliers,
      and(
        eq(schema.suppliers.id, schema.liabilities.supplier_id),
        eq(schema.suppliers.user_id, schema.liabilities.user_id),
      ),
    );
}

export async function carregarPainelDividas(): Promise<PainelDividas> {
  const ownerId = await requireUserId();
  const db = getDb();
  const hojeIso = isoHoje();

  const linhas = await baseDividasQuery(db)
    .where(eq(schema.liabilities.user_id, ownerId))
    .orderBy(asc(schema.liabilities.name));

  const idsAtivos = linhas.filter((l) => l.status === "active").map((l) => l.id);

  // Só busca parcelas das dívidas ativas: quitadas/renegociadas/inadimplentes
  // não entram nos números do topo, e o cronograma completo de cada uma fica
  // pra tela de detalhe — carregar tudo aqui pra listar cards seria puxar
  // centenas de linhas que a lista nunca usa.
  const parcelas: ParcelaBruta[] =
    idsAtivos.length > 0
      ? await db
          .select({
            id: schema.liability_installments.id,
            liabilityId: schema.liability_installments.liability_id,
            numero: schema.liability_installments.installment_number,
            vencimento: schema.liability_installments.due_date,
            valor: schema.liability_installments.total_amount,
            valorPago: schema.liability_installments.paid_amount,
            dataPagamento: schema.liability_installments.paid_date,
            status: schema.liability_installments.status,
          })
          .from(schema.liability_installments)
          .where(
            and(
              eq(schema.liability_installments.user_id, ownerId),
              inArray(schema.liability_installments.liability_id, idsAtivos),
            ),
          )
          .orderBy(asc(schema.liability_installments.due_date))
      : [];

  const porDivida = new Map<string, ParcelaBruta[]>();
  for (const p of parcelas) {
    const lista = porDivida.get(p.liabilityId) ?? [];
    lista.push(p);
    porDivida.set(p.liabilityId, lista);
  }

  const resumos = linhas.map((l) => montarResumo(l, porDivida.get(l.id) ?? [], hojeIso));

  const ativas = resumos.filter((r) => r.status === "active").sort(ordenarPorUrgencia);
  const atencao = resumos
    .filter((r) => r.status === "renegotiated" || r.status === "defaulted")
    .sort(ordenarPorUrgencia);
  const quitadas = resumos.filter((r) => r.status === "paid_off");

  const dividaTotal = ativas.reduce((s, r) => s + r.saldoDevedor, 0);

  const valoresParcela = ativas.map((r) => r.valorParcela).filter((v): v is number => v != null);
  const compromissoMensal = valoresParcela.reduce((s, v) => s + v, 0);
  const compromissoMensalEstimado = ativas.some(
    (r) => r.valorParcela != null && r.valorParcelaEstimado,
  );
  const compromissoIncompleto = ativas.some((r) => r.valorParcela == null);

  const previsoes = ativas.map((r) => r.previsaoFim).filter((p): p is PrevisaoFim => p != null);
  const fimDoUltimoCompromisso =
    previsoes.length > 0 ? previsoes.reduce((max, p) => (p.data > max.data ? p : max)) : null;
  const fimIncompleto = ativas.some((r) => r.previsaoFim == null);

  return {
    dividaTotal,
    compromissoMensal,
    compromissoMensalEstimado,
    compromissoIncompleto,
    fimDoUltimoCompromisso,
    fimIncompleto,
    ativas,
    atencao,
    quitadas,
    temAlgumaDivida: linhas.length > 0,
  };
}

export async function carregarDetalheDivida(id: string): Promise<DetalheDivida | null> {
  const ownerId = await requireUserId();
  const db = getDb();
  const hojeIso = isoHoje();

  const [row] = await baseDividasQuery(db).where(
    and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, ownerId)),
  );
  if (!row) return null;

  const parcelasRows = await db
    .select({
      id: schema.liability_installments.id,
      liabilityId: schema.liability_installments.liability_id,
      numero: schema.liability_installments.installment_number,
      vencimento: schema.liability_installments.due_date,
      valor: schema.liability_installments.total_amount,
      valorPago: schema.liability_installments.paid_amount,
      dataPagamento: schema.liability_installments.paid_date,
      status: schema.liability_installments.status,
    })
    .from(schema.liability_installments)
    .where(
      and(
        eq(schema.liability_installments.liability_id, id),
        eq(schema.liability_installments.user_id, ownerId),
      ),
    )
    .orderBy(asc(schema.liability_installments.installment_number));

  const resumo = montarResumo(row, parcelasRows, hojeIso);

  let previaCronograma: PreviaCronograma | null = null;
  if (parcelasRows.length === 0) {
    const resolved = resolverConfigAmortizacao({
      original: num(row.original),
      taxa: row.taxa != null ? num(row.taxa) : null,
      tipoTaxa: row.tipoTaxa,
      sistema: row.sistema,
      parcelasTotais: row.parcelasTotais,
      inicio: row.inicio,
    });
    if (resolved) {
      const schedule = generateSchedule(resolved.config);
      previaCronograma = {
        parcelas: schedule.length,
        primeiraParcela: schedule[0]?.totalAmount ?? 0,
        premissa: resolved.premissa,
      };
    }
  }

  return {
    ...resumo,
    produtoNome: row.produto,
    instituicaoNome: row.instituicao,
    fornecedorNome: row.fornecedor,
    financialProductId: row.financialProductId,
    parcelas: parcelasRows.map((p) => ({
      id: p.id,
      numero: p.numero,
      vencimento: p.vencimento,
      valor: num(p.valor),
      valorPago: num(p.valorPago),
      dataPagamento: p.dataPagamento,
      status: p.status,
      situacao: situacaoParcela(p.status, p.vencimento, hojeIso),
    })),
    previaCronograma,
  };
}

// ══════════════════════════════════════════════════════════════
// Escrita — só o que vai além do CRUD de uma linha (ver cabeçalho do arquivo)
// ══════════════════════════════════════════════════════════════

/**
 * Executa as escritas atomicamente. Duplicado de
 * `apps/web/src/app/actions/financial-centers.ts::gravaAtomicamente` (não dá
 * pra importar de lá: é uma função não exportada, e action já existente está
 * fora do que este branch pode alterar). Mesmo motivo de existir: o driver
 * HTTP da Neon (produção) só tem `batch()` e lança em `transaction()`; o
 * driver `pg` (testes de integração locais, ver packages/db/src/client.ts)
 * só tem `transaction()`. Sem os dois caminhos, ou a produção quebra ou os
 * testes quebram.
 */
async function gravaAtomicamente(
  monta: (executor: Db) => readonly [unknown, ...unknown[]],
): Promise<void> {
  const db = getDb();
  const suportaBatch = typeof (db as { batch?: unknown }).batch === "function";

  if (suportaBatch) {
    await (db as unknown as { batch: (b: readonly unknown[]) => Promise<unknown> }).batch(
      monta(db),
    );
    return;
  }

  await db.transaction(async (tx) => {
    for (const comando of monta(tx as unknown as Db)) {
      await (comando as Promise<unknown>);
    }
  });
}

/**
 * Gera de uma vez o cronograma inteiro de uma dívida usando a amortização de
 * `@sbf/domain` (SAC/Price/Misto — a mesma lógica testada que qualquer outra
 * tela do produto usaria; nada aqui recalcula a fórmula).
 *
 * Recusa gerar se a dívida já tem alguma parcela lançada (idempotência: não
 * duplica cronograma) ou se faltam os dados mínimos (total de parcelas, data
 * de início). Uma única instrução `insert(...).values([...])` já é atômica
 * por si só — não precisa de `gravaAtomicamente` aqui, ao contrário de
 * `marcarParcelaComoPaga`, que grava em duas tabelas.
 */
export async function gerarCronogramaAutomatico(liabilityId: string): Promise<{ criadas: number }> {
  const userId = await requireUserId();
  const db = getDb();

  const [liability] = await db
    .select()
    .from(schema.liabilities)
    .where(and(eq(schema.liabilities.id, liabilityId), eq(schema.liabilities.user_id, userId)));
  if (!liability) throw new Error("Dívida não encontrada");

  const existentes = await db
    .select({ id: schema.liability_installments.id })
    .from(schema.liability_installments)
    .where(
      and(
        eq(schema.liability_installments.liability_id, liabilityId),
        eq(schema.liability_installments.user_id, userId),
      ),
    );
  if (existentes.length > 0) {
    throw new Error(
      "Esta dívida já tem parcelas cadastradas — o cronograma automático só vale para dívidas sem nenhuma parcela lançada.",
    );
  }

  const resolved = resolverConfigAmortizacao({
    original: num(liability.original_amount),
    taxa: liability.interest_rate != null ? num(liability.interest_rate) : null,
    tipoTaxa: liability.rate_type,
    sistema: liability.amortization_system,
    parcelasTotais: liability.total_installments,
    inicio: liability.start_date,
  });
  if (!resolved) {
    throw new Error(
      "Para gerar o cronograma automaticamente, defina o total de parcelas e a data de início da dívida.",
    );
  }
  const dataInicio = liability.start_date;
  if (!dataInicio) throw new Error("Data de início ausente");

  const schedule = generateSchedule(resolved.config);
  const linhas = schedule.map((parcela) => ({
    user_id: userId,
    liability_id: liabilityId,
    installment_number: parcela.installmentNumber,
    due_date: somaMesesISO(dataInicio, parcela.installmentNumber - 1),
    total_amount: parcela.totalAmount,
    principal_amount: parcela.principalAmount,
    interest_amount: parcela.interestAmount,
    insurance_amount: parcela.insuranceAmount > 0 ? parcela.insuranceAmount : null,
    fee_amount: parcela.feeAmount > 0 ? parcela.feeAmount : null,
    status: "pending" as const,
  }));

  await db.insert(schema.liability_installments).values(linhas);

  return { criadas: linhas.length };
}

/**
 * Marca uma parcela como paga e abate o saldo devedor da dívida — sem isso o
 * "quanto falta" do topo da tela nunca se moveria conforme o usuário fosse
 * pagando. Grava as duas tabelas atomicamente via `gravaAtomicamente`.
 *
 * Simplificação assumida quando a parcela não veio de um cronograma gerado
 * pela calculadora (`principal_amount` nulo): o valor pago inteiro é tratado
 * como amortização de principal, por falta de um detalhamento melhor.
 *
 * Idempotente: chamar de novo numa parcela já paga não faz nada.
 */
export async function marcarParcelaComoPaga(
  installmentId: string,
  input?: { valorPago?: number; dataPagamento?: string },
): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const [parcela] = await db
    .select()
    .from(schema.liability_installments)
    .where(
      and(
        eq(schema.liability_installments.id, installmentId),
        eq(schema.liability_installments.user_id, userId),
      ),
    );
  if (!parcela) throw new Error("Parcela não encontrada");
  if (parcela.status === "paid") return;

  const [divida] = await db
    .select()
    .from(schema.liabilities)
    .where(
      and(eq(schema.liabilities.id, parcela.liability_id), eq(schema.liabilities.user_id, userId)),
    );
  if (!divida) throw new Error("Dívida não encontrada");

  const valorPago = input?.valorPago ?? num(parcela.total_amount);
  const dataPagamento = input?.dataPagamento ?? isoHoje();
  const principalAbatido =
    parcela.principal_amount != null ? num(parcela.principal_amount) : valorPago;
  const novoSaldo = Math.max(0, num(divida.outstanding_balance) - principalAbatido);
  const novasPagas = (divida.paid_installments ?? 0) + 1;
  const quitada =
    novoSaldo <= 0 ||
    (divida.total_installments != null && novasPagas >= divida.total_installments);

  await gravaAtomicamente(
    (executor) =>
      [
        executor
          .update(schema.liability_installments)
          .set({ status: "paid", paid_amount: valorPago, paid_date: dataPagamento })
          .where(
            and(
              eq(schema.liability_installments.id, installmentId),
              eq(schema.liability_installments.user_id, userId),
            ),
          ),
        executor
          .update(schema.liabilities)
          .set({
            outstanding_balance: novoSaldo,
            paid_installments: novasPagas,
            status: quitada ? "paid_off" : divida.status,
          })
          .where(and(eq(schema.liabilities.id, divida.id), eq(schema.liabilities.user_id, userId))),
      ] as const,
  );
}
