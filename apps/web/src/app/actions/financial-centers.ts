"use server";

/**
 * Centros e rateios.
 *
 * Vocabulário (CONTEXT.md — não use um no lugar do outro):
 * - **Centro**: *para quem* ou *para qual projeto* o dinheiro foi. Tem saldo,
 *   entradas e saídas, e se comporta como conta corrente virtual.
 * - **Rateio**: a divisão do valor de uma movimentação entre centros. Guarda
 *   **valor**, nunca percentual.
 * - **Não alocado**: a parte do valor que ainda não foi rateada. Estado
 *   legítimo e visível — a soma dos rateios pode ser menor que a movimentação,
 *   nunca maior.
 *
 * Segurança: não existe mais RLS (o Neon não tem). Toda consulta filtra
 * `user_id` explicitamente por `ownerId` (o dono dos dados, não o ator), e
 * toda FK que vem do cliente (`transaction_id`, `financial_center_id`) passa
 * por checagem de posse antes de qualquer escrita — o repo já fechou 16 IDORs
 * por não fazer exatamente isso.
 */

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, gte, inArray, lte, sql } from "drizzle-orm";
import {
  canAllocateTransactionType,
  centerBalance,
  fromCents,
  splitEvenly,
  toCents,
  validateAllocations,
  INFLOW_TRANSACTION_TYPES,
  type AllocationInput,
} from "@sbf/domain";
import {
  allocateTransactionSchema,
  createFinancialCenterSchema,
  updateFinancialCenterSchema,
  type CreateFinancialCenterInput,
  type UpdateFinancialCenterInput,
} from "@sbf/validation";

export type FinancialCenter = typeof schema.financial_centers.$inferSelect;
export type TransactionAllocation = typeof schema.transaction_allocations.$inferSelect;

export interface FinancialCenterWithBalance extends FinancialCenter {
  /** Quanto entrou no centro, desde sempre. */
  inflow: number;
  /** Quanto saiu do centro, desde sempre. */
  outflow: number;
  /** Entradas menos saídas — o saldo da conta corrente virtual. */
  balance: number;
  /** Entradas dentro do período consultado. */
  monthInflow: number;
  /** Saídas dentro do período consultado. */
  monthOutflow: number;
  /** Resultado do mês: entrou menos saiu dentro do período. */
  monthResult: number;
}

export interface TransactionAllocationsSummary {
  transactionId: string;
  /** Valor da movimentação, com o sinal que ela tem no ledger. */
  transactionAmount: number;
  /** Soma dos rateios. */
  allocated: number;
  /** Parte ainda não rateada — pode ser maior que zero, e isso não é erro. */
  unallocated: number;
  allocations: TransactionAllocation[];
}

export interface MigrateTagsToCentersResult {
  centers: Array<{ id: string; name: string; created: boolean; fromTagIds: string[] }>;
  /** Rateios efetivamente gravados. */
  allocationsCreated: number;
  /** Movimentações que ganharam pelo menos um rateio. */
  transactionsAllocated: number;
  /** Já tinham rateio — a migração nunca sobrescreve trabalho manual. */
  transactionsSkippedAlreadyAllocated: number;
  /** Pagamento de fatura, que por regra não rateia. */
  transactionsSkippedNotAllocatable: number;
  /** Valor zero (ou que arredonda para zero centavo): não há o que ratear. */
  transactionsSkippedZeroAmount: number;
}

// ══════════════════════════════════════════════════════════════
// Infraestrutura interna (não exportada: em arquivo "use server" só função
// async pode ser exportada)
// ══════════════════════════════════════════════════════════════

type Db = ReturnType<typeof getDb>;

/**
 * Quantas linhas por consulta ao montar `IN (...)`. Evita estourar o limite
 * de parâmetros do Postgres quando a migração varre milhares de movimentações.
 */
const TAMANHO_LOTE = 500;

function emLotes<T>(items: readonly T[], tamanho = TAMANHO_LOTE): T[][] {
  const lotes: T[][] = [];
  for (let i = 0; i < items.length; i += tamanho) lotes.push(items.slice(i, i + tamanho));
  return lotes;
}

