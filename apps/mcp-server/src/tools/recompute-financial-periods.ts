/**
 * @sbf/mcp-server — Tool: recompute_financial_periods
 * Invoca a RPC generate_financial_periods do Supabase para gerar/atualizar
 * os períodos financeiros do usuário.
 */
import type { SupabaseClient } from "@supabase/supabase-js";

export interface RecomputeResult {
  periodsCreated: number;
  startDay: number;
  monthsAhead: number;
}

export async function recomputeFinancialPeriods(
  supabase: SupabaseClient,
  userId: string,
  opts?: { startDay?: number; monthsAhead?: number },
): Promise<RecomputeResult> {
  // Buscar preferências do usuário se startDay não fornecido
  let resolvedStartDay: number;
  if (opts?.startDay) {
    resolvedStartDay = opts.startDay;
  } else {
    const { data: prefs, error: prefsError } = await supabase
      .from("user_financial_preferences")
      .select("financial_cycle_start_day")
      .eq("user_id", userId)
      .single();

    if (prefsError || !prefs) {
      throw new Error(
        `Preferências financeiras não encontradas para o usuário. Configure financial_cycle_start_day primeiro. (${prefsError?.message ?? "no data"})`,
      );
    }
    resolvedStartDay = Number(prefs.financial_cycle_start_day);
  }

  const monthsAhead = opts?.monthsAhead ?? 12;

  // Chamar a RPC do Supabase
  const { data, error } = await supabase.rpc("generate_financial_periods", {
    start_day: resolvedStartDay,
    months_ahead: monthsAhead,
  });

  if (error) {
    throw new Error(`Erro ao gerar períodos financeiros: ${error.message}`);
  }

  return {
    periodsCreated: typeof data === "number" ? data : 0,
    startDay: resolvedStartDay,
    monthsAhead,
  };
}
