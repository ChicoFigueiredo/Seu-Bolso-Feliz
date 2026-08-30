"use server";

import { requireUser } from "@/lib/auth";
import { generatePeriods } from "@sbf/domain";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq, gte, lte, sql } from "drizzle-orm";

type FinancialPeriod = typeof schema.financial_periods.$inferSelect;
type FinancialPeriodInsert = Omit<typeof schema.financial_periods.$inferInsert, "user_id" | "id">;
type FinancialPeriodUpdate = Partial<FinancialPeriodInsert>;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

// ══════════════════════════════════════════════════════════════
// Financial Periods
// ══════════════════════════════════════════════════════════════

export async function getFinancialPeriods(filters?: {
  isCurrent?: boolean;
  startDate?: string;
  endDate?: string;
}): Promise<FinancialPeriod[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.financial_periods.user_id, userId)];
  if (filters?.isCurrent !== undefined) {
    conditions.push(eq(schema.financial_periods.is_current, filters.isCurrent));
  }
  if (filters?.startDate) {
    conditions.push(gte(schema.financial_periods.start_date, filters.startDate));
  }
  if (filters?.endDate) {
    conditions.push(lte(schema.financial_periods.end_date, filters.endDate));
  }

  return db
    .select()
    .from(schema.financial_periods)
    .where(and(...conditions))
    .orderBy(asc(schema.financial_periods.start_date));
}

export async function getCurrentFinancialPeriod(): Promise<FinancialPeriod | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.financial_periods)
    .where(
      and(
        eq(schema.financial_periods.is_current, true),
        eq(schema.financial_periods.user_id, userId),
      ),
    );
  return row ?? null;
}

export async function createFinancialPeriod(
  input: FinancialPeriodInsert,
): Promise<FinancialPeriod> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.financial_periods)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar período financeiro");
  return row;
}

export async function updateFinancialPeriod(
  id: string,
  input: FinancialPeriodUpdate,
): Promise<FinancialPeriod> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.financial_periods)
    .set(input)
    .where(and(eq(schema.financial_periods.id, id), eq(schema.financial_periods.user_id, userId)))
    .returning();
  if (!row) throw new Error("Período financeiro não encontrado");
  return row;
}

export async function deleteFinancialPeriod(id: string): Promise<void> {
  // Nota: o código Supabase original não faz `.select().single()` aqui — só
  // verifica `error`. 0 linhas afetadas (id inexistente ou de outro usuário)
  // não lança erro hoje; esta reescrita preserva esse comportamento (exceção
  // documentada à regra geral de "lança erro em 0 linhas").
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.financial_periods)
    .where(and(eq(schema.financial_periods.id, id), eq(schema.financial_periods.user_id, userId)));
}

// ══════════════════════════════════════════════════════════════
// Gerar Períodos Financeiros (era a RPC generate_financial_periods)
// ══════════════════════════════════════════════════════════════
//
// `generate_financial_periods` é uma das SEIS funções descartadas por inteiro
// na migração Supabase → Neon (packages/db/schema.sql, NOTA item 4): sabia
// "quem é o usuário atual" só via `auth.uid()`. Confirmado por consulta ao
// `pg_proc` do Neon: NÃO existe lá. Reimplementada aqui em TypeScript, o que
// é preferível a recriá-la no banco — a aritmética de ciclo já existe pura e
// testada em `@sbf/domain` (`generatePeriods`), inclusive o clamp de
// `startDay` para o último dia de meses curtos e o rótulo "DD/MM/YYYY a
// DD/MM/YYYY", idênticos ao plpgsql original.
//
// Duas coisas continuam no banco de propósito:
//   - `is_current` é recalculado com o `CURRENT_DATE` do Postgres (não com o
//     relógio/fuso do processo Node), como o plpgsql fazia;
//   - a checagem de duplicata é a mesma do original (`WHERE NOT EXISTS` por
//     (user_id, start_date, end_date)) — não existe unique constraint nesse
//     trio, então ela é feita em código, e como no plpgsql não é à prova de
//     corrida entre duas chamadas simultâneas do mesmo usuário.