/**
 * Executa as escritas numa transação de banco só.
 *
 * `getDb()` devolve o driver HTTP da Neon em produção e o driver `pg` quando
 * a `DATABASE_URL` aponta para localhost (ver packages/db/src/client.ts). Os
 * dois oferecem atomicidade, mas não pela mesma porta: o HTTP da Neon só tem
 * `batch()` (uma requisição, uma transação) e **lança** em `transaction()`; o
 * `pg` só tem `transaction()` e não define `batch()`. Sem este desvio, ou a
 * produção quebra ou os testes de integração quebram.
 */
async function gravaAtomicamente(
  monta: (executor: Db) => readonly [unknown, ...unknown[]],
): Promise<void> {
  const db = getDb();
  const suportaBatch = typeof (db as { batch?: unknown }).batch === "function";

  if (suportaBatch) {
    // O tipo de `batch` é uma tupla de BatchItem; os comandos montados aqui
    // são exatamente isso, mas o compilador não consegue estreitar a tupla
    // devolvida por `monta`.
    await (db as unknown as { batch: (b: readonly unknown[]) => Promise<unknown> }).batch(
      monta(db),
    );
    return;
  }

  await db.transaction(async (tx) => {
    // Mesmo motivo do cast em packages/db/src/client.ts: os dois drivers
    // expõem a mesma API de query builder, só o transporte muda, e a união
    // dos dois tipos colapsaria os overloads de `.values()`/`.returning()`.
    for (const comando of monta(tx as unknown as Db)) {
      await (comando as Promise<unknown>);
    }
  });
}

/**
 * Primeiro e último dia do mês civil que contém `referencia`, no formato de
 * `transactions.event_date` (`date`, sem fuso).
 *
 * Formata a partir dos componentes locais em vez de `toISOString()`: o ISO
 * converte para UTC e, num servidor a oeste de Greenwich, o dia 1º vira o
 * último dia do mês anterior.
 */
function mesCivilDe(referencia: Date): { startDate: string; endDate: string } {
  const paraIso = (data: Date) =>
    [
      String(data.getFullYear()).padStart(4, "0"),
      String(data.getMonth() + 1).padStart(2, "0"),
      String(data.getDate()).padStart(2, "0"),
    ].join("-");
  const ano = referencia.getFullYear();
  const mes = referencia.getMonth();
  return {
    startDate: paraIso(new Date(ano, mes, 1)),
    endDate: paraIso(new Date(ano, mes + 1, 0)),
  };
}

/**
 * Confere que todos os centros pertencem ao usuário — e que nenhum está
 * arquivado, porque centro arquivado não recebe rateio novo.
 */
async function requireFinancialCentersOwned(
  centerIds: readonly string[],
  ownerId: string,
): Promise<void> {
  const unicos = [...new Set(centerIds)];
  if (unicos.length === 0) return;

  const db = getDb();
  const linhas = await db
    .select({
      id: schema.financial_centers.id,
      name: schema.financial_centers.name,
      status: schema.financial_centers.status,
    })
    .from(schema.financial_centers)
    .where(
      and(
        inArray(schema.financial_centers.id, unicos),
        eq(schema.financial_centers.user_id, ownerId),
      ),
    );

  if (linhas.length !== unicos.length) {
    throw new Error("Um ou mais centros não foram encontrados");
  }
  const arquivado = linhas.find((linha) => linha.status === "archived");
  if (arquivado) {
    throw new Error(`O centro "${arquivado.name}" está arquivado e não aceita rateio novo`);
  }
}

/** Confere posse da movimentação e devolve o que o rateio precisa saber dela. */
async function carregaMovimentacaoDoUsuario(
  transactionId: string,
  ownerId: string,
): Promise<{ id: string; amount: number; type: string }> {
  const db = getDb();
  const [linha] = await db
    .select({
      id: schema.transactions.id,
      amount: schema.transactions.amount,
      type: schema.transactions.type,
    })
    .from(schema.transactions)
    .where(
      and(eq(schema.transactions.id, transactionId), eq(schema.transactions.user_id, ownerId)),
    );
  if (!linha) throw new Error("Movimentação não encontrada");
  return linha;
}

