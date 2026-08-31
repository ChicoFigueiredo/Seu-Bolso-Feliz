/**
 * @sbf/mcp-server — Tool: recompute_financial_periods
 *
 * Substitui a chamada à RPC `generate_financial_periods`: uma das seis
 * funções descartadas por inteiro na migração para o Neon (não amarrava
 * mais nada além de `auth.uid()` — ver NOTA em packages/db/schema.sql;
 * confirmado ausente em `pg_proc` do Neon).
 *
 * A aritmética do ciclo (`generatePeriods`) é a MESMA usada por
 * `apps/web/src/app/actions/financial-periods.ts::generateFinancialPeriods`
 * — ambas importam de `@sbf/domain`, então o cálculo não é duplicado, só a
 * casca de persistência (que aqui não pode reaproveitar a Server Action:
 * ela depende de `requireUser()`/cookies do Next.js, que não existem num
 * processo stdio; o MCP resolve `userId` via `LOCAL_USER_ID`).
 */
import { eq, sql } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
import { generatePeriods } from "@sbf/domain";

export interface RecomputeResult {
  periodsCreated: number;
  startDay: number;
  monthsAhead: number;
}

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

export async function recomputeFinancialPeriods(
  db: Database,
  userId: string,
  opts?: { startDay?: number; monthsAhead?: number },
): Promise<RecomputeResult> {
  // Buscar preferências do usuário se startDay não fornecido
  let resolvedStartDay: number;
  if (opts?.startDay) {
    resolvedStartDay = opts.startDay;
  } else {
    const [prefs] = await db
      .select({
        financial_cycle_start_day: schema.user_financial_preferences.financial_cycle_start_day,
      })
      .from(schema.user_financial_preferences)
      .where(eq(schema.user_financial_preferences.user_id, userId));

    if (!prefs?.financial_cycle_start_day) {
      throw new Error(
        "Preferências financeiras não encontradas para o usuário. Configure financial_cycle_start_day primeiro.",
      );
    }
    resolvedStartDay = prefs.financial_cycle_start_day;
  }

  const monthsAhead = opts?.monthsAhead ?? 12;

  const ancora = parseIsoDateLocal(toIsoDateLocal(new Date()));
  const periodos = generatePeriods(
    { startDay: resolvedStartDay, anchorDate: ancora },
    monthsAhead,
  ).map((p) => ({
    start_date: toIsoDateLocal(p.startDate),
    end_date: toIsoDateLocal(p.endDate),
    label: p.label,
  }));

  // Checagem de duplicata em código, igual ao plpgsql original: não há
  // unique constraint em (user_id, start_date, end_date), então isto não é
  // à prova de corrida entre duas chamadas simultâneas do mesmo usuário —
  // herdado, não introduzido aqui.
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
        // Deliberadamente `false`: o UPDATE abaixo cobre todos os períodos
        // do usuário — inclusive estes — com o CURRENT_DATE do banco,
        // exatamente como o plpgsql original fazia ao final.
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

  return { periodsCreated: novos.length, startDay: resolvedStartDay, monthsAhead };
}