/** `YYYY-MM-DD` → Date no fuso LOCAL. `new Date("2026-08-30")` seria UTC. */
function parseIsoDateLocal(iso: string): Date {
  const [y, m, d] = iso.split("-").map(Number);
  if (!y || !m || !d) throw new Error(`Data inválida: ${iso}`);
  return new Date(y, m - 1, d);
}

/** Date → `YYYY-MM-DD` pelos componentes LOCAIS (`toISOString` converteria a UTC). */
function toIsoDateLocal(date: Date): string {
  const mes = String(date.getMonth() + 1).padStart(2, "0");
  const dia = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${mes}-${dia}`;
}

export async function generateFinancialPeriods(
  startDay: number,
  monthsAhead: number = 12,
  anchorDate?: string,
): Promise<void> {
  const userId = await requireUserId();

  if (!Number.isInteger(startDay) || startDay < 1 || startDay > 31) {
    throw new Error("Dia de início do ciclo deve estar entre 1 e 31");
  }
  if (!Number.isInteger(monthsAhead) || monthsAhead < 1) {
    throw new Error("Quantidade de meses deve ser um inteiro positivo");
  }

  const ancora = parseIsoDateLocal(anchorDate ?? toIsoDateLocal(new Date()));
  const periodos = generatePeriods({ startDay, anchorDate: ancora }, monthsAhead).map((p) => ({
    start_date: toIsoDateLocal(p.startDate),
    end_date: toIsoDateLocal(p.endDate),
    label: p.label,
  }));

  const db = getDb();

  const existentes = await db
    .select({
      start_date: schema.financial_periods.start_date,
      end_date: schema.financial_periods.end_date,
    })
    .from(schema.financial_periods)
    .where(eq(schema.financial_periods.user_id, userId));
  const jaExiste = new Set(existentes.map((p) => `${p.start_date}|${p.end_date}`));

  const novos = periodos.filter((p) => !jaExiste.has(`${p.start_date}|${p.end_date}`));
  if (novos.length > 0) {
    await db.insert(schema.financial_periods).values(
      novos.map((p) => ({
        user_id: userId,
        start_date: p.start_date,
        end_date: p.end_date,
        label: p.label,
        // Deliberadamente `false`: o UPDATE abaixo cobre todos os períodos do
        // usuário — inclusive estes — com o CURRENT_DATE do banco, exatamente
        // como o plpgsql fazia ao final.
        is_current: false,
      })),
    );
  }

  await db
    .update(schema.financial_periods)
    .set({
      is_current: sql`(CURRENT_DATE >= ${schema.financial_periods.start_date} AND CURRENT_DATE <= ${schema.financial_periods.end_date})`,
    })
    .where(eq(schema.financial_periods.user_id, userId));
}

// ══════════════════════════════════════════════════════════════
// Encontrar Período para Data (era a RPC get_financial_period_for_date)
// ══════════════════════════════════════════════════════════════
//
// Também uma das seis descartadas — confirmado ausente no `pg_proc` do Neon.
// O corpo original era um único SELECT com `user_id = auth.uid()`; aqui vira
// uma consulta Drizzle com o filtro por dono explícito. A RPC devolvia só o
// `id` e o call site fazia um segundo SELECT — as duas idas viram uma só, com
// o mesmo contrato de saída (`FinancialPeriod | null`).

export async function getFinancialPeriodForDate(date: string): Promise<FinancialPeriod | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.financial_periods)
    .where(
      and(
        eq(schema.financial_periods.user_id, userId),
        lte(schema.financial_periods.start_date, date),
        gte(schema.financial_periods.end_date, date),
      ),
    )
    .orderBy(desc(schema.financial_periods.start_date))
    .limit(1);
  return row ?? null;
}

// ══════════════════════════════════════════════════════════════
// User Financial Preferences
// ══════════════════════════════════════════════════════════════

export async function getFinancialPreferences() {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, userId));
  return row ?? null;
}

export async function upsertFinancialPreferences(input: {
  financial_cycle_start_day?: number | null;
  financial_cycle_anchor_date?: string | null;
  default_currency?: string;
}) {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.user_financial_preferences)
    .values({ ...input, user_id: userId })
    .onConflictDoUpdate({
      target: schema.user_financial_preferences.user_id,
      set: input,
    })
    .returning();
  return row;
}