// ══════════════════════════════════════════════════════════════
// CRUD de Centro
// ══════════════════════════════════════════════════════════════

export async function listFinancialCenters(filters?: {
  status?: "active" | "archived";
  type?: "projeto" | "pessoa" | "negocio";
}): Promise<FinancialCenter[]> {
  const { ownerId } = await requireUser();
  const db = getDb();

  const condicoes = [eq(schema.financial_centers.user_id, ownerId)];
  if (filters?.status) condicoes.push(eq(schema.financial_centers.status, filters.status));
  if (filters?.type) condicoes.push(eq(schema.financial_centers.type, filters.type));

  return db
    .select()
    .from(schema.financial_centers)
    .where(and(...condicoes))
    .orderBy(asc(schema.financial_centers.name));
}

export async function getFinancialCenter(id: string): Promise<FinancialCenter | null> {
  const { ownerId } = await requireUser();
  const db = getDb();
  const [linha] = await db
    .select()
    .from(schema.financial_centers)
    .where(and(eq(schema.financial_centers.id, id), eq(schema.financial_centers.user_id, ownerId)));
  return linha ?? null;
}

export async function createFinancialCenter(
  input: CreateFinancialCenterInput,
): Promise<FinancialCenter> {
  const { ownerId } = await requireUser();
  const dados = createFinancialCenterSchema.parse(input);
  const db = getDb();

  const [linha] = await db
    .insert(schema.financial_centers)
    .values({ ...dados, user_id: ownerId })
    .returning();
  if (!linha) throw new Error("Falha ao criar centro");
  return linha;
}

export async function updateFinancialCenter(
  id: string,
  input: UpdateFinancialCenterInput,
): Promise<FinancialCenter> {
  const { ownerId } = await requireUser();
  const dados = updateFinancialCenterSchema.parse(input);
  const db = getDb();

  const [linha] = await db
    .update(schema.financial_centers)
    .set({ ...dados, updated_at: new Date().toISOString() })
    .where(and(eq(schema.financial_centers.id, id), eq(schema.financial_centers.user_id, ownerId)))
    .returning();
  if (!linha) throw new Error("Centro não encontrado");
  return linha;
}

/** Arquiva o centro: some das listas de escolha, mas o histórico continua. */
export async function archiveFinancialCenter(id: string): Promise<FinancialCenter> {
  return updateFinancialCenter(id, { status: "archived" });
}

export async function unarchiveFinancialCenter(id: string): Promise<FinancialCenter> {
  return updateFinancialCenter(id, { status: "active" });
}

/**
 * Exclui o centro de vez. A FK é `on delete restrict`: um centro com rateio
 * não pode sumir sem levar junto a história do dinheiro que passou por ele —
 * nesse caso o caminho é arquivar.
 */
export async function deleteFinancialCenter(id: string): Promise<void> {
  const { ownerId } = await requireUser();
  const db = getDb();

  const [rateio] = await db
    .select({ id: schema.transaction_allocations.id })
    .from(schema.transaction_allocations)
    .where(
      and(
        eq(schema.transaction_allocations.financial_center_id, id),
        eq(schema.transaction_allocations.user_id, ownerId),
      ),
    )
    .limit(1);
  if (rateio) {
    throw new Error(
      "Este centro já tem rateio e não pode ser excluído. Arquive o centro para tirá-lo das listas sem perder o histórico.",
    );
  }

  const removidos = await db
    .delete(schema.financial_centers)
    .where(and(eq(schema.financial_centers.id, id), eq(schema.financial_centers.user_id, ownerId)))
    .returning({ id: schema.financial_centers.id });
  if (removidos.length === 0) throw new Error("Centro não encontrado");
}

// ══════════════════════════════════════════════════════════════
// Listagem com saldo
// ══════════════════════════════════════════════════════════════

/**
 * Centros com saldo, entradas, saídas e resultado do mês.
 *
 * `startDate`/`endDate` delimitam o "mês". O padrão é o mês civil corrente;
 * quem usa ciclo financeiro personalizado (ver `@sbf/domain/financial-cycle`)
 * passa as datas do período.
 */
export async function listFinancialCentersWithBalance(options?: {
  status?: "active" | "archived";
  startDate?: string;
  endDate?: string;
}): Promise<FinancialCenterWithBalance[]> {
  const { ownerId } = await requireUser();
  const db = getDb();

  const mes = mesCivilDe(new Date());
  const startDate = options?.startDate ?? mes.startDate;
  const endDate = options?.endDate ?? mes.endDate;

  // A mesma regra de direção do domínio (`allocationDirection`), montada a
  // partir da lista exportada em vez de repetir os literais no SQL.
  const ehEntrada = inArray(schema.transactions.type, [...INFLOW_TRANSACTION_TYPES]);
  const noPeriodo = and(
    gte(schema.transactions.event_date, startDate),
    lte(schema.transactions.event_date, endDate),
  );
  const valor = schema.transaction_allocations.amount;

  // `::text` em vez de `::float8`: `sum(numeric)` é decimal exato no
  // Postgres, e o texto chega sem uma rodada extra de arredondamento binário.
  const soma = (condicao: ReturnType<typeof and>) =>
    sql<string>`coalesce(sum(${valor}) filter (where ${condicao}), 0)::text`;

  const linhas = await db
    .select({
      center: schema.financial_centers,
      inflow: soma(ehEntrada),
      outflow: soma(sql`not (${ehEntrada})`),
      monthInflow: soma(and(ehEntrada, noPeriodo)),
      monthOutflow: soma(and(sql`not (${ehEntrada})`, noPeriodo)),
    })
    .from(schema.financial_centers)
    .leftJoin(
      schema.transaction_allocations,
      and(
        eq(schema.transaction_allocations.financial_center_id, schema.financial_centers.id),
        eq(schema.transaction_allocations.user_id, ownerId),
      ),
    )
    .leftJoin(
      schema.transactions,
      and(
        eq(schema.transactions.id, schema.transaction_allocations.transaction_id),
        eq(schema.transactions.user_id, ownerId),
      ),
    )
    .where(
      and(
        eq(schema.financial_centers.user_id, ownerId),
        ...(options?.status ? [eq(schema.financial_centers.status, options.status)] : []),
      ),
    )
    .groupBy(schema.financial_centers.id)
    .orderBy(asc(schema.financial_centers.name));

  return linhas.map((linha) => {
    const total = centerBalance(Number(linha.inflow), Number(linha.outflow));
    const doMes = centerBalance(Number(linha.monthInflow), Number(linha.monthOutflow));
    return {
      ...linha.center,
      inflow: total.inflow,
      outflow: total.outflow,
      balance: total.balance,
      monthInflow: doMes.inflow,
      monthOutflow: doMes.outflow,
      monthResult: doMes.balance,
    };
  });
}

// ══════════════════════════════════════════════════════════════
// Rateio
// ══════════════════════════════════════════════════════════════

export async function getTransactionAllocations(
  transactionId: string,
): Promise<TransactionAllocationsSummary> {
  const { ownerId } = await requireUser();
  const movimentacao = await carregaMovimentacaoDoUsuario(transactionId, ownerId);
  const db = getDb();

  const allocations = await db
    .select()
    .from(schema.transaction_allocations)
    .where(
      and(
        eq(schema.transaction_allocations.transaction_id, transactionId),
        eq(schema.transaction_allocations.user_id, ownerId),
      ),
    )
    .orderBy(asc(schema.transaction_allocations.created_at));

  const validacao = validateAllocations(
    movimentacao.amount,
    allocations.map((linha) => ({
      financialCenterId: linha.financial_center_id,
      amount: linha.amount,
    })),
  );

  return {
    transactionId,
    transactionAmount: movimentacao.amount,
    allocated: validacao.allocated,
    unallocated: validacao.unallocated,
    allocations,
  };
}

/**
 * Substitui **todo** o conjunto de rateios de uma movimentação, numa
 * transação de banco só. Lista vazia apaga os rateios existentes.
 *
 * Ordem das checagens (todas antes de qualquer escrita):
 * 1. formato do payload (Zod);
 * 2. posse da movimentação — e o tipo dela, porque pagar fatura de cartão não
 *    rateia: as compras dentro da fatura já foram rateadas uma a uma;
 * 3. posse de todos os centros (e nenhum arquivado);
 * 4. a regra do domínio: valores positivos, sem centro repetido, soma menor ou
 *    igual ao valor da movimentação.
 */
export async function allocateTransaction(
  transactionId: string,
  allocations: AllocationInput[],
): Promise<TransactionAllocationsSummary> {
  const { ownerId } = await requireUser();
  const payload = allocateTransactionSchema.parse({ transactionId, allocations });

  const movimentacao = await carregaMovimentacaoDoUsuario(payload.transactionId, ownerId);
  if (!canAllocateTransactionType(movimentacao.type)) {
    throw new Error(
      "Pagamento de fatura não aceita rateio: as compras dentro da fatura já foram rateadas.",
    );
  }

  await requireFinancialCentersOwned(
    payload.allocations.map((rateio) => rateio.financialCenterId),
    ownerId,
  );

  const validacao = validateAllocations(movimentacao.amount, payload.allocations);
  if (!validacao.ok) {
    throw new Error(validacao.issues.map((problema) => problema.message).join(" "));
  }

  // Grava o valor já arredondado ao centavo — o mesmo número que a validação
  // usou. Sem isso, `numeric(14,2)` arredondaria por conta própria e a soma
  // gravada poderia divergir da soma validada.
  const linhas = payload.allocations.map((rateio) => ({
    user_id: ownerId,
    transaction_id: payload.transactionId,
    financial_center_id: rateio.financialCenterId,
    amount: fromCents(toCents(rateio.amount)),
    note: rateio.note ?? null,
  }));

  await gravaAtomicamente((executor) => {
    const apaga = executor
      .delete(schema.transaction_allocations)
      .where(
        and(
          eq(schema.transaction_allocations.transaction_id, payload.transactionId),
          eq(schema.transaction_allocations.user_id, ownerId),
        ),
      );
    return linhas.length === 0
      ? [apaga]
      : [apaga, executor.insert(schema.transaction_allocations).values(linhas)];
  });

  return getTransactionAllocations(payload.transactionId);
}

/**
 * Divide o valor da movimentação igualmente entre os centros e grava.
 * Açúcar em cima de `allocateTransaction` — a divisão em si é pura
 * (`splitEvenly`), e o resto de centavo vai para o primeiro centro.
 */
export async function allocateTransactionEvenly(
  transactionId: string,
  centerIds: string[],
): Promise<TransactionAllocationsSummary> {
  const { ownerId } = await requireUser();
  const movimentacao = await carregaMovimentacaoDoUsuario(transactionId, ownerId);
  return allocateTransaction(transactionId, splitEvenly(movimentacao.amount, centerIds));
}

// ══════════════════════════════════════════════════════════════
// Migração de Tags para Centros
// ══════════════════════════════════════════════════════════════

/**
 * As cinco tags que viram Centro. As demais continuam tags — tag é rótulo
 * livre, sem valor e sem saldo; centro tem saldo e aceita rateio.
 */
const TAGS_QUE_VIRAM_CENTRO = [
  { name: "Edulingo", type: "projeto" },
  { name: "Imob Max", type: "negocio" },
  { name: "Daiane", type: "pessoa" },
  { name: "IA", type: "projeto" },
  { name: "Seu Bolso Feliz", type: "projeto" },
] as const;

/** Compara nomes ignorando caixa, acento e espaço repetido. */
function normalizaNome(nome: string): string {
  return nome
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .trim()
    .replace(/\s+/g, " ")
    .toLowerCase();
}

/**
 * Converte as tags Edulingo / Imob Max / Daiane / IA / Seu Bolso Feliz em
 * Centros e cria o rateio de 100% do valor para as movimentações que tinham
 * a tag.
 *
 * **Idempotente**: rodar duas vezes não duplica nada. O centro é reaproveitado
 * pela unicidade `(user_id, name)`, e uma movimentação que já tem qualquer
 * rateio é pulada — a migração nunca sobrescreve o que foi feito à mão.
 *
 * As tags **continuam existindo** e continuam ligadas às movimentações: elas
 * seguem servindo para achar coisas. O que muda é que agora existe um centro
 * medindo o dinheiro.
 *
 * Movimentação com mais de uma dessas tags não pode receber 100% para cada
 * uma (a soma passaria do valor). Nesse caso o valor é dividido igualmente
 * entre os centros, com o resto de centavo indo para o primeiro.
 */
export async function migrateTagsToCenters(): Promise<MigrateTagsToCentersResult> {
  const { ownerId } = await requireUser();
  const db = getDb();

  // ── 1. Achar as tags do usuário que casam com os cinco nomes ──
  const tagsDoUsuario = await db
    .select({ id: schema.tags.id, name: schema.tags.name })
    .from(schema.tags)
    .where(eq(schema.tags.user_id, ownerId));

  const porNomeCanonico = new Map<string, { tagIds: string[]; type: string }>();
  for (const tag of tagsDoUsuario) {
    const alvo = TAGS_QUE_VIRAM_CENTRO.find(
      (centro) => normalizaNome(centro.name) === normalizaNome(tag.name),
    );
    if (!alvo) continue;
    const acumulado = porNomeCanonico.get(alvo.name) ?? { tagIds: [], type: alvo.type };
    acumulado.tagIds.push(tag.id);
    porNomeCanonico.set(alvo.name, acumulado);
  }

  const resultado: MigrateTagsToCentersResult = {
    centers: [],
    allocationsCreated: 0,
    transactionsAllocated: 0,
    transactionsSkippedAlreadyAllocated: 0,
    transactionsSkippedNotAllocatable: 0,
    transactionsSkippedZeroAmount: 0,
  };
  if (porNomeCanonico.size === 0) return resultado;

  // ── 2. Criar (ou reaproveitar) o Centro de cada nome ──
  const centroPorNome = new Map<string, string>();
  for (const [name, { tagIds, type }] of porNomeCanonico) {
    const [criado] = await db
      .insert(schema.financial_centers)
      .values({ user_id: ownerId, name, type, description: `Convertido da tag "${name}"` })
      .onConflictDoNothing({
        target: [schema.financial_centers.user_id, schema.financial_centers.name],
      })
      .returning({ id: schema.financial_centers.id });

    let centroId = criado?.id;
    if (!centroId) {
      const [existente] = await db
        .select({ id: schema.financial_centers.id })
        .from(schema.financial_centers)
        .where(
          and(
            eq(schema.financial_centers.user_id, ownerId),
            eq(schema.financial_centers.name, name),
          ),
        );
      if (!existente) throw new Error(`Falha ao criar ou localizar o centro "${name}"`);
      centroId = existente.id;
    }

    centroPorNome.set(name, centroId);
    resultado.centers.push({
      id: centroId,
      name,
      created: Boolean(criado),
      fromTagIds: tagIds,
    });
  }

  // ── 3. Movimentações marcadas com essas tags ──
  const todasAsTagIds = [...porNomeCanonico.values()].flatMap((entrada) => entrada.tagIds);
  const nomePorTagId = new Map<string, string>();
  for (const [name, { tagIds }] of porNomeCanonico) {
    for (const tagId of tagIds) nomePorTagId.set(tagId, name);
  }

  const marcadas: Array<{ transactionId: string; tagId: string; amount: number; type: string }> =
    [];
  for (const lote of emLotes(todasAsTagIds)) {
    const linhas = await db
      .select({
        transactionId: schema.transaction_tags.transaction_id,
        tagId: schema.transaction_tags.tag_id,
        amount: schema.transactions.amount,
        type: schema.transactions.type,
      })
      .from(schema.transaction_tags)
      .innerJoin(
        schema.transactions,
        eq(schema.transactions.id, schema.transaction_tags.transaction_id),
      )
      .where(
        and(
          inArray(schema.transaction_tags.tag_id, lote),
          // Filtro de posse explícito: `transaction_tags` não tem `user_id`.
          eq(schema.transactions.user_id, ownerId),
        ),
      );
    marcadas.push(...linhas);
  }
  if (marcadas.length === 0) return resultado;

  // ── 4. Agrupar por movimentação ──
  const porMovimentacao = new Map<string, { amount: number; type: string; centerIds: string[] }>();
  for (const linha of marcadas) {
    const nomeCentro = nomePorTagId.get(linha.tagId);
    if (!nomeCentro) continue;
    const centroId = centroPorNome.get(nomeCentro);
    if (!centroId) continue;

    const atual = porMovimentacao.get(linha.transactionId) ?? {
      amount: linha.amount,
      type: linha.type,
      centerIds: [],
    };
    // Duas tags diferentes podem apontar para o mesmo centro (ex.: "IA" e
    // "ia"): o centro entra uma vez só.
    if (!atual.centerIds.includes(centroId)) atual.centerIds.push(centroId);
    porMovimentacao.set(linha.transactionId, atual);
  }

  // ── 5. Quem já tem rateio fica como está ──
  const idsCandidatos = [...porMovimentacao.keys()];
  const jaRateadas = new Set<string>();
  for (const lote of emLotes(idsCandidatos)) {
    const linhas = await db
      .select({ transactionId: schema.transaction_allocations.transaction_id })
      .from(schema.transaction_allocations)
      .where(
        and(
          inArray(schema.transaction_allocations.transaction_id, lote),
          eq(schema.transaction_allocations.user_id, ownerId),
        ),
      );
    for (const linha of linhas) jaRateadas.add(linha.transactionId);
  }

  // ── 6. Montar os rateios ──
  const novasLinhas: Array<typeof schema.transaction_allocations.$inferInsert> = [];
  for (const [transactionId, dados] of porMovimentacao) {
    if (jaRateadas.has(transactionId)) {
      resultado.transactionsSkippedAlreadyAllocated += 1;
      continue;
    }
    if (!canAllocateTransactionType(dados.type)) {
      resultado.transactionsSkippedNotAllocatable += 1;
      continue;
    }

    const centavos = Math.abs(toCents(dados.amount));
    if (centavos === 0) {
      resultado.transactionsSkippedZeroAmount += 1;
      continue;
    }

    // 100% do valor quando há um centro só. Com mais de um, 100% para cada
    // estouraria a movimentação — divide igualmente. Quando não sobra um
    // centavo por centro, tudo vai para o primeiro (rateio de valor zero é
    // proibido pelo domínio e pelo banco).
    const primeiroCentro = dados.centerIds[0];
    if (!primeiroCentro) continue;
    const partes: AllocationInput[] =
      dados.centerIds.length === 1 || centavos < dados.centerIds.length
        ? [{ financialCenterId: primeiroCentro, amount: fromCents(centavos) }]
        : splitEvenly(dados.amount, dados.centerIds);

    resultado.transactionsAllocated += 1;
    for (const parte of partes) {
      novasLinhas.push({
        user_id: ownerId,
        transaction_id: transactionId,
        financial_center_id: parte.financialCenterId,
        amount: parte.amount,
        note: "Rateio criado na conversão de tags em centros",
      });
    }
  }

  // ── 7. Gravar ──
  for (const lote of emLotes(novasLinhas)) {
    const gravadas = await db
      .insert(schema.transaction_allocations)
      .values(lote)
      // Rede extra de idempotência: se duas execuções correrem juntas, a
      // unicidade (transaction_id, financial_center_id) decide.
      .onConflictDoNothing({
        target: [
          schema.transaction_allocations.transaction_id,
          schema.transaction_allocations.financial_center_id,
        ],
      })
      .returning({ id: schema.transaction_allocations.id });
    resultado.allocationsCreated += gravadas.length;
  }

  return resultado;
}
